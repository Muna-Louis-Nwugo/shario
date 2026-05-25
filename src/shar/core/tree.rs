//! Contains character tree that manages local state
use std::fmt;

use crate::shar::error::Error;
use crate::shar::prelude::*;
use std::collections::HashMap;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
// TODO: Lowkey might need to include peer id of child as well as parent
pub type Anchor = HashMap<(IdSize, IdSize), (IdSize, char)>;

/// Represents a file in the shar
pub struct SharFile {
    file_path: String,
    tree: HashMap<AnchorIdSize, Anchor>,
}

impl SharFile {
    // TODO: Tree traversal to reconstruct file
    pub fn new(file_path: &str) -> Result<Self> {
        let file = std::fs::read_to_string(file_path);

        match file {
            Ok(file) => {
                println!("SharFile::new, okay entered");
                let mut shar_file = SharFile {
                    file_path: String::from(file_path),
                    tree: HashMap::new(),
                };

                // it's okay to ignore the Error that could occur here because we're performing the
                // same check fo end up in this Ok()
                shar_file.add_file(String::from(file));

                Ok(shar_file)
            }

            Err(e) => Err(Error::ReadFail(
                format!("Something went wrong while trying to read file contents: \n {e} \n")
                    .to_string(),
            )),
        }
    }

    /// Adds all the contents of a file to the tree.
    fn add_file(&mut self, file_contents: String) {
        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary

        for (i, c) in file_contents.char_indices() {
            // every id starts from one
            let id = (i % (ANCHOR_LENGTH)) as u8 + 1;
            let anchor_id: u16 = id as u16 / ANCHOR_LENGTH as u16;
            let parent_id = id - 1;
            let val = c;
            let peer_id = 0;

            let crdt = CRDT::new(val, id, parent_id, anchor_id, peer_id);

            self.add_crdt(crdt);
        }
    }

    /// Adds a CRDT to the tree.
    ///
    /// While this is a public function, it is recommended to use standard "add" methods to add new
    /// values accoring to supported IDE specifications.
    pub fn add_crdt(&mut self, mut crdt: CRDT) {
        let anchor_id = crdt.anchor_id;
        let parent_id = crdt.parent_id;
        let peer_id = crdt.peer_id;
        let val = crdt.value;
        let id = crdt.id;

        // try and get the anchor from the HashMap
        let anchor_map = self.tree.get_mut(&anchor_id);

        match anchor_map {
            // if the key existed, then just add a CRDT
            Some(anchor) => {
                // if there is still space in that anchor, add it
                if anchor.values().collect::<Vec<_>>().len() <= 250 {
                    anchor.insert((peer_id, parent_id), (id, val));
                } else {
                    //TODO: properly pass this into the next iteration
                    crdt.anchor_id += 1;
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

impl fmt::Display for SharFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        // write_out the file path
        write!(f, "{}\n", self.file_path)?;

        let crdt_tree = self.tree.clone();
        let mut anchor_id = 0;

        // print out the CRDTs
        for anchor in crdt_tree.into_values() {
            write!(f, "ANCHOR: {}\n\n", anchor_id)?;
            for crdt in anchor {
                write!(
                    f,
                    "Peer_id: {}; parent_id: {}; id: {}; value: {} \n",
                    crdt.0.0, crdt.0.1, crdt.1.0, crdt.1.1
                )?;
            }
            anchor_id += 1;
        }

        Ok(())
    }
}

/// Rerpresents a directory in the shar
pub struct SharDirectory {
    dir_name: String,
    sub_dir: Vec<SharDirectory>,
    sub_files: Vec<SharFile>,
}

impl SharDirectory {
    /// Doesn't yet support symlinks anywhere in the tree being initialized
    pub fn new(dir_path: String) -> Result<Self> {
        let entries = std::fs::read_dir(&dir_path);
        let mut sub_dir_vector = Vec::new();
        let mut sub_file_vector = Vec::new();

        match entries {
            Ok(entries) => {
                // Recursively call new() on children. If the current entry is a file, create the
                // file's CRDT tree

                // NOTE: this uses to_str.unwrap which will crash on non UTF-8 characters
                // TODO: allow this to be used on paths containing non-UTF8 characters. This is not
                // a very high priority task, but it should be done to provide maximum flexibility
                for entry in entries {
                    let entry = entry?;
                    let entry_type = entry.file_type()?;
                    // if it's a directory, recursively create a new SharDir
                    if entry_type.is_dir() {
                        sub_dir_vector
                            .push(Self::new(String::from(entry.path().to_str().unwrap()))?);
                    } else if entry_type.is_file() {
                        print!("File was found \n");
                        let file = SharFile::new(entry.path().to_str().unwrap())?;

                        sub_file_vector.push(file);
                    }
                }
                Ok(SharDirectory {
                    dir_name: String::from(&dir_path),
                    sub_dir: sub_dir_vector,
                    sub_files: sub_file_vector,
                })
            }

            Err(e) => Err(Error::ReadFail(
                format!("Failed to read directory, try again: \n {e} \n").to_string(),
            )),
        }
    }
}

impl fmt::Display for SharDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // print the name of this directory
        write!(f, "{}\n", self.dir_name)?;

        // print the subfiles
        let files = &self.sub_files;
        for file in files {
            file.fmt(f)?;
        }

        // print the sub_directories
        let dirs = &self.sub_dir;
        for dir in dirs {
            dir.fmt(f)?;
        }

        Ok(())
    }
}
