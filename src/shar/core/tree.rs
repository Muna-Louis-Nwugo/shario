//! The actual CRDT tree: [`SharFile`] holds one file's character state, and
//! [`SharDirectory`] recursively mirrors a directory of them, routing every
//! operation to the right file by path.
//!
//! Each `SharFile` keeps two views of the same data:
//! - `characters`: the real CRDT state, an id-keyed map of every character ever
//!   inserted (including tombstoned ones — there's no garbage collection yet),
//!   each pointing at its parent's `(id, peer)`.
//! - `projection`: a derived `line -> column -> (id, peer)` view, rebuilt
//!   incrementally as `characters` changes, purely for the IDE's benefit (it
//!   thinks in positions, not ids). Tombstoned characters are dropped from here
//!   immediately, even though they stay in `characters` forever.
//!
//! Resolving a node's *current* position from just its `(id, peer)` (e.g. to
//! place a new sibling, or to find where to delete from) is what
//! [`SharFile::find_crdt`]'s ring search and [`SharFile::find_tombstone`]'s
//! recursive climb exist for.

use std::fmt;

use crate::shar::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// Characters that end a projection line rather than occupying a column in one.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// One file's CRDT state, in both its id-keyed form and its derived
/// line/column projection. See the module docs for how the two relate.
#[derive(Debug, Clone)]
pub struct SharFile {
    /// This file's location. Not part of the CRDT state itself — two replicas
    /// of the same logical file can (and often will) live at different local
    /// paths, but should still compare equal once converged (see the
    /// `PartialEq` impl below, which deliberately excludes this field).
    file_path: PathBuf,
    /// Every character ever inserted, live or tombstoned, keyed by its
    /// globally-unique `(id, peer)` identity. `(0, 0)` is the root sentinel —
    /// not a real character, just "nothing before this" for the very first
    /// character of the file.
    characters: HashMap<(IdSize, PeerIdSize), CrdtRelation>,
    /// The derived `line -> column -> (id, peer)` view the IDE actually reads
    /// from. Tombstoned characters are removed from here as soon as they're
    /// deleted, even though they remain in `characters` forever.
    projection: Vec<Vec<(IdSize, PeerIdSize)>>,
    /// Per line, the `(id, peer)` of whatever anchors that line's start — the
    /// newline ending the previous line, or the root sentinel for line 0.
    /// Needed because a line's true first-character parent (a newline, or the
    /// sentinel) is never itself stored in `projection`, so there'd otherwise
    /// be no way to resolve a front-of-line insert's parent at all. Kept in
    /// lockstep with `projection`'s indices — shifted on line split/merge the
    /// same way `projection` itself is.
    line_start_ids: Vec<(IdSize, PeerIdSize)>,
}

// file_path is local placement, not CRDT state, so it's excluded — two replicas of the same
// logical file naturally live at different paths, but should compare equal once they converge
impl PartialEq for SharFile {
    fn eq(&self, other: &Self) -> bool {
        self.characters == other.characters && self.projection == other.projection
    }
}

impl SharFile {
    /// Loads `file_path` from disk and builds its initial CRDT tree, assigning
    /// each character a sequentially-increasing id from `counter`. `counter` is
    /// shared across an entire `SharDirectory`, not reset per file — see
    /// [`Self::add_file`] for why that's safe.
    // TODO: Tree traversal to reconstruct file
    pub fn new(file_path: PathBuf, counter: &mut u32) -> Result<Self> {
        let file = std::fs::read_to_string(&file_path);

        match file {
            Ok(file) => {
                let mut shar_file = SharFile {
                    file_path: file_path,
                    characters: HashMap::new(),
                    projection: Vec::new(),
                    line_start_ids: Vec::new(),
                };

                // it's okay to ignore the Error that could occur here because we're performing the
                // same check fo end up in this Ok()
                shar_file.add_file(file, counter);

                Ok(shar_file)
            }

            Err(e) => Err(Error::ReadFail(
                format!("Something went wrong while trying to read file contents: \n {e} \n")
                    .to_string(),
            )),
        }
    }

    /// Seeds the root sentinel, then inserts every character of `file_contents`
    /// in order, each parented on the character immediately before it *within
    /// this file* (`prev_id`, reset to the sentinel at the start of every
    /// file). Deliberately not `id - 1`: `counter` is shared across every file
    /// in a directory, so `id - 1` would sometimes be the last character of a
    /// *different* file loaded just before this one, which doesn't exist in
    /// this file's own `characters` map and would fail to resolve.
    fn add_file(&mut self, file_contents: String, counter: &mut u32) {
        // the shar specification states that peer 0 is reserved for the char itself to add to the
        // tree as necessary
        self.projection.push(Vec::new());
        self.characters
            .insert((0, 0), CrdtRelation::new(char::from(0), 0, 0));
        self.line_start_ids.push((0, 0));

        let file_path = self.file_path.clone();
        let mut line = 0;
        let mut start_of_line;
        let mut prev: char = char::from(0);
        let mut prev_id = 0;

        for (_i, c) in file_contents.char_indices() {
            *counter += 1;
            let id = counter.clone();

            let crdt = CRDT::new(id, 0, CrdtRelation::new(c, prev_id, 0));
            prev_id = id;

            start_of_line = is_line_break(prev);

            // safe to ignore: file_path always matches self's own path during initial load
            let _ = self.add_crdt(line, crdt, start_of_line);

            if is_line_break(c) {
                line += 1;
            }

            prev = c;
        }
    }

    /// Splits a projection line in two right after `coordinates`, so the element at
    /// `coordinates.1` stays the last element of the original line. If `coordinates.1`
    /// is already the last index in the line, this just appends a new empty line after it.
    fn add_line_to_projection(&mut self, coordinates: (usize, usize)) {
        if self.projection[coordinates.0].is_empty() {
            self.projection.insert(coordinates.0 + 1, Vec::new());
            return;
        }

        let new_line = self.projection[coordinates.0].split_off(coordinates.1 + 1);
        self.projection.insert(coordinates.0 + 1, new_line);
    }

    /// Looks up the `(id, peer)` currently sitting at `(line, column)` in the
    /// projection. Returns `None` if either coordinate is out of bounds, never
    /// panics.
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

    /// Looks up `line_number`'s start-of-line anchor — the `(id, peer)` of the
    /// newline (or root sentinel, for line 0) that a front-of-line insert on
    /// this line should be parented on. See `line_start_ids`. Returns `None`
    /// if the line number is out of bounds.
    pub fn get_line_id_peer(&self, line_number: usize) -> Option<(IdSize, PeerIdSize)> {
        let id_peer = self.line_start_ids.get(line_number);

        match id_peer {
            Some(id_peer) => {
                return Some(id_peer.clone());
            }

            None => None,
        }
    }

    /// Finds `(id, peer)`'s *current* position in the projection, treating
    /// `line_num` only as a starting guess, never as ground truth.
    ///
    /// Checks `line_num` itself first, then rings outward — one line up, one
    /// line down, two up, two down, and so on — until it finds a match or runs
    /// off both ends of the file. This is what makes a stale hint (e.g. a
    /// remote op whose sender computed `line_num` before seeing concurrent
    /// local edits that shifted lines around) a *performance* cost rather than
    /// a correctness bug: the search always terminates on the real position,
    /// it just costs more the further off the hint was.
    ///
    /// `(0, 0)` short-circuits immediately — that's the root sentinel, which
    /// was never inserted into the projection in the first place.
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

    /// Resolves where a new child of `(id, peer)` should be inserted, when
    /// `(id, peer)`'s own relation says its *parent* has already been deleted.
    ///
    /// A tombstoned node has no position in the projection to anchor on
    /// (`find_crdt` would find nothing), so this climbs the parent chain
    /// recursively — skipping over each deleted ancestor in turn — until it
    /// reaches either a live ancestor (resolved via `find_crdt`) or the root
    /// sentinel, then walks forward from there applying the same id/peer
    /// tie-break rule `add_crdt` uses for ordinary siblings. In effect: figure
    /// out where the deleted node *would* still be if it hadn't been removed,
    /// so a child parented on it lands in the right place relative to its
    /// still-live siblings.
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
                    }
                    // don't start recursing if the parent is the sentinel
                    else if (relation.parent_id, relation.parent_peer) == (0, 0) {
                        coordinates = (0, 0);
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

    /// Inserts a single character into the tree.
    ///
    /// `line_num` is a ring-search hint for resolving `crdt`'s parent, not a
    /// guaranteed final position — see `find_crdt`. `start_line` marks a
    /// front-of-line insert (parent is a newline or the root sentinel, neither
    /// of which live in the projection), which resolves via `line_start_ids`
    /// instead of a coordinate search.
    ///
    /// Once a parent position is resolved, new siblings are ordered by walking
    /// forward from it and comparing against whatever's already there: a
    /// larger id goes first, and equal ids (a same-tick concurrent insert from
    /// two different peers) tie-break on the smaller peer id going first. This
    /// walk is what guarantees replicas converge regardless of what order they
    /// apply concurrent inserts in — see `test_convergence`.
    ///
    /// Returns `Ok(None)` for a duplicate/retried op that's already applied
    /// (a no-op, not an error), `Ok(Some(position))` for a real insert, and
    /// `Err` if the parent doesn't exist on this replica yet (the "out of
    /// order delivery" case — see `SharQueue::add_network_operation`, which is
    /// what actually catches this and backlogs the op for later).
    pub fn add_crdt(
        &mut self,
        line_num: usize,
        crdt: CRDT,
        start_line: bool,
    ) -> Result<Option<(usize, usize)>> {
        let id = crdt.id;
        let peer = crdt.peer;
        let relation = &crdt.relation;
        let parent_exists: bool;

        // a retry/resend of an op we've already applied is a no-op, not a duplicate insert
        if self.characters.contains_key(&(id, peer)) {
            return Ok(None);
        }

        if !self.characters.is_empty() {
            if let Some(par) = self
                .characters
                .get(&(relation.parent_id, relation.parent_peer))
            {
                parent_exists = !par.deleted;
            } else {
                return Err(Error::Generic(String::from("Parent does not exist")));
            }
        } else {
            parent_exists = true;
        }

        // the first character of a line has no real projected predecessor to look up (its
        // parent may be a newline, which is deliberately never stored in the projection) — the
        // line itself is already known, so there's nothing to search for
        let parent = if start_line {
            if !self
                .line_start_ids
                .contains(&(relation.parent_id, relation.parent_peer))
            {
                self.line_start_ids
                    .insert(line_num, (relation.parent_id, relation.parent_peer));
            }
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
                    return Ok(Some(coordinates));
                }

                // an empty line has no siblings to compare against, so the new character is simply
                // the only thing on it
                if self.projection[coordinates.0].is_empty() {
                    self.projection[coordinates.0].push(insertion_value);
                    return Ok(Some((coordinates.0, 0)));
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
                            return Ok(Some((coordinates.0, j + offset)));
                        } else if next_element.0 < id {
                            // if the next element has the same parent but a smaller id, put this one
                            // first
                            self.projection[coordinates.0].insert(j + offset, insertion_value);
                            return Ok(Some((coordinates.0, j + offset)));
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
                                return Ok(Some((coordinates.0, j + offset)));
                            }
                        }
                    } else {
                        // just put to the end of the line, assuming next element doesn't exist because
                        // we're at the end
                        self.projection[coordinates.0].push((id, peer));
                        return Ok(Some((
                            coordinates.0,
                            self.projection[coordinates.0].len() - 1,
                        )));
                    }
                }

                Ok(None)
            }

            Err(_e) => Err(Error::OutOfBounds(String::from(
                "parent could not be found",
            ))),
        }
    }

    /// Tombstones a character and drops it from the projection.
    ///
    /// Idempotent: removing an already-deleted `(id, peer)` is a no-op that
    /// returns `Ok(None)`, not an error. Removing an id that was never inserted
    /// at all is an `Err`.
    ///
    /// If `(id, peer)` is itself a line's start-of-line anchor (a newline),
    /// removing it merges that line onto the one above it — splicing its
    /// projection contents onto the previous line, dropping the now-empty line
    /// and its `line_start_ids` entry, and shifting every later line's anchor
    /// down by one index to match. The root sentinel can never be a removal
    /// target (nothing calls this with `(0, 0)`), so there's always a "line
    /// above" to merge into. On success, returns `Some((row, col, true))` for
    /// this line-merge case, or `Some((row, col, false))` for an ordinary
    /// single-character removal — the caller uses that flag to know whether a
    /// merge happened.
    pub fn remove_crdt(
        &mut self,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
    ) -> Result<Option<(usize, usize, bool)>> {
        // remove the crdt from the HashMap
        let crdt_relation = self.characters.get_mut(&(id, peer));

        match crdt_relation {
            Some(val) => {
                if val.deleted {
                    return Ok(None);
                }
                val.deleted = true;
            }

            None => return Err(Error::Generic(String::from("crdt cannot be found"))),
        }

        // check if this is one of the line
        if let Some(line) = self.line_start_ids.iter().position(|&x| x == (id, peer)) {
            self.line_start_ids.remove(line);

            // fix the projection
            let mut to_be_deleted = self.projection[line].clone();

            // append deleted line to line above it
            // REMEMBER at the IDE level, you are unable to remove the sentinel character, so
            // this won't break in that case since that case never arrives
            self.projection[line - 1].append(&mut to_be_deleted);

            // delete the line
            self.projection.remove(line);
            return Ok(Some((line, 0, true)));
        }

        // find the value in the projection and delete it
        let position = self.find_crdt(line_num, id, peer);

        match position {
            Ok(pos) => {
                self.projection[pos.0].remove(pos.1);
                Ok(Some((pos.0, pos.1, false)))
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

/// One directory in the shar, holding its own files and recursively mirroring
/// its subdirectories. Every operation on a specific file is routed here by
/// path (see [`Self::find_file`]) and delegated to the matching [`SharFile`].
pub struct SharDirectory {
    /// This directory's own path.
    dir_name: PathBuf,
    /// Direct child directories, each recursively holding its own tree.
    sub_dir: Vec<SharDirectory>,
    /// Files directly inside this directory (not in a subdirectory).
    sub_files: Vec<SharFile>,
}

impl SharDirectory {
    /// Recursively walks `dir_path`, loading every file into a [`SharFile`]
    /// and every subdirectory into a nested `SharDirectory`. `counter` is
    /// threaded through and shared across every file in the whole tree, so ids
    /// are unique across the entire directory, not just within one file.
    ///
    /// Doesn't yet support symlinks anywhere in the tree being initialized.
    pub fn new(dir_path: PathBuf, counter: &mut u32) -> Result<Self> {
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
                        sub_dir_vector.push(Self::new(entry.path(), counter)?);
                    } else if entry_type.is_file() {
                        let file = SharFile::new(entry.path(), counter)?;

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

    /// Routes to `file_path`'s [`SharFile`] and delegates — see
    /// [`SharFile::add_crdt`] for the actual insertion logic.
    pub fn add_crdt(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
        crdt: CRDT,
        start_line: bool,
    ) -> Result<Option<(usize, usize)>> {
        let path = file_path.iter();

        // recursively search for the end of the path
        if let Some(file) = self.find_file(path) {
            return file.add_crdt(line_num, crdt, start_line);
        } else {
            Err(Error::Generic(String::from("File not found")))
        }
    }

    /// Routes to `file_path`'s [`SharFile`] and delegates — see
    /// [`SharFile::remove_crdt`] for the actual removal logic.
    pub fn remove_crdt(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
        id: IdSize,
        peer: PeerIdSize,
    ) -> Result<Option<(usize, usize, bool)>> {
        let path = file_path.iter();

        // recursively search for the end of the path
        if let Some(file) = self.find_file(path) {
            return file.remove_crdt(line_num, id, peer);
        } else {
            Err(Error::Generic(String::from("File not found")))
        }
    }

    /// Routes to `file_path`'s [`SharFile`] and delegates — see
    /// [`SharFile::get_id_peer`].
    pub fn get_id_peer(
        &mut self,
        file_path: &PathBuf,
        pos: (usize, usize),
    ) -> Result<Option<(IdSize, PeerIdSize)>> {
        let path = file_path.iter();

        if let Some(file) = self.find_file(path) {
            return Ok(file.get_id_peer(pos));
        } else {
            Err(Error::Generic(String::from("File not found")))
        }
    }

    /// Routes to `file_path`'s [`SharFile`] and delegates — see
    /// [`SharFile::get_line_id_peer`].
    pub fn get_line_id_peer(
        &mut self,
        file_path: &PathBuf,
        line_num: usize,
    ) -> Result<Option<(IdSize, PeerIdSize)>> {
        let path = file_path.iter();

        if let Some(file) = self.find_file(path) {
            return Ok(file.get_line_id_peer(line_num));
        } else {
            Err(Error::Generic(String::from("File not found")))
        }
    }

    /// Finds the `SharFile` at `path`, recursing into subdirectories as needed.
    ///
    /// At each level, `path`'s components matching this directory's own
    /// `dir_name` are consumed first, then the next component is matched
    /// against either `sub_files` (if it's the last component — a file) or
    /// `sub_dir` (otherwise, recursing with the *original*, unstripped path
    /// passed down fresh, since each level's own `dir_name` is always a valid
    /// prefix of the true full path, at any depth).
    fn find_file<'a>(&'a mut self, mut path: std::path::Iter<'_>) -> Option<&'a mut SharFile> {
        // check if there even is anything in here?
        if self.sub_dir.is_empty() && self.sub_files.is_empty() {
            return None;
        }

        // clone the path for recursive use
        let path_copy = path.clone();

        let root = self.dir_name.iter();

        // use up the iterator until it gets past the root of the shar
        for i in root {
            let name = path.next();

            match name {
                Some(n) => {
                    if i == n {
                        continue;
                    } else {
                        return None;
                    }
                }
                None => {
                    return None;
                }
            };
        }

        // do we still have runway in the provided path?
        if let Some(next) = path.next() {
            // is the provided path the file?
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
                        return dir.find_file(path_copy);
                    }
                }
                return None;
            }
        } else {
            return None;
        }
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
