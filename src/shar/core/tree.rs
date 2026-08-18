//! Contains character tree that manages local state
use std::fmt;

use clap::Id;

use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

// A vector representation of a file's line. Each element is a decomposed CRDT in tuple form:
//
// (value, parent_id, parent_peer_id)

/// Characters that end a projection line rather than occupying a column in one.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

// Behaviours for structs representing file system or directory names
pub trait Entry<T> {
    fn new(file_path: PathBuf) -> Result<T>;

    fn add_crdt(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
        crdt: &CrdtRelation,
        start_line: bool,
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
        self.projection.push(Vec::new());

        let file_path = self.file_path.clone();
        let mut line = 0;
        let mut start_of_line = false;

        for (_i, c) in file_contents.char_indices() {
            self.char_counter += 1;
            let id = self.char_counter;

            let relation = CrdtRelation::new(c, id - 1, 0);

            // safe to ignore: file_path always matches self's own path during initial load
            let _ = self.add_crdt(&file_path, line, id, 0, &relation, start_of_line);

            if is_line_break(c) {
                line += 1;
                start_of_line = true;
            } else {
                start_of_line = false;
            }
        }
    }

    /// Splits a projection line in two right after `coordinates`, so the element at
    /// `coordinates.1` stays the last element of the original line. If `coordinates.1`
    /// is already the last index in the line, this just appends a new empty line after it.
    fn add_line_to_projection(&mut self, coordinates: (usize, usize)) {
        let new_line = self.projection[coordinates.0].split_off(coordinates.1 + 1);
        self.projection.insert(coordinates.0 + 1, new_line);
    }

    // returns the id, peer pair at a specific index
    pub fn get_id_peer(&self, coordinates: (usize, usize)) -> Option<(IdSize, PeerIdSize)> {
        if coordinates.0 <= self.projection.len() - 1 {
            let val = self.projection[coordinates.0].get(coordinates.1);

            match val {
                Some((id, peer)) => Some((id.clone(), peer.clone())),

                None => None,
            }
        } else {
            None
        }
    }

    // finds the parent of a crdt by performing a ring search starting from the parent's presumed
    // location and stepping up to the top of the file and down to the bottom of the file to find
    // it
    fn find_parent(&self, line_num: usize, id: IdSize, peer: PeerIdSize) -> Result<(usize, usize)> {
        // nothing has been added yet, so there's nothing to search for — this must be the root
        // sentinel parent of the very first character
        if self.characters.is_empty() {
            return Ok((0, 0));
        }

        let line = &self.projection[line_num];

        if let Some(index) = line.iter().position(|&item| item == (id, peer)) {
            Ok((line_num, index))
        } else {
            let mut up_offset = 1;
            let mut down_offset = 1;
            let mut up_exhausted = false;
            let mut down_exhausted = false;

            let mut check_up = || {
                if line_num < up_offset {
                    return Err(Error::OutOfBounds(String::from("up exhausted")));
                } else {
                    let current_line = &self.projection[line_num - up_offset];
                    if let Some(index) = current_line.iter().position(|&item| item == (id, peer)) {
                        Ok((line_num - up_offset, index))
                    } else {
                        up_offset += 1;
                        Err(Error::Generic(String::from("not found on line")))
                    }
                }
            };

            let mut check_down = || {
                if line_num + down_offset >= self.projection.len() {
                    return Err(Error::OutOfBounds(String::from("down exhausted")));
                } else {
                    let current_line = &self.projection[line_num + down_offset];
                    if let Some(index) = current_line.iter().position(|&item| item == (id, peer)) {
                        Ok((line_num + down_offset, index))
                    } else {
                        down_offset += 1;
                        Err(Error::Generic(String::from("not found on line")))
                    }
                }
            };

            while up_exhausted != true || down_exhausted != true {
                match check_up() {
                    Ok((row, col)) => return Ok((row, col)),

                    Err(e) => {
                        if e == Error::OutOfBounds(String::from("up exhausted")) {
                            up_exhausted = true;
                        }
                    }
                }

                match check_down() {
                    Ok((row, col)) => return Ok((row, col)),

                    Err(e) => {
                        if e == Error::OutOfBounds(String::from("down exhausted")) {
                            down_exhausted = true;
                        }
                    }
                }
            }

            Err(Error::OutOfBounds(String::from("parent cannot be found")))
        }
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
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
        crdt: &CrdtRelation,
        start_line: bool,
    ) -> Result<()> {
        if file_path != &self.file_path {
            return Err(Error::Generic(String::from("Oops! Wrong file")));
        }

        // a retry/resend of an op we've already applied is a no-op, not a duplicate insert
        if self.characters.contains_key(&(id, peer)) {
            return Ok(());
        }

        // the first character of a line has no real projected predecessor to look up (its
        // parent may be a newline, which is deliberately never stored in the projection) — the
        // line itself is already known, so there's nothing to search for
        let parent = if start_line {
            Ok((line_num, 0))
        } else {
            self.find_parent(line_num, crdt.parent_id, crdt.parent_peer)
        };

        match parent {
            Ok(coordinates) => {
                // add this crdt to the HashMap
                let relation = crdt.clone();
                self.characters.insert((id, peer), relation);

                let insertion_value = (id, peer);

                // if this is a new line, split the projection here instead of inserting a character
                if is_line_break(crdt.value) {
                    self.add_line_to_projection(coordinates);
                    return Ok(());
                }

                // an empty line has no siblings to compare against, so the new character is simply
                // the only thing on it
                if self.projection[coordinates.0].is_empty() {
                    self.projection[coordinates.0].push(insertion_value);
                    return Ok(());
                }

                // characters at the very front of a line have no preceding sibling to anchor on, so
                // start the walk at the first element instead of the element after `coordinates.1`
                let mut insert_at = if start_line { 0 } else { coordinates.1 + 1 };

                // walk forward comparing against each sibling candidate, stopping as soon as we find
                // where this CRDT belongs (running off the end of the line just falls out of the loop,
                // and inserting at that index is equivalent to pushing)
                while let Some(candidate) = self.projection[coordinates.0].get(insert_at) {
                    let candidate_info = (
                        self.characters[candidate].parent_id,
                        self.characters[candidate].parent_peer,
                    );

                    if candidate_info != (crdt.parent_id, crdt.parent_peer) {
                        // the candidate isn't a sibling of this CRDT — insert here
                        break;
                    } else if candidate.0 < id {
                        // same parent but a smaller id — this CRDT comes first
                        break;
                    } else if candidate.0 == id && candidate.1 >= peer {
                        // same id, and the peer id of what's already there is greater than or equal to
                        // this peer id — assume whoever made this joined first, so it goes first
                        break;
                    }

                    insert_at += 1;
                }

                self.projection[coordinates.0].insert(insert_at, insertion_value);

                Ok(())
            }

            Err(_e) => Err(Error::OutOfBounds(String::from(
                "parent could not be found",
            ))),
        }
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
        _line_num: usize,
        _id: IdSize,
        _peer: PeerIdSize,
        _crdt: &CrdtRelation,
        _start_line: bool,
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
