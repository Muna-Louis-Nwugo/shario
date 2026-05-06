//! Contains character tree that manages local state

use crate::shar::prelude::*;
use std::collections::HashMap;

// Represents an anchor
// Keys are (Peer_id, crdt_id)
pub type Anchor = HashMap<(ID_SIZE, ID_SIZE), Vec<CRDT>>;

/// Represents the character tree that manages local state.
struct Tree {
    tree: HashMap<ANCHOR_ID_SIZE, Anchor>,
}

impl Tree {
    pub fn new() -> Self {
        Tree {
            tree: HashMap::new(Anchor::new()),
        }
    }

    pub fn add(&mut self, crdt: CRDT) {
        let anchor = crdt.anchor_id;
        let parent = crdt.parent_id;

        // try finding the parent in the tree
        if (!self.tree.values().contains_key((anchor, parent))) {
            panic!("This parent doesn't exist in the shar yet")
        }
    }
}
