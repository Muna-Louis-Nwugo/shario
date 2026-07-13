//! Contains character tree that manages local state
use std::fmt;

use axum::extract::Path;

use crate::shar::error::Error;
use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
// TODO: Lowkey might need to include peer id of child as well as parent
pub type Line = Vec<(IdSize, PeerIdSize, Atom)>;

// Behaviours for structs representing file system or directory names
pub trait Entry<T> {
    fn new(file_path: PathBuf) -> Result<T>;

    fn add_crdt(&mut self, crdt: &CRDT, file_path: &PathBuf, line_num: LineSize, parent: IdSize);
}

/// Represents a file in the shar
#[derive(Debug, Clone)]
pub struct SharFile {
    file_path: PathBuf,
    tree: HashMap<LineSize, Line>,
    char_counter: u32,
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
                // when adding a file, it just uses the peer_id of 0. smallest possible peer_id,
                // meaning that the file's original state is always what gets preference
                current_line.push((self.char_counter, 0, Atom::new(c)));
            }
        }
    }
}

impl Entry<SharFile> for SharFile {
    // TODO: Tree traversal to reconstruct file
    fn new(file_path: PathBuf) -> Result<Self> {
        let file = std::fs::read_to_string(&file_path);

        match file {
            Ok(file) => {
                println!("SharFile::new, okay entered");
                let mut shar_file = SharFile {
                    file_path: file_path,
                    tree: HashMap::new(),
                    char_counter: 0,
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
    fn add_crdt(
        &mut self,
        crdt: &CRDT,
        file_path: &PathBuf,
        mut line_number: LineSize,
        parent_id: IdSize,
    ) {
        // iterate through the line to find the parent_id
        let mut parent_index: Option<usize> = None;

        let line = &self.tree[&line_number];

        for (pos, element) in line.iter().enumerate() {
            if element.0 == parent_id {
                parent_index = Some(pos);

                break;
            }
        }

        if parent_index.is_none() {
            // send of to recursively check above and below line until it's found
            // TODO: Make this actually perform properly recursively
            line_number += 1;
            self.add_crdt(crdt, file_path, line_number, parent_id);
        }

        let (id, peer_id, val) = (crdt.id, crdt.peer_id, crdt.value);

        //check what's already there
        let already_there = self.tree[&line_number][parent_index.unwrap_or(0) + 1];

        // if the conter id value is larger, then it wins
        if already_there.0 > id {
            if let Some(line) = self.tree.get_mut(&line_number) {
                line.insert(parent_index.unwrap_or(0) + 2, (id, peer_id, val));
            };
        } else if id > already_there.0 {
            if let Some(line) = self.tree.get_mut(&line_number) {
                line.insert(parent_index.unwrap_or(0) + 1, (id, peer_id, val));
            };
        } else {
            // if both of the ids are the same, the one with the smaller peer_id wins. This favours
            // those who joined the session earlier
            if already_there.1 < peer_id {
                if let Some(line) = self.tree.get_mut(&line_number) {
                    line.insert(parent_index.unwrap_or(0) + 2, (id, peer_id, val));
                };
            } else if peer_id < already_there.1 {
                if let Some(line) = self.tree.get_mut(&line_number) {
                    line.insert(parent_index.unwrap_or(0) + 1, (id, peer_id, val));
                };
            }
        }
    }
}

impl<'a> fmt::Display for SharFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        // write_out the file path
        write!(f, "{}\n", self.file_path.display())?;

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
    dir_name: PathBuf,
    sub_dir: Vec<SharDirectory>,
    sub_files: Vec<SharFile>,
}

impl Entry<SharDirectory> for SharDirectory {
    /// Doesn't yet support symlinks anywhere in the tree being initialized
    fn new(dir_path: PathBuf) -> Result<Self> {
        let entries = std::fs::read_dir(&dir_path);
        let mut sub_dir_vector = Vec::new();
        let mut sub_file_vector = Vec::new();

        match entries {
            Ok(entries) => {
                // Recursively call new() on children. If the current entry is a file, create the
                // file's CRDT tree

                for entry in entries {
                    let entry = entry?;
                    let entry_type = entry.file_type()?;
                    // if it's a directory, recursively create a new SharDir
                    if entry_type.is_dir() {
                        sub_dir_vector.push(Self::new(entry.path())?);
                    } else if entry_type.is_file() {
                        print!("File was found \n");
                        let file = SharFile::new(entry.path())?;

                        sub_file_vector.push(file);
                    }
                }
                Ok(SharDirectory {
                    dir_name: dir_path,
                    sub_dir: sub_dir_vector,
                    sub_files: sub_file_vector,
                })
            }

            Err(e) => Err(Error::ReadFail(
                format!("Failed to read directory, try again: \n {e} \n").to_string(),
            )),
        }
    }

    fn add_crdt(&mut self, crdt: &CRDT, file_path: &PathBuf, line_num: LineSize, parent: IdSize) {}
}

impl fmt::Display for SharDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // print the name of this directory
        write!(f, "{}\n", self.dir_name.display())?;

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
