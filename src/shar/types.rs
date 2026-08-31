//! CRDT node types and the `Operation` types used to move edits around.

// GLOBAL VARIABLES

use std::path::PathBuf;

/// A node's value, parent pointer, and deleted flag.
#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CrdtRelation {
    /// The character this node represents.
    pub value: char,
    /// Parent's id. `(0, 0)` is the root sentinel.
    pub parent_id: u32,
    /// Parent's peer.
    pub parent_peer: u8,
    /// Whether this node has been tombstoned.
    pub deleted: bool,
}

impl CrdtRelation {
    pub fn new(value: char, parent_id: u32, parent_peer: u8) -> Self {
        CrdtRelation {
            value: value,
            parent_id: parent_id,
            parent_peer: parent_peer,
            deleted: false,
        }
    }
}

/// A [`CrdtRelation`] plus the `(id, peer)` identity of the node it describes.
#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CRDT {
    /// Unique when paired with `peer`.
    pub id: u32,
    /// The peer/replica that created this node.
    pub peer: u8,
    pub relation: CrdtRelation,
}

impl CRDT {
    pub fn new(id: u32, peer: u8, relation: CrdtRelation) -> Self {
        CRDT {
            id: id,
            peer: peer,
            relation: relation,
        }
    }
}

/// A single character insertion, local or remote. See
/// [`crate::shar::core::queue::SharQueue`].
#[derive(Clone)]
pub struct AddOperation {
    /// The file this insertion belongs to.
    pub file_path: PathBuf,
    /// The character being inserted, with its resolved parent.
    pub crdt: CRDT,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
    /// Whether this is a front-of-line insert.
    pub start_line: bool,
}

impl AddOperation {
    pub fn new(path: PathBuf, crdt: CRDT, row: usize, start_line: bool) -> Self {
        AddOperation {
            file_path: path,
            crdt: crdt,
            row: row,
            start_line: start_line,
        }
    }
}

/// A single character removal, local or remote — just a target `(id, peer)`,
/// no value or parent needed. See [`crate::shar::core::queue::SharQueue`].
#[derive(Clone)]
pub struct RemoveOperation {
    /// The file this removal belongs to.
    pub file_path: PathBuf,
    /// Target id.
    pub id: u32,
    /// Target peer.
    pub peer: u8,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
}

impl RemoveOperation {
    pub fn new(file_path: PathBuf, id: u32, peer: u8, row: usize) -> Self {
        RemoveOperation {
            file_path: file_path,
            id: id,
            peer: peer,
            row: row,
        }
    }
}
