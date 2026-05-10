//! Contains character tree that manages local state

use std::cmp;

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
