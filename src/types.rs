//! CRDT node types and the `Operation` types used to move edits around.

// GLOBAL VARIABLES

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A node's value, parent pointer, and deleted flag.
#[derive(Copy, PartialEq, Clone, Debug, Deserialize, Serialize)]
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
#[derive(Copy, PartialEq, Clone, Debug, Deserialize, Serialize)]
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

/// A single character remote insertion. See
/// [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct NetworkAdd {
    /// The file this insertion belongs to.
    pub file_path: PathBuf,
    /// The character being inserted, with its resolved parent.
    pub crdt: CRDT,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
    /// Whether this is a front-of-line insert.
    pub start_line: bool,
}

impl NetworkAdd {
    pub fn new(path: PathBuf, crdt: CRDT, row: usize, start_line: bool) -> Self {
        NetworkAdd {
            file_path: path,
            crdt: crdt,
            row: row,
            start_line: start_line,
        }
    }
}

/// A single character remote removal — just a target `(id, peer)`,
/// no value or parent needed. See [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkRemove {
    /// The file this removal belongs to.
    pub file_path: PathBuf,
    /// Target id.
    pub id: u32,
    /// Target peer.
    pub peer: u8,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
}

impl NetworkRemove {
    pub fn new(file_path: PathBuf, id: u32, peer: u8, row: usize) -> Self {
        NetworkRemove {
            file_path: file_path,
            id: id,
            peer: peer,
            row: row,
        }
    }
}

/// A single character IDE insertion. See
/// [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdeAdd {
    /// The file this addition belongs to
    pub file_path: PathBuf,
    /// the row of the this addition's parent
    pub parent_row: usize,
    /// the col of this addition's parent
    pub parent_col: usize,
    /// the value of this addition
    pub val: char,
    /// is this position at the beginning of a line?
    pub start_line: bool,
}

impl IdeAdd {
    pub fn new(
        file_path: PathBuf,
        parent_row: usize,
        parent_col: usize,
        val: char,
        start_line: bool,
    ) -> Self {
        IdeAdd {
            file_path: file_path,
            parent_row: parent_row,
            parent_col: parent_col,
            val: val,
            start_line: start_line,
        }
    }
}

/// A single character IDE removal — just a target `(id, peer)`,
/// no value or parent needed. See [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Debug, Deserialize)]
pub struct IdeRemove {
    /// The file this removal belongs to
    pub file_path: PathBuf,
    /// the row of the this removal
    pub row: usize,
    /// the col of this removal
    pub col: usize,
    /// is this removal a line?
    pub is_whole_line: bool,
}

impl IdeRemove {
    pub fn new(file_path: PathBuf, row: usize, col: usize, is_whole_line: bool) -> Self {
        IdeRemove {
            file_path: file_path,
            row: row,
            col: col,
            is_whole_line: is_whole_line,
        }
    }
}

#[derive(Debug, Clone)]
pub enum IdeOp {
    ADD (IdeAdd),
    REMOVE (IdeRemove),
}


#[derive(Debug, Clone)]
pub enum NetworkOp {
    ADD (NetworkAdd),
    REMOVE (NetworkRemove), 
}

/// A Websocket connection message
#[derive(Debug, Deserialize)]
pub struct Connect {
    /// Is this a local IDE connection? True -> Yes, False -> No (network connection)
    pub local: bool,
    /// The path of the connection
    pub path: PathBuf,
}
