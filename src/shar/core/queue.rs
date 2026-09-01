//! `SharQueue` mediates between the IDE and the CRDT tree, and backlogs
//! out-of-order remote ops until their dependency arrives.

use crate::shar::core::tree::SharDirectory;
use crate::shar::prelude::*;
use crate::types::{CrdtRelation, IdeAdd, IdeRemove};
use crate::types::{NetworkAdd, NetworkRemove};
use std::path::PathBuf;

/// Owns the tree for one shar session and mediates every read/write to it.
#[derive(Debug, Default)]
pub struct SharQueue {
    /// Remote adds waiting on a parent that hasn't arrived yet.
    add_backlog: Vec<NetworkAdd>,
    /// Remote removes waiting on a target that hasn't arrived yet.
    remove_backlog: Vec<NetworkRemove>,
    /// This replica's peer id.
    peer: PeerIdSize,
    tree: SharDirectory,
    /// Shared across every file in the directory.
    counter: u32,
    /// Called with `(row, col)` when an add is applied.
    pub add_callback: Option<fn(usize, usize)>,
    /// Called with `(row, col, was_line_merge)` when a remove is applied.
    pub remove_callback: Option<fn(usize, usize, bool)>,
}

impl SharQueue {
    /// Loads the shar rooted at `dir_path`, storing `add_callback`/
    /// `remove_callback` so they're set. A `SharQueue` built any other way
    /// (e.g. `SharQueue::default()`) leaves them `None`, and calling
    /// [`Self::add_network_operation`]/[`Self::remove_network_operation`] on
    /// one will panic.
    pub fn new(
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        add_callback: fn(usize, usize),
        remove_callback: fn(usize, usize, bool),
    ) -> Result<Self> {
        let mut counter: u32 = 0;
        let tree = SharDirectory::new(dir_path, &mut counter)?;
        /* create a new shar queue*/
        let queue = SharQueue {
            add_backlog: Vec::new(),
            remove_backlog: Vec::new(),
            peer: this_peer_id,
            counter: counter,
            tree: tree,
            add_callback: Some(add_callback),
            remove_callback: Some(remove_callback),
        };

        Ok(queue)
    }

    /// Applies a locally-typed character and returns the `NetworkAdd` to
    /// send to peers. `start_line` resolves the parent via the line's
    /// start-of-line anchor instead of `(parent_row, parent_col)`.
    pub fn add_ide_operation(&mut self, op: IdeAdd) -> Result<NetworkAdd> {
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
                    let op =
                        NetworkAdd::new(file_path.clone(), crdt.clone(), parent_row, start_line);

                    let _ = self.tree.add_crdt(&file_path, parent_row, crdt, start_line);
                    Ok(op)
                } else {
                    Err(Error::Generic(String::from("position not found")))
                }
            }

            Err(e) => Err(Error::Generic(format!("Something went wrong: {e}"))),
        }
    }

    /// Applies a locally-triggered removal and returns the `NetworkRemove` to
    /// send to peers. `is_whole_line` removes `row`'s line-start anchor
    /// instead of `(row, col)`. Rejects the root sentinel as a target.
    pub fn remove_ide_operation(&mut self, op: IdeRemove) -> Result<NetworkRemove> {
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

                let _ = self.tree.remove_crdt(&file_path, row, id_peer.0, id_peer.1);
                Ok(NetworkRemove::new(
                    file_path.clone(),
                    id_peer.0,
                    id_peer.1,
                    row,
                ))
            }

            // If None, this element doesn't currently exist in the shar. Since this is getting
            // added from the IDE, there are 2 explanations:
            // 1. The IDE got the position wrong
            // 2. The element has already been removed in the shar, but the IDE hasn't reflected
            //    this change
            //
            // Either way, we want to return an Error so that the IDE can confirm the positions and
            // try again.
            None => Err(Error::OutOfBounds(String::from(
                "this element does not exist in the shar",
            ))),
        }
    }

    /// Applies a remote `NetworkAdd`. If the parent doesn't exist yet, backlogs
    /// `op` instead. On success, replays any backlogged add/remove waiting on it.
    ///
    /// Panics if `add_callback` is `None` — only possible if this `SharQueue`
    /// wasn't built via [`Self::new`].
    pub fn add_network_operation(&mut self, op: NetworkAdd) {
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();
        let start_line = op.start_line;

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt, start_line);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    (self.add_callback.unwrap())(position.0, position.1);

                    // traverse the add_backlog to see if we have any inserts depending on this
                    let mut i = 0;

                    while i < self.add_backlog.len() {
                        let item = self.add_backlog[i].clone();

                        let item_crdt = item.crdt;

                        if (crdt.id, crdt.peer)
                            == (item_crdt.relation.parent_id, item_crdt.relation.parent_peer)
                        {
                            self.add_backlog.remove(i);
                            self.add_network_operation(item);
                        } else {
                            i += 1;
                        }
                    }

                    // traverse the remove_backlog to see if we have any removes depending on this:

                    let mut j = 0;
                    let mut removed = false;
                    while j < self.remove_backlog.len() {
                        let item = self.remove_backlog[j].clone();

                        if (item.id, item.peer) == (crdt.id, crdt.peer) {
                            if !removed {
                                self.remove_backlog.remove(j);
                                self.remove_network_operation(item);
                                removed = true;
                            }
                        } else {
                            j += 1;
                        }
                    }
                } else {
                    return;
                }
            }

            Err(_e) => {
                self.add_backlog.push(op);
            }
        };
    }

    /// Applies a remote `NetworkRemove`. If the target doesn't exist yet,
    /// backlogs `op` instead; drained once the matching add arrives (see
    /// [`Self::add_network_operation`]). A retry of an already-removed target
    /// is a no-op.
    ///
    /// Panics if `remove_callback` is `None` — only possible if this
    /// `SharQueue` wasn't built via [`Self::new`].
    pub fn remove_network_operation(&mut self, op: NetworkRemove) {
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let id = op_clone.id;
        let peer = op_clone.peer;
        let row = op_clone.row;

        let removed = self.tree.remove_crdt(&file_path, row, id, peer);

        match removed {
            Ok(pos) => {
                if let Some(val) = pos {
                    (self.remove_callback.unwrap())(val.0, val.1, val.2);
                } else {
                    // if the remove returns none, the value has already been removed so do nothing
                    return;
                }
            }
            Err(_e) => self.remove_backlog.push(op),
        }
    }
}
