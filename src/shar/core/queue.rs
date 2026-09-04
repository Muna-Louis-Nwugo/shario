//! `SharQueue` mediates between the IDE and the CRDT tree, and backlogs
//! out-of-order remote ops until their dependency arrives.

use crate::shar::core::tree::SharDirectory;
use crate::shar::prelude::*;
use crate::types::{CrdtRelation, IdeAdd, IdeOp, IdeRemove, NetworkAdd, NetworkOp, NetworkRemove};
use std::collections::HashMap;
use std::path::PathBuf;

/// Owns the tree for one shar session and mediates every read/write to it.
#[derive(Debug, Default)]
pub struct SharQueue {
    /// Remote adds waiting on a parent that hasn't arrived yet.
    ide_backlog: HashMap<(usize, usize), Vec<IdeOp>>,
    /// Remote removes waiting on a target that hasn't arrived yet.
    network_backlog: HashMap<(IdSize, PeerIdSize), Vec<NetworkOp>>,
    /// This replica's peer id.
    peer: PeerIdSize,
    tree: SharDirectory,
    /// Shared across every file in the directory.
    counter: u32,
    /// Called with `(row, col)` when an add is applied.
    pub network_add_callback: Option<fn(usize, usize)>,
    /// Called with `(row, col, was_line_merge)` when a remove is applied.
    pub network_remove_callback: Option<fn(usize, usize, bool)>,
    pub ide_add_callback: Option<fn(NetworkAdd)>,
    pub ide_remove_callback: Option<fn(NetworkRemove)>,
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
        network_add_callback: fn(usize, usize),
        network_remove_callback: fn(usize, usize, bool),
        ide_add_callback: fn(NetworkAdd),
        ide_remove_callback: fn(NetworkRemove),
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

    /// Applies a locally-typed character and returns the `NetworkAdd` to
    /// send to peers. `start_line` resolves the parent via the line's
    /// start-of-line anchor instead of `(paren:willt_row, parent_col)`.
    pub fn add_ide_operation(&mut self, op: IdeAdd) -> Result<()> {
        tracing::debug!(?op, "add_ide_operation");
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let parent_row = op_clone.parent_row;
        let parent_col = op_clone.parent_col;
        let val = op_clone.val;
        let start_line = op_clone.start_line;

        // find the parent id

        let parent_id_peer;
        if start_line {
            parent_id_peer = self.tree.get_line_id_peer(&file_path, parent_row);
        } else {
            parent_id_peer = self.tree.get_id_peer(&file_path, (parent_row, parent_col));
        }

        match parent_id_peer {
            Ok(parent) => {
                if let Some(parent_real) = parent {
                    self.counter += 1;
                    let relation = CrdtRelation::new(val, parent_real.0, parent_real.1);
                    let crdt = CRDT::new(self.counter, self.peer, relation);
                    let network_op =
                        NetworkAdd::new(file_path.clone(), crdt.clone(), parent_row, start_line);

                    let add = self.tree.add_crdt(&file_path, parent_row, crdt, start_line);

                    match add {
                        Ok(add) => {
                            if let Some(pos) = add {
                                self.clear_ide_backlog(pos.0, pos.1);
                                (self.ide_add_callback.unwrap())(network_op);
                                Ok(())
                            } else {
                                // This has probably already been added
                                Ok(())
                            }
                        }

                        Err(e) => Err(Error::Generic(String::from(format!(
                            "Something went wrong: {e}"
                        )))),
                    }
                } else {
                    if let Some(backlogged) = self.ide_backlog.get_mut(&(parent_row, parent_col)) {
                        backlogged.push(IdeOp::ADD(op));
                    } else {
                        let _ = self
                            .ide_backlog
                            .insert((parent_row, parent_col), vec![IdeOp::ADD(op)]);
                    }

                    Ok(())
                }
            }

            Err(e) => Err(Error::Generic(format!("Something went wrong: {e}"))),
        }
    }

    /// Applies a locally-triggered removal and returns the `NetworkRemove` to
    /// send to peers. `is_whole_line` removes `row`'s line-start anchor
    /// instead of `(row, col)`. Rejects the root sentinel as a target.
    pub fn remove_ide_operation(&mut self, op: IdeRemove) -> Result<()> {
        tracing::debug!(?op, "remove_ide_operation");
        let op_clone = op.clone();

        let file_path = op_clone.file_path;
        let row = op_clone.row;
        let col = op_clone.col;
        let is_whole_line = op_clone.is_whole_line;
        // find the id/peer of the crdt
        let id_peer;

        if is_whole_line {
            id_peer = self.tree.get_line_id_peer(&file_path, row)?;
        } else {
            id_peer = self.tree.get_id_peer(&file_path, (row, col))?;
        }

        match id_peer {
            Some(id_peer) => {
                // make sure the sentinel never gets through.
                if id_peer == (0, 0) {
                    return Err(Error::OutOfBounds(String::from("Can't remove sentinel")));
                }

                let remove = self.tree.remove_crdt(&file_path, row, id_peer.0, id_peer.1);

                match remove {
                    Ok(_remove) => {
                        (self.ide_remove_callback.unwrap())(NetworkRemove::new(
                            file_path.clone(),
                            id_peer.0,
                            id_peer.1,
                            row,
                        ));
                        Ok(())
                    }

                    Err(_e) => {
                        println!("remove gets backlogged: item existed but remove failed");
                        if let Some(backlogged) = self.ide_backlog.get_mut(&(row, col)) {
                            backlogged.push(IdeOp::REMOVE(op));
                        } else {
                            let _ = self.ide_backlog.insert((row, col), vec![IdeOp::REMOVE(op)]);
                        }
                        Ok(())
                    }
                }
            }

            // If None, this element doesn't currently exist in the shar.
            // In this case, add it to the backlog
            None => {
                println!("remove gets backlogged: item didn't exist");
                if let Some(backlogged) = self.ide_backlog.get_mut(&(row, col)) {
                    backlogged.push(IdeOp::REMOVE(op));
                } else {
                    let _ = self.ide_backlog.insert((row, col), vec![IdeOp::REMOVE(op)]);
                }
                Ok(())
            }
        }
    }

    /// Applies a remote `NetworkAdd`. If the parent doesn't exist yet, backlogs
    /// `op` instead. On success, replays any backlogged add/remove waiting on it.
    ///
    /// Panics if `network_add_callback` is `None` — only possible if this `SharQueue`
    /// wasn't built via [`Self::new`].
    pub fn add_network_operation(&mut self, op: NetworkAdd) {
        tracing::debug!(?op, "add_network_operation");
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();
        let start_line = op.start_line;

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt, start_line);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    tracing::debug!(
                        row = position.0,
                        col = position.1,
                        "add_network_operation applied"
                    );
                    (self.network_add_callback.unwrap())(position.0, position.1);

                    self.clear_network_backlog(crdt.id, crdt.peer);
                } else {
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
    pub fn remove_network_operation(&mut self, op: NetworkRemove) {
        tracing::debug!(?op, "remove_network_operation");
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
                        is_line = val.2,
                        "remove_network_operation applied"
                    );
                    (self.network_remove_callback.unwrap())(val.0, val.1, val.2);
                } else {
                    // if the remove returns none, the value has already been removed so do nothing
                    tracing::debug!("remove_network_operation target already removed, no-op");
                    return;
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "remove_network_operation backlogged, target not found yet");
                if let Some(backlogged) = self.network_backlog.get_mut(&(id, peer)) {
                    backlogged.push(NetworkOp::REMOVE(op));
                } else {
                    let _ = self
                        .network_backlog
                        .insert((id, peer), vec![NetworkOp::REMOVE(op)]);
                }
            }
        }
    }

    fn clear_network_backlog(&mut self, id: IdSize, peer: PeerIdSize) {
        // get the list of dependancies
        let dependancies = self.network_backlog.remove(&(id, peer));

        if let Some(list) = dependancies {
            for item in list {
                match item {
                    NetworkOp::ADD(op) => {
                        self.add_network_operation(op);
                    }

                    NetworkOp::REMOVE(op) => {
                        self.remove_network_operation(op);
                    }
                }
            }
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }

    fn clear_ide_backlog(&mut self, row: usize, col: usize) {
        // get the list of dependancies
        let dependancies = self.ide_backlog.remove(&(row, col));

        if let Some(list) = dependancies {
            for item in list {
                match item {
                    IdeOp::ADD(op) => {
                        let _ = self.add_ide_operation(op);
                    }

                    IdeOp::REMOVE(op) => {
                        let _ = self.remove_ide_operation(op);
                    }
                }
            }
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }
}
