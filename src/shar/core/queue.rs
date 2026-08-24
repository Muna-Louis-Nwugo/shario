use crate::shar::prelude::*;
use crate::shar::types::CrdtRelation;
use crate::shar::{core::tree::SharDirectory, types::Operation};
use std::path::PathBuf;
/* The Shar operation queue */

pub struct SharQueue {
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
    ) -> Result<Operation> {
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
                    let op = Operation::new(crdt.clone(), OperationType::AddChar);

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
        is_line: bool,
    ) -> Result<()> {
        // find the id/peer of the crdt
        let id_peer;

        if is_line {
            id_peer = self.tree.get_line_id_peer(file_path, row)?;
        } else {
            id_peer = self.tree.get_id_peer(file_path, (row, col))?;
        }

        match id_peer {
            // assume if None, the crdt has either already been removed or doesn't exist yet to be
            // remved
            // TODO: Once network is up and running, gotta figure out a way to wait to apply
            // changes when the parent / state for those changes don't exist yet
            Some(id_peer) => {
                // make sure the sentinel never gets through.
                if id_peer == (0, 0) {
                    return Ok(());
                }

                let _ = self.tree.remove_crdt(file_path, row, id_peer.0, id_peer.1);
                Ok(())
            }

            None => Ok(()),
        }
    }
}
