//! `SharQueue` is the mediator between the IDE, the CRDT tree, and (eventually)
//! the network — the hub in shario's hub-and-spoke architecture. It owns the
//! one [`SharDirectory`] for a session, hands out ids from a single shared
//! counter, and is the only thing that ever calls into the tree directly.
//!
//! Local edits (`add_ide_crdt`/`remove_ide_crdt`) are resolved and applied to
//! the tree synchronously and return an `Operation` for the caller to send out
//! over the network — no local queueing needed, since human typing speed is far
//! below what the tree can absorb.
//!
//! Remote edits (`add_network_operation`/`remove_network_operation`) can arrive
//! whose parent (for an add) or target (for a remove) hasn't reached this
//! replica yet, since delivery isn't guaranteed to be causally ordered. When
//! that happens the op is stashed in `add_backlog`/`remove_backlog` instead of
//! applied; every time a *later* op successfully lands, both backlogs are
//! re-checked for anything that was waiting on exactly that id, which can cause
//! a chain of previously-stuck ops to resolve all at once.

use crate::shar::prelude::*;
use crate::shar::types::CrdtRelation;
use crate::shar::{core::tree::SharDirectory, types::AddOperation, types::RemoveOperation};
use std::path::PathBuf;

/// Owns the tree for one shar session and mediates every read/write to it.
pub struct SharQueue {
    /// Remote adds whose parent hasn't arrived yet, waiting to be replayed.
    add_backlog: Vec<AddOperation>,
    /// Remote removes whose target hasn't arrived yet, waiting to be replayed.
    remove_backlog: Vec<RemoveOperation>,
    /// This replica's own peer id, stamped onto every locally-created `CRDT`.
    peer: PeerIdSize,
    /// The actual CRDT state for the whole shar directory.
    tree: SharDirectory,
    /// A single counter shared across every file in the directory, so every
    /// locally-created id is unique within this replica (still needs pairing
    /// with `peer` to be globally unique across replicas).
    counter: u32,
    /// Invoked with `(row, col)` whenever an add is actually applied to the
    /// projection (not when it's merely backlogged).
    add_callback: fn(usize, usize),
    /// Invoked with `(row, col, was_line_merge)` whenever a remove is actually
    /// applied — the third field is `true` when the removal merged two lines
    /// together (see `SharFile::remove_crdt`), `false` for an ordinary removal.
    remove_callback: fn(usize, usize, bool),
}

impl SharQueue {
    /// Loads the shar rooted at `dir_path` and wraps it in a fresh queue for
    /// `this_peer_id`. `add_callback`/`remove_callback` are how the IDE finds
    /// out where an operation actually landed once it's applied.
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
            add_callback: add_callback,
            remove_callback: remove_callback,
        };

        Ok(queue)
    }

    /// Applies a locally-typed character to the tree and returns the resulting
    /// `AddOperation` for the caller to send to peers.
    ///
    /// `(parent_row, parent_col)` is the position the IDE says the new
    /// character goes after. If `start_line` is true, that position is ignored
    /// in favor of the line's start-of-line anchor instead (see
    /// `SharFile::get_line_id_peer`) — this is the front-of-line case, where
    /// the real parent is a newline or the root sentinel, neither of which live
    /// in the projection at a queryable `(row, col)`.
    pub fn add_ide_crdt(
        &mut self,
        file_path: &PathBuf,
        parent_row: usize,
        parent_col: usize,
        val: char,
        start_line: bool,
    ) -> Result<AddOperation> {
        // find the parent id

        let parent_id_peer;
        if start_line {
            parent_id_peer = self.tree.get_line_id_peer(file_path, parent_row);
        } else {
            parent_id_peer = self.tree.get_id_peer(file_path, (parent_row, parent_col));
        }

        match parent_id_peer {
            Ok(parent) => {
                if let Some(parent_real) = parent {
                    self.counter += 1;
                    let relation = CrdtRelation::new(val, parent_real.0, parent_real.1);
                    let crdt = CRDT::new(self.counter, self.peer, relation);
                    let op =
                        AddOperation::new(file_path.clone(), crdt.clone(), parent_row, start_line);

                    let _ = self.tree.add_crdt(file_path, parent_row, crdt, start_line);
                    Ok(op)
                } else {
                    Err(Error::Generic(String::from("position not found")))
                }
            }

            Err(e) => Err(Error::Generic(format!("Something went wrong: {e}"))),
        }
    }

    /// Applies a locally-triggered removal to the tree and returns the
    /// resulting `RemoveOperation` for the caller to send to peers.
    ///
    /// If `is_whole_line` is true, `row`'s line-start anchor is removed instead
    /// of a character at `(row, col)` — this is how deleting the newline before
    /// a line (merging it into the previous one) gets triggered. The root
    /// sentinel can never legally be a removal target (there's no line "-1" to
    /// merge into), so a request that resolves to it is rejected with an
    /// `Err` rather than passed through to `SharFile::remove_crdt`.
    pub fn remove_ide_crdt(
        &mut self,
        file_path: &PathBuf,
        row: usize,
        col: usize,
        is_whole_line: bool,
    ) -> Result<RemoveOperation> {
        // find the id/peer of the crdt
        let id_peer;

        if is_whole_line {
            id_peer = self.tree.get_line_id_peer(file_path, row)?;
        } else {
            id_peer = self.tree.get_id_peer(file_path, (row, col))?;
        }

        match id_peer {
            Some(id_peer) => {
                // make sure the sentinel never gets through.
                if id_peer == (0, 0) {
                    return Err(Error::OutOfBounds(String::from("Can't remove sentinel")));
                }

                let _ = self.tree.remove_crdt(file_path, row, id_peer.0, id_peer.1);
                Ok(RemoveOperation::new(
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

    /// Applies a remote `AddOperation`, recursively.
    ///
    /// If `op`'s parent already exists, it's applied immediately, its position
    /// is reported through `add_callback`, and then both backlogs are swept for
    /// anything waiting specifically on this newly-applied node: any matching
    /// `add_backlog` entry is removed and re-applied (via a recursive call to
    /// this same function, so a chain of dependents can cascade in one pass),
    /// and any matching `remove_backlog` entry is removed and handed to
    /// [`Self::remove_network_operation`].
    ///
    /// If the parent doesn't exist yet, `op` is stashed in `add_backlog`
    /// instead of applied — this is the out-of-order-delivery case, and it's
    /// silent by design: nothing is reported until the dependency actually
    /// arrives and this function runs again for it.
    pub fn add_network_operation(&mut self, op: AddOperation) {
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();
        let start_line = op.start_line;

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt, start_line);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    (self.add_callback)(position.0, position.1);

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

    /// Applies a remote `RemoveOperation`.
    ///
    /// If the target already exists, it's tombstoned and its position (plus
    /// whether the removal merged two lines) is reported through
    /// `remove_callback`. If the target has already been removed, nothing
    /// happens — a duplicate/retried remove is a no-op, not an error.
    ///
    /// If the target doesn't exist yet, `op` is stashed in `remove_backlog`
    /// instead — the out-of-order-delivery case for removes, mirroring
    /// [`Self::add_network_operation`]'s handling for adds. It's drained from
    /// there once the matching add finally arrives (see that function).
    pub fn remove_network_operation(&mut self, op: RemoveOperation) {
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let id = op_clone.id;
        let peer = op_clone.peer;
        let row = op_clone.row;

        let removed = self.tree.remove_crdt(&file_path, row, id, peer);

        match removed {
            Ok(pos) => {
                if let Some(val) = pos {
                    (self.remove_callback)(val.0, val.1, val.2);
                } else {
                    // if the remove returns none, the value has already been removed so do nothing
                    return;
                }
            }
            Err(_e) => self.remove_backlog.push(op),
        }
    }
}
