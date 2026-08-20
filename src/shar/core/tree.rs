//! Contains character tree that manages local state
use std::fmt;

use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        crdt: &CRDT,
        start_line: bool,
    ) -> Result<()>;

    fn remove_crdt(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
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

// file_path is local placement, not CRDT state, so it's excluded — two replicas of the same
// logical file naturally live at different paths, but should compare equal once they converge
impl PartialEq for SharFile {
    fn eq(&self, other: &Self) -> bool {
        self.characters == other.characters
            && self.projection == other.projection
            && self.char_counter == other.char_counter
    }
}

impl SharFile {
    /// Adds all the contents of a file to the tree.
    fn add_file(&mut self, file_contents: String) {
        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary
        self.projection.push(Vec::new());
        self.characters
            .insert((0, 0), CrdtRelation::new(char::from(0), 0, 0));

        let file_path = self.file_path.clone();
        let mut line = 0;
        let mut start_of_line = false;

        for (_i, c) in file_contents.char_indices() {
            self.char_counter += 1;
            let id = self.char_counter;

            let crdt = CRDT::new(id, 0, CrdtRelation::new(c, id - 1, 0));

            // safe to ignore: file_path always matches self's own path during initial load
            let _ = self.add_crdt(&file_path, line, &crdt, start_of_line);

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

    // finds a crdt by performing a ring search starting from the parent's presumed
    // location and stepping up to the top of the file and down to the bottom of the file to find
    // it
    fn find_crdt(&self, line_num: usize, id: IdSize, peer: PeerIdSize) -> Result<(usize, usize)> {
        // nothing has been added yet, so there's nothing to search for — this must be the root
        // sentinel parent of the very first character
        if (id, peer) == (0, 0) {
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

    // finds where a tombstone would have been recursively
    fn find_tombstone(
        &self,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
    ) -> Result<(usize, usize)> {
        let crdt = self.characters.get(&(id, peer));

        match crdt {
            Some(relation) => {
                if let Some(parent) = self
                    .characters
                    .get(&(relation.parent_id, relation.parent_peer))
                {
                    let coordinates: (usize, usize);
                    let offset: usize;
                    let range: usize;
                    if parent.deleted {
                        coordinates = self.find_tombstone(
                            line_num,
                            relation.parent_id,
                            relation.parent_peer,
                        )?;
                        offset = 0;
                        range = self.projection[coordinates.0].len() + 1;
                    } else {
                        coordinates =
                            self.find_crdt(line_num, relation.parent_id, relation.parent_peer)?;
                        offset = 1;
                        range = self.projection[coordinates.0].len();
                    }

                    // figure out where it goes in the projection
                    for j in coordinates.1..range {
                        // if the next element in the line exists
                        if let Some(next_element) = self.projection[coordinates.0].get(j + offset) {
                            let next_info = (
                                self.characters[next_element].parent_id,
                                self.characters[next_element].parent_peer,
                            );

                            if next_info != (relation.parent_id, relation.parent_peer) {
                                // if the next element doesn't have the same parent, just return
                                // this one
                                return Ok((coordinates.0, j + offset));
                            } else if next_element.0 < id {
                                // if the next element has the same parent but a smaller id, return
                                // this one
                                return Ok((coordinates.0, j + offset));
                            } else if next_element.0 == id {
                                // if the ids are equal, move on to the peer ids

                                // if the peer id  of what's already there is less than this peer
                                // id, that implies that it was made by someone who joined earlier.
                                // In this case, move on to the next value
                                if next_element.1 < peer {
                                    continue;
                                }
                                // if the peer id of what's already there is less than or (god
                                // forbid) equal to this peer id, then assume who made this joined
                                // first and return this one
                                else {
                                    return Ok((coordinates.0, j + offset));
                                }
                            }
                        } else {
                            // just put to the end of the line, assuming next element doesn't exist because
                            // we're at the end
                            return Ok((coordinates.0, j + offset));
                        }
                    }

                    Ok((0, 0))
                } else {
                    return Err(Error::Generic(String::from("tombstone not found")));
                }
            }

            None => return Err(Error::Generic(String::from("tombstone not found"))),
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
        crdt: &CRDT,
        start_line: bool,
    ) -> Result<()> {
        if file_path != &self.file_path {
            return Err(Error::Generic(String::from("Oops! Wrong file")));
        }

        let id = crdt.id;
        let peer = crdt.peer;
        let relation = &crdt.relation;
        let parent_exists: bool;

        // a retry/resend of an op we've already applied is a no-op, not a duplicate insert
        if self.characters.contains_key(&(id, peer)) {
            return Ok(());
        }

        if !self.characters.is_empty() {
            parent_exists = !self.characters[&(relation.parent_id, relation.parent_peer)].deleted;
        } else {
            parent_exists = true;
        }

        // the first character of a line has no real projected predecessor to look up (its
        // parent may be a newline, which is deliberately never stored in the projection) — the
        // line itself is already known, so there's nothing to search for
        let parent = if start_line {
            Ok((line_num, 0))
        } else {
            if parent_exists {
                self.find_crdt(line_num, relation.parent_id, relation.parent_peer)
            } else {
                self.find_tombstone(line_num, relation.parent_id, relation.parent_peer)
            }
        };

        match parent {
            Ok(coordinates) => {
                // add this crdt to the HashMap
                self.characters.insert((id, peer), relation.clone());

                let insertion_value = (id, peer);

                // if this is a new line, split the projection here instead of inserting a character
                if is_line_break(relation.value) {
                    self.add_line_to_projection(coordinates);
                    return Ok(());
                }

                // an empty line has no siblings to compare against, so the new character is simply
                // the only thing on it
                if self.projection[coordinates.0].is_empty() {
                    self.projection[coordinates.0].push(insertion_value);
                    return Ok(());
                }

                let start: usize;
                let offset: usize;
                let range: usize;
                if start_line {
                    start = 0;
                    offset = 0;
                    range = self.projection[coordinates.0].len() + 1;
                } else if !parent_exists {
                    start = coordinates.1;
                    offset = 0;
                    range = self.projection[coordinates.0].len() + 1;
                } else {
                    start = coordinates.1;
                    offset = 1;
                    range = self.projection[coordinates.0].len();
                }

                // figure out where it goes in the projection
                for j in start..range {
                    // if the next element in the line exists
                    if let Some(next_element) = self.projection[coordinates.0].get(j + offset) {
                        let next_info = (
                            self.characters[next_element].parent_id,
                            self.characters[next_element].parent_peer,
                        );

                        if next_info != (relation.parent_id, relation.parent_peer) {
                            // if the next element doesn't have the same parent, just insert this one next
                            self.projection[coordinates.0].insert(j + offset, insertion_value);
                            break;
                        } else if next_element.0 < id {
                            // if the next element has the same parent but a smaller id, put this one
                            // first
                            self.projection[coordinates.0].insert(j + offset, insertion_value);
                            break;
                        } else if next_element.0 == id {
                            // if the ids are equal, move on to the peer ids
                            //

                            // if the peer id  of what's already there is less than this peer
                            // id, that implies that it was made by someone who joined earlier.
                            // In this case, move on to the next value
                            if next_element.1 < peer {
                                continue;
                            }
                            // if the peer id of what's already there is less than or (god
                            // forbid) equal to this peer id, then assume who made this joined
                            // first and insert insert the CRDT
                            else {
                                self.projection[coordinates.0].insert(j + offset, insertion_value);
                                break;
                            }
                        }
                    } else {
                        // just put to the end of the line, assuming next element doesn't exist because
                        // we're at the end
                        self.projection[coordinates.0].push((id, peer));
                    }
                }

                Ok(())
            }

            Err(_e) => Err(Error::OutOfBounds(String::from(
                "parent could not be found",
            ))),
        }
    }

    fn remove_crdt(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
    ) -> Result<()> {
        if file_path != &self.file_path {
            return Err(Error::Generic(String::from("Oops! Wrong file")));
        }
        // remove the crdt from the HashMap
        let crdt = self.characters.get_mut(&(id, peer));

        match crdt {
            Some(val) => {
                if val.deleted {
                    return Ok(());
                }
                val.deleted = true;
            }

            None => return Err(Error::Generic(String::from("crdt cannot be found"))),
        }

        // find the value in the projection and delete it
        let position = self.find_crdt(line_num, id, peer);

        match position {
            Ok(pos) => {
                self.projection[pos.0].remove(pos.1);
                Ok(())
            }

            Err(_e) => Err(Error::Generic(String::from("crdt not found"))),
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

impl SharDirectory {
    // finds the SharFile corresponding to a path
    fn find_file<'a>(&'a mut self, mut path: std::path::Iter<'_>) -> Option<&'a mut SharFile> {
        // check if there even is anything in here?
        if self.sub_dir.is_empty() && self.sub_files.is_empty() {
            return None;
        }

        // do we still have runway in the provided path?
        if let Some(next) = path.next() {
            // is the provided path a file?
            if path.clone().next().is_none() {
                // if yes, find the file in the file vector. Since files are the end of a path, if
                // the file isn't found, just error
                for file in &mut self.sub_files {
                    if file.file_path.ends_with(next) {
                        return Some(file);
                    }
                }

                return None;
            } else {
                // if no, just loop through the directory vector trying to find the right one, then
                // recursively call this function on it
                for dir in &mut self.sub_dir {
                    if dir.dir_name.ends_with(next) {
                        return dir.find_file(path);
                    }
                }
                return None;
            }
        } else {
            return None;
        }
    }
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
        file_path: &PathBuf,
        line_num: usize,
        crdt: &CRDT,
        start_line: bool,
    ) -> Result<()> {
        let mut path = file_path.iter();
        let root = self.dir_name.iter();

        // use up the iterator until it gets past the root of the shar
        for i in root {
            let name = path.next();

            match name {
                Some(n) => {
                    if i == n {
                        continue;
                    } else {
                        return Err(Error::UnknownOrigin(String::from(
                            "Provided path does not match up with root",
                        )));
                    }
                }
                None => {
                    return Err(Error::UnknownOrigin(String::from(
                        "Provided path is upstream from root",
                    )));
                }
            };
        }

        // recursively search for the end of the path
        if let Some(file) = self.find_file(path) {
            file.add_crdt(file_path, line_num, crdt, start_line)?;
            Ok(())
        } else {
            Err(Error::Generic(String::from("File not found")))
        }
    }

    fn remove_crdt(
        &mut self,
        _file_path: &PathBuf,
        _line_num: usize,
        _id: IdSize,
        _peer: PeerIdSize,
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
