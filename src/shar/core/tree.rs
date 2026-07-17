//! Contains character tree that manages local state
use std::fmt;

use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

// Represents an anchor. Each node is a decomposed CRDT
// Key is (Peer_id, parent_id)
// Value is (id, value)
// TODO: Lowkey might need to include peer id of child as well as parent
pub type Line = Vec<(IdSize, PeerIdSize, Value)>;

/// Encodes a single character to its UTF-8 bytes (one char per node).
fn char_bytes(c: char) -> Value {
    let mut buf = [0u8; 4];
    c.encode_utf8(&mut buf).as_bytes().to_vec()
}

// Behaviours for structs representing file system or directory names
pub trait Entry<T> {
    fn new(file_path: PathBuf) -> Result<T>;

    fn add_crdt(
        &mut self,
        crdt: &CRDT,
        file_path: &PathBuf,
        line_num: LineSize,
        parent: IdSize,
    ) -> Result<()>;
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
        first_line.push((0, 0, char_bytes(0 as char)));
        self.char_counter += 1;

        let mut line_count = 0;

        self.tree.insert(line_count, first_line);

        for (_i, c) in file_contents.char_indices() {
            if c == '\n' {
                line_count += 1;
                let mut new_line = Line::new();
                new_line.push((self.char_counter, 0, char_bytes('\n')));
                self.char_counter += 1;
                self.tree.insert(line_count, new_line);
            } else if c == '\r' {
                line_count += 1;
                let mut new_line = Line::new();
                new_line.push((self.char_counter, 0, char_bytes('\r')));
                self.char_counter += 1;
                self.tree.insert(line_count, new_line);
            } else {
                let current_line = self.tree.get_mut(&line_count).unwrap();
                // when adding a file, it just uses the peer_id of 0. smallest possible peer_id,
                // meaning that the file's original state is always what gets preference
                current_line.push((self.char_counter, 0, char_bytes(c)));
            }
        }
    }
    fn check_line(&self, line_number: LineSize, parent_id: IdSize) -> Result<Option<usize>> {
        let line = self
            .tree
            .get(&line_number)
            .ok_or(Error::OutOfBounds(String::from("Line out of bounds")))?;
        let mut parent_index = None;

        for (pos, element) in line.iter().enumerate() {
            if element.0 == parent_id {
                parent_index = Some(pos);

                break;
            }
        }

        Ok(parent_index)
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
        _file_path: &PathBuf,
        line_number: LineSize,
        parent_id: IdSize,
    ) -> Result<()> {
        // TODO:  Add support for special cases such as new line and remove line

        // iterate through the line to find the parent_id
        let mut parent_index: Option<usize> = None;

        let mut distance_from_og = 0;
        let mut num_errors = 0;

        // looks through the entire fire to find the CRDT, starting from the assumed line
        while parent_index.is_none() {
            if num_errors >= 2 {
                return Err(Error::OutOfBounds(String::from("Parent does not exist")));
            }
            let check_forward = self.check_line(line_number + distance_from_og, parent_id);

            match check_forward {
                Ok(result) => {
                    if !result.is_none() {
                        parent_index = result;
                    }
                }

                Err(_e) => {
                    num_errors += 1;
                }
            }

            if parent_index.is_none() {
                if distance_from_og > line_number {
                    // inrememnt one to continue checking forward, but stop here so we don't
                    // underflow
                    distance_from_og += 1;
                    continue;
                }

                let check_backward = self.check_line(line_number - distance_from_og, parent_id);

                match check_backward {
                    Ok(result) => {
                        if !result.is_none() {
                            parent_index = result;
                        }
                    }

                    Err(_e) => {
                        num_errors += 1;
                    }
                }
            }

            distance_from_og += 1;
        }

        let (id, peer_id, val) = (crdt.id, crdt.peer_id, crdt.value.clone());

        // if the parent is the last in its line, just insert this at the end
        if parent_index >= Some(self.tree[&line_number].len() - 1) {
            if let Some(line) = self.tree.get_mut(&line_number) {
                line.push((id, peer_id, val));
            };

            Ok(())
        } else {
            // check what's already sitting after the parent. Only the id and peer_id
            // matter for ordering, and both are Copy, so we don't hold a borrow of the value
            let successor = &self.tree[&line_number][parent_index.unwrap_or(0) + 1];
            let (other_id, other_peer) = (successor.0, successor.1);

            // if the counter id value is larger, then it wins

            if let Some(line) = self.tree.get_mut(&line_number) {
                if other_id > id {
                    line.insert(parent_index.unwrap_or(0) + 2, (id, peer_id, val));
                } else if id > other_id {
                    line.insert(parent_index.unwrap_or(0) + 1, (id, peer_id, val));
                } else {
                    // if both of the ids are the same, the one with the smaller peer_id wins. This favours
                    // those who joined the session earlier
                    if other_peer < peer_id {
                        line.insert(parent_index.unwrap_or(0) + 2, (id, peer_id, val));
                    } else if peer_id < other_peer {
                        line.insert(parent_index.unwrap_or(0) + 1, (id, peer_id, val));
                    }
                }
            }

            Ok(())
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
                    crdt.0,
                    crdt.1,
                    String::from_utf8_lossy(&crdt.2)
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

    fn add_crdt(
        &mut self,
        _crdt: &CRDT,
        _file_path: &PathBuf,
        _line_num: LineSize,
        _parent: IdSize,
    ) -> Result<()> {
        Ok(())
    }
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
