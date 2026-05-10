//! Contains character tree that manages local state

use std::cmp;
use std::path::Path;

use crate::shar::error::Error;
use crate::shar::prelude::*;
use std::collections::HashMap;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
pub type Anchor = HashMap<(ID_SIZE, ID_SIZE), (ID_SIZE, char)>;

/// Represents the character tree that manages local state.
struct Tree {
    tree: HashMap<ANCHOR_ID_SIZE, Anchor>,
}

impl Tree {
    pub fn new() -> Self {
        Tree {
            tree: HashMap::new(),
        }
    }

    /// Adds a CRDT to the tree.
    ///
    /// While this is a public function, it is recommended to use standard "add" methods to add new
    /// values accoring to supported IDE specifications.
    pub fn add_CRDT(&mut self, crdt: CRDT) {
        let mut anchor_id = crdt.anchor_id;
        let parent_id = crdt.parent_id;
        let peer_id = crdt.peer_id;
        let val = crdt.value;
        let id = crdt.id;

        // try and get the anchor from the HashMap
        let mut anchor_map = self.tree.get_mut(&anchor_id);

        match (anchor_map) {
            // if the key existed, then just add a CRDT
            Some(anchor) => {
                // if there is still space in that anchor, add it
                if anchor.values().collect::<Vec<_>>().len() <= 250 {
                    anchor.insert((peer_id, parent_id), (id, val));
                } else {
                    anchor_id += 1;
                    self.create_anchor(&crdt);
                }
            }
            // if the key doesn't exist, make it
            None => {
                self.create_anchor(&crdt);
            }
        };
    }

    pub fn add_file(file_path: String) -> Result<()> {
        // pull contents of the file
        let file_contents = std::fs::read_to_string(file_path);
        // ids. When a file is being created, the ids will just represent the order in which they
        // were added to the tree (AKA their position in the file)
        let id: u8 = 0;

        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary
        let peer_id = 0;

        match (file_contents) {
            Ok(contents) => {
                for (i, c) in contents.char_indices() { 
                    id = i as u8;

                    let anchor_id: u16 = id as u16 % ANCHOR_LENGTH as u16;
                    let parent_id = id - 1;
                    let val = c;
                    let id = id;

                    let crdt =  CRDT::new(val, id, parent_id, anchor_id, peer_id);
                }
            },

            Err(e) => return Err(Error::ReadFail("Something went wrong while trying to read file contents: {e}".to_string()));
        };
    }

    fn create_anchor(&mut self, crdt: &CRDT) {
        let anchor_id = crdt.anchor_id;
        let parent_id = crdt.parent_id;
        let peer_id = crdt.peer_id;
        let val = crdt.value;
        let id = crdt.id;

        self.tree.insert(
            anchor_id,
            HashMap::from([((peer_id, parent_id), (id, val))]),
        );
    }
}
