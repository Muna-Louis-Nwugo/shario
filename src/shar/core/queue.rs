use crate::shar::prelude::*;
use crate::shar::types::CrdtRelation;
use crate::shar::{core::tree::SharDirectory, types::AddOperation, types::RemoveOperation};
use std::path::PathBuf;
/* The Shar operation queue */

pub struct SharQueue {
    add_backlog: Vec<AddOperation>,
    remove_backlog: Vec<RemoveOperation>,
    peer: PeerIdSize,
    tree: SharDirectory,
    counter: u32,
    callback: fn(usize, usize, char),
}

impl SharQueue {
    pub fn new(
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        callback: fn(usize, usize, char),
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
            callback: callback,
        };

        Ok(queue)
    }

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
                    is_whole_line,
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

    pub fn add_network_operation(&mut self, op: AddOperation, callback: fn(usize, usize)) {
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();
        let start_line = op.start_line;

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt, start_line);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    callback(position.0, position.1);

                    // traverse the add_backlog to see if we have any inserts depending on this
                    let mut i = 0;

                    while i < self.add_backlog.len() {
                        let item = self.add_backlog[i].clone();

                        let item_crdt = item.crdt;

                        if (crdt.id, crdt.peer)
                            == (item_crdt.relation.parent_id, item_crdt.relation.parent_peer)
                        {
                            self.add_backlog.remove(i);
                            self.add_network_operation(item, callback);
                        } else {
                            i += 1;
                        }
                    }

                    // traverse the remove_backlog to see if we have any removes depending on this:
                    for _i in 0..self.remove_backlog.len() {
                        // DO REMOVE
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
}
