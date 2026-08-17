//! Contains character tree that manages local state
use std::fmt;

use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

// A vector representation of a file's line. Each element is a decomposed CRDT in tuple form:
//
// (value, parent_id, parent_peer_id)

// Behaviours for structs representing file system or directory names
pub trait Entry<T> {
    fn new(file_path: PathBuf) -> Result<T>;

    fn add_crdt(
        &mut self,
        file_path: &PathBuf,
        coordinates: (usize, usize),
        id: IdSize,
        peer: PeerIdSize,
        crdt: &CrdtRelation,
    ) -> Result<()>;
}

/// Represents a file in the shar
#[derive(Debug, Clone)]
pub struct SharFile {
    file_path: PathBuf,
    characters: HashMap<(IdSize, PeerIdSize), CrdtRelation>,
    projection: Vec<Vec<(IdSize, u8)>>,
    char_counter: u32,
}

impl SharFile {
    /// Adds all the contents of a file to the tree.
    fn add_file(&mut self, file_contents: String) {
        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary

        for (_i, c) in file_contents.char_indices() {
            self.char_counter += 1;

            let relation = CrdtRelation::new(c, self.char_counter - 1, 0);

            self.characters.insert((self.char_counter, 0), relation);
        }
    }

    fn update_projection(
        &mut self,
        relation: CrdtRelation,
        value: Value,
        id: IdSize,
        peer_id: PeerIdSize,
    ) {
    }

    /// Splits a projection line in two right after `coordinates`, so the element at
    /// `coordinates.1` stays the last element of the original line. If `coordinates.1`
    /// is already the last index in the line, this just appends a new empty line after it.
    fn add_line_to_projection(&mut self, coordinates: (usize, usize)) {
        let new_line = self.projection[coordinates.0].split_off(coordinates.1 + 1);
        self.projection.insert(coordinates.0 + 1, new_line);
    }
}

impl Entry<SharFile> for SharFile {
    // TODO: Tree traversal to reconstruct file
    fn new(file_path: PathBuf) -> Result<Self> {
        let file = std::fs::read_to_string(&file_path);

        match file {
            Ok(file) => {
                let mut shar_file = SharFile {
                    file_path: file_path,
                    characters: HashMap::new(),
                    projection: Vec::new(),
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
        file_path: &PathBuf,
        coordiantes: (usize, usize),
        id: IdSize,
        peer: PeerIdSize,
        crdt: &CrdtRelation,
    ) -> Result<()> {
        if file_path != &self.file_path {
            return Err(Error::Generic(String::from("Oops! Wrong file")));
        }

        // add this crdt to the HashMap
        let relation = crdt.clone();
        self.characters.insert((id, peer), relation);

        let insertion_value = (id, peer);

        // if this is a new line, split the projection here instead of inserting a character
        if matches!(
            crdt.value,
            '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            self.add_line_to_projection(coordiantes);
            return Ok(());
        }

        // figure out where it goes in the projection
        for j in coordiantes.1..self.projection[coordiantes.0].len() {
            // if the next element in the line exists
            if let Some(next_element) = self.projection[coordiantes.0].get(j + 1) {
                let next_info = (
                    self.characters[next_element].parent_id,
                    self.characters[next_element].parent_peer,
                );

                if next_info != (crdt.parent_id, crdt.parent_peer) {
                    // if the next element doesn't have the same parent, just insert this one next
                    self.projection[coordiantes.0].insert(j + 1, insertion_value);
                    break;
                } else if next_element.0 < id {
                    // if the next element has the same parent but a smaller id, put this one
                    // first
                    self.projection[coordiantes.0].insert(j + 1, insertion_value);
                    break;
                } else if next_element.0 == id {
                    // if the ids are equal, move on to the peer ids
                    //

                    // if the peer id  of what's already there is less than this peer
                    // id, that implies that it was made by someone who joined earlier.
                    // In this case, increment the offset and continue the coop to
                    // check the next value
                    if next_element.1 < peer {
                        continue;
                    }
                    // if the peer id of what's already there is less than or (god
                    // forbid) equal to this peer id, then assume who made this joined
                    // first and insert insert the CRDT
                    else {
                        self.projection[coordiantes.0].insert(j + 1, insertion_value);
                        break;
                    }
                }
            } else {
                // just put to the end of the line, assuming next element doesn't exist because
                // we're at the end
                self.projection[coordiantes.0].push((id, peer));
            }
        }

        Ok(())
    }
}

impl<'a> fmt::Display for SharFile {
    fn fmt(&self, _f: &mut fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
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
        _file_path: &PathBuf,
        _coordiantes: (usize, usize),
        _id: IdSize,
        _peer: PeerIdSize,
        _crdt: &CrdtRelation,
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
