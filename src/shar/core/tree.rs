//! Contains character tree that manages local state
use std::path::Path;

use crate::shar::error::Error;
use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
pub type Anchor = HashMap<(ID_SIZE, ID_SIZE), (ID_SIZE, char)>;

/// Represents a file in the shar
struct SharFile {
    file_path: String,
    tree: HashMap<ANCHOR_ID_SIZE, Anchor>,
}

impl SharFile {
    pub fn new(file_path: String) -> Result<Self> {
        let file = std::fs::read_to_string(file_path);

        match file {
            Ok(file) => {
                let mut shar_file = SharFile {
                    file_path: file.clone(),
                    tree: HashMap::new(),
                };

                // it's okay to ignore the Error that could occur here because we're performing the
                // same check fo end up in this Ok()
                shar_file.add_file(file)?;

                Ok(shar_file)
            }

            Err(e) => {
                return Err(Error::ReadFail(
                    format!("Something went wrong while trying to read file contents: {e}")
                        .to_string(),
                ));
            }
        }
    }

    /// Adds all the contents of a file to the tree.
    fn add_file(&mut self, file_path: String) -> Result<()> {
        // pull contents of the file
        let file_contents = std::fs::read_to_string(file_path);
        // ids. When a file is being created, the ids will just represent the order in which they
        // were added to the tree (AKA their position in the file)

        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary

        match file_contents {
            Ok(contents) => {
                for (i, c) in contents.char_indices() {
                    let id = (i % (ANCHOR_LENGTH)) as u8;
                    let anchor_id: u16 = id as u16 % ANCHOR_LENGTH as u16;
                    let parent_id = id - 1;
                    let val = c;
                    let peer_id = 0;

                    let crdt = CRDT::new(val, id, parent_id, anchor_id, peer_id);

                    self.add_CRDT(crdt);
                }
            }

            Err(e) => {
                return Err(Error::ReadFail(
                    format!("Something went wrong while trying to read file contents: {e}")
                        .to_string(),
                ));
            }
        };

        Ok(())
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

        match anchor_map {
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

    pub fn add_directory(&mut self, file_path: String) {
        // turn the string into a path
        let file_path = Path::new(&file_path);

        for entry in file_path {}
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

/// Rerpresents a directory in the shar
struct SharDirectory {
    dir_path: String,
    sub_dir: Vec<SharDirectory>,
    sub_files: Vec<SharFile>,
}

impl SharDirectory {
    pub fn new(dir_path: String) {}
}
