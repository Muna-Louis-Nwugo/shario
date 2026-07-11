//! Contains character tree that manages local state
use std::fmt;

use crate::shar::error::Error;
use crate::shar::prelude::*;
use std::collections::HashMap;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
// TODO: Lowkey might need to include peer id of child as well as parent
pub type Line = Vec<(IdSize, PeerIdSize, Atom)>;

// Behaviours for structs representing file system or directory names
pub trait Entry<T> {
    fn new(file_path: String, all_ids: Vec<u8>, this_peer_id: PeerIdSize) -> Result<T>;

    fn add_crdt(&mut self, crdt: CRDT);
}

/// Represents a file in the shar
#[derive(Debug, Clone)]
pub struct SharFile {
    file_path: String,
    tree: HashMap<LineSize, Line>,
    char_counter: u32,
    all_peer_ids: Vec<PeerIdSize>,
    this_peer_id: PeerIdSize,
}

impl SharFile {
    /// Adds all the contents of a file to the tree.
    fn add_file(&mut self, file_contents: String) {
        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary
        let mut first_line = Line::new();
        first_line.push((0, 0, Atom::new(0 as char)));
        self.char_counter += 1;

        let mut line_count = 0;

        self.tree.insert(line_count, first_line);

        for (_i, c) in file_contents.char_indices() {
            if c == '\n' {
                line_count += 1;
                let mut new_line = Line::new();
                new_line.push((self.char_counter, 0, Atom::new('\n')));
                self.char_counter += 1;
                self.tree.insert(line_count, new_line);
            } else if c == '\r' {
                line_count += 1;
                let mut new_line = Line::new();
                new_line.push((self.char_counter, 0, Atom::new('\r')));
                self.char_counter += 1;
                self.tree.insert(line_count, new_line);
            } else {
                let current_line = self.tree.get_mut(&line_count).unwrap();
                current_line.push((self.char_counter, self.this_peer_id, Atom::new(c)));
            }
        }
    }
}

impl Entry<SharFile> for SharFile {
    // TODO: Tree traversal to reconstruct file
    fn new(
        file_path: String,
        all_peer_ids: Vec<PeerIdSize>,
        this_peer_id: PeerIdSize,
    ) -> Result<Self> {
        let file = std::fs::read_to_string(&file_path);

        match file {
            Ok(file) => {
                println!("SharFile::new, okay entered");
                let mut shar_file = SharFile {
                    file_path: file_path,
                    tree: HashMap::new(),
                    char_counter: 0,
                    all_peer_ids: all_peer_ids,
                    this_peer_id: this_peer_id,
                };

                // it's okay to ignore the Error that could occur here because we're performing the
                // same check fo end up in this Ok()
                shar_file.add_file(file);

                Ok(shar_file)
            }

            Err(e) => Err(Error::ReadFail(
                format!("Something went wrong while trying to read file contents: \n {e} \n")
                    .to_string(),
            )),
        }
    }
    /// Adds a CRDT to the tree.
    fn add_crdt(&mut self, mut crdt: CRDT) {}
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
                    "id: {}; peer_id: {}; value: {:?}; \n",
                    crdt.0, crdt.1, crdt.2
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

impl Entry<SharDirectory> for SharDirectory {
    /// Doesn't yet support symlinks anywhere in the tree being initialized
    fn new(
        dir_path: String,
        all_peer_ids: Vec<PeerIdSize>,
        this_peer_id: PeerIdSize,
    ) -> Result<Self> {
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
                        sub_dir_vector.push(Self::new(
                            String::from(entry.path().to_str().unwrap()),
                            all_peer_ids.clone(),
                            this_peer_id,
                        )?);
                    } else if entry_type.is_file() {
                        print!("File was found \n");
                        let file = SharFile::new(
                            String::from(entry.path().to_str().unwrap()),
                            all_peer_ids.clone(),
                            this_peer_id,
                        )?;

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

    fn add_crdt(&mut self, crdt: CRDT) {}
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
