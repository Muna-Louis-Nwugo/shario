//! `SharQueue` mediates between the IDE and the CRDT tree, and backlogs
//! out-of-order remote ops until their dependency arrives.

use crate::shar::core::tree::SharDirectory;
use crate::shar::prelude::*;
use crate::types::{CrdtRelation, IdeAdd, IdeAddConfirmed, NetworkAdd, NetworkOp, Remove};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;

// Pin says (keep this locked in memory)
// The reason we need this is that async functions become self-refernetial structs, and if they're
// moved in memory their references to themselves become stale, which creates undefined behaviour
//
//
// Send tells the compiler that this is safe to move to another thread to free up this one. It
// doesn't necessarily move it, it just says it can be moved
type BoxedFun = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Owns the tree for one shar session and mediates every read/write to it.
#[derive(Default)]
pub struct SharQueue {
    /// IDE adds waiting on a parent that hasn't arrived yet.
    ide_backlog: HashMap<(usize, usize), Vec<IdeAdd>>,
    /// network operations waiting on a target that hasn't arrived yet.
    network_backlog: HashMap<(IdSize, PeerIdSize), Vec<NetworkOp>>,
    /// This replica's peer id.
    peer: PeerIdSize,
    tree: SharDirectory,
    /// Shared across every file in the directory.
    counter: u32,

    /// Called with `(row, col)` when an add is applied.
    pub network_add_callback: Option<Box<dyn Fn(usize, usize) -> BoxedFun + Send + Sync>>,
    /// Called with `(row, col, was_line_merge)` when a remove is applied.
    pub network_remove_callback: Option<Box<dyn Fn(usize, usize) -> BoxedFun + Send + Sync>>,
    pub ide_add_callback:
        Option<Box<dyn Fn(NetworkAdd, IdeAddConfirmed) -> BoxedFun + Send + Sync>>,
    pub ide_remove_callback: Option<Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>>,
}

impl SharQueue {
    /// Loads the shar rooted at `dir_path`, storing `network_add_callback`/
    /// `network_remove_callback` so they're set. A `SharQueue` built any other way
    /// (e.g. `SharQueue::default()`) leaves them `None`, and calling
    /// [`Self::add_network_operation`]/[`Self::remove_network_operation`] on
    /// one will panic.
    pub fn new(
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        network_add_callback: Box<dyn Fn(usize, usize) -> BoxedFun + Send + Sync>,
        network_remove_callback: Box<dyn Fn(usize, usize) -> BoxedFun + Send + Sync>,
        ide_add_callback: Box<dyn Fn(NetworkAdd, IdeAddConfirmed) -> BoxedFun + Send + Sync>,
        ide_remove_callback: Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>,
    ) -> Result<Self> {
        let mut counter: u32 = 0;
        let tree = SharDirectory::new(dir_path, &mut counter)?;
        /* create a new shar queue*/
        let queue = SharQueue {
            ide_backlog: HashMap::new(),
            network_backlog: HashMap::new(),
            peer: this_peer_id,
            counter: counter,
            tree: tree,
            network_add_callback: Some(network_add_callback),
            network_remove_callback: Some(network_remove_callback),
            ide_add_callback: Some(ide_add_callback),
            ide_remove_callback: Some(ide_remove_callback),
        };

        Ok(queue)
    }

    /// Every file's `(id, peer)` projection, keyed by its full path. See
    /// [`SharDirectory::identities`].
    pub fn identities(&self) -> Vec<(PathBuf, Vec<Vec<(IdSize, PeerIdSize)>>)> {
        self.tree.identities()
    }

    /// Applies a locally-typed character and returns the `NetworkAdd` to
    /// send to peers. `start_line` resolves the parent via the line's
    /// start-of-line anchor instead of `(paren:willt_row, parent_col)`.
    pub async fn add_ide_operation(&mut self, op: IdeAdd) -> Result<()> {
        tracing::debug!(?op, "add_ide_operation called");
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let parent_row = op_clone.parent_row;
        let parent_col = op_clone.parent_col;
        let val = op_clone.val;
        let tag = op_clone.tag;

        // find the parent id

        let parent_id_peer = self.tree.get_id_peer(&file_path, (parent_row, parent_col));

        match parent_id_peer {
            Ok(parent) => {
                if let Some(parent_real) = parent {
                    self.counter += 1;
                    let relation = CrdtRelation::new(val, parent_real.0, parent_real.1);
                    let crdt = CRDT::new(self.counter, self.peer, relation);
                    let network_op = NetworkAdd::new(file_path.clone(), crdt.clone(), parent_row);
                    let return_op = IdeAddConfirmed::new(tag, self.counter, self.peer);

                    let add = self.tree.add_crdt(&file_path, parent_row, crdt);

                    match add {
                        Ok(add) => {
                            if let Some(pos) = add {
                                tracing::debug!(
                                    row = pos.0,
                                    col = pos.1,
                                    "add_ide_operation resolved position, clearing dependants"
                                );
                                self.clear_ide_backlog(pos.0, pos.1).await;
                                (self.ide_add_callback.as_ref().unwrap())(network_op, return_op)
                                    .await;
                                Ok(())
                            } else {
                                tracing::debug!(
                                    "add_ide_operation was a no-op, likely already added"
                                );
                                Ok(())
                            }
                        }

                        Err(e) => Err(Error::Generic(String::from(format!(
                            "Something went wrong: {e}"
                        )))),
                    }
                } else {
                    tracing::debug!(
                        parent_row,
                        parent_col,
                        "add_ide_operation backlogged, parent not found yet"
                    );
                    if let Some(backlogged) = self.ide_backlog.get_mut(&(parent_row, parent_col)) {
                        backlogged.push(op);
                    } else {
                        let _ = self.ide_backlog.insert((parent_row, parent_col), vec![op]);
                    }

                    Ok(())
                }
            }

            Err(_e) => {
                tracing::debug!(
                    parent_row,
                    parent_col,
                    "add_ide_operation backlogged, parent not found yet"
                );
                if let Some(backlogged) = self.ide_backlog.get_mut(&(parent_row, parent_col)) {
                    backlogged.push(op);
                } else {
                    let _ = self.ide_backlog.insert((parent_row, parent_col), vec![op]);
                }

                Ok(())
            }
        }
    }

    /// Applies a locally-triggered removal and returns the `NetworkRemove` to
    /// send to peers. `is_whole_line` removes `row`'s line-start anchor
    /// instead of `(row, col)`. Rejects the root sentinel as a target.
    pub async fn remove_ide_operation(&mut self, op: Remove) {
        let operation = self.remove_operation(op.clone());

        if let Some(_) = operation {
            (self.ide_remove_callback.as_ref().unwrap())(op).await;
        }
    }

    /// Applies a remote `NetworkAdd`. If the parent doesn't exist yet, backlogs
    /// `op` instead. On success, replays any backlogged add/remove waiting on it.
    ///
    /// Panics if `network_add_callback` is `None` — only possible if this `SharQueue`
    /// wasn't built via [`Self::new`].
    pub async fn add_network_operation(&mut self, op: NetworkAdd) {
        tracing::debug!(?op, "add_network_operation called");
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    tracing::debug!(
                        row = position.0,
                        col = position.1,
                        "add_network_operation resolved position, clearing dependants"
                    );
                    (self.network_add_callback.as_ref().unwrap())(position.0, position.1).await;

                    self.clear_network_backlog(crdt.id, crdt.peer).await;
                } else {
                    tracing::debug!("add_network_operation was a no-op, likely already added");
                    return;
                }
            }

            Err(e) => {
                tracing::debug!(error = %e, "add_network_operation backlogged, parent not found yet");
                if let Some(backlogged) = self
                    .network_backlog
                    .get_mut(&(crdt.relation.parent_id, crdt.relation.parent_peer))
                {
                    backlogged.push(NetworkOp::ADD(op));
                } else {
                    let _ = self.network_backlog.insert(
                        (crdt.relation.parent_id, crdt.relation.parent_peer),
                        vec![NetworkOp::ADD(op)],
                    );
                }
            }
        };
    }

    /// Applies a remote `NetworkRemove`. If the target doesn't exist yet,
    /// backlogs `op` instead; drained once the matching add arrives (see
    /// [`Self::add_network_operation`]). A retry of an already-removed target
    /// is a no-op.
    ///
    /// Panics if `network_remove_callback` is `None` — only possible if this
    /// `SharQueue` wasn't built via [`Self::new`].
    pub async fn remove_network_operation(&mut self, op: Remove) {
        let operation = self.remove_operation(op);

        if let Some(op_frfr) = operation {
            (self.network_remove_callback.as_ref().unwrap())(op_frfr.0, op_frfr.1).await;
        }
    }

    fn remove_operation(&mut self, op: Remove) -> Option<(usize, usize)> {
        tracing::debug!(?op, "remove_network_operation called");
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let id = op_clone.id;
        let peer = op_clone.peer;
        let row = op_clone.row;

        let removed = self.tree.remove_crdt(&file_path, row, id, peer);

        match removed {
            Ok(pos) => {
                if let Some(val) = pos {
                    tracing::debug!(
                        row = val.0,
                        col = val.1,
                        "remove_network_operation resolved target"
                    );
                    return Some((val.0, val.1));
                } else {
                    // if the remove returns none, the value has already been removed so do nothing
                    tracing::debug!("remove_network_operation target already removed, no-op");
                    return None;
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "remove_network_operation backlogged, target not found yet");
                if let Some(backlogged) = self.network_backlog.get_mut(&(id, peer)) {
                    backlogged.push(NetworkOp::REMOVE(op));
                    return None;
                } else {
                    let _ = self
                        .network_backlog
                        .insert((id, peer), vec![NetworkOp::REMOVE(op)]);
                    return None;
                }
            }
        }
    }

    async fn clear_network_backlog(&mut self, id: IdSize, peer: PeerIdSize) {
        // get the list of dependancies
        let dependancies = self.network_backlog.remove(&(id, peer));

        if let Some(list) = dependancies {
            for item in list {
                match item {
                    NetworkOp::ADD(op) => {
                        Box::pin(self.add_network_operation(op)).await;
                    }

                    NetworkOp::REMOVE(op) => {
                        self.remove_network_operation(op).await;
                    }
                }
            }
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }

    async fn clear_ide_backlog(&mut self, row: usize, col: usize) {
        // get the list of dependancies
        let dependancies = self.ide_backlog.remove(&(row, col));

        if let Some(list) = dependancies {
            for item in list {
                Box::pin(self.add_ide_operation(item)).await;
            }
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }
}
