//! Shar types for crdts and operations

// GLOBAL VARIABLES

use std::path::PathBuf;

use axum::extract::Path;

#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CrdtRelation {
    pub value: char,
    pub parent_id: u32,
    pub parent_peer: u8,
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

/// A CrdtRelation together with the identity (id, peer) of the node it describes.
#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CRDT {
    pub id: u32,
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

///crdt: [CRDT] -> A CRDT
///peer: u32 -> The user_id that created the operation
#[derive(Clone)]
pub struct AddOperation {
    pub file_path: PathBuf,
    pub crdt: CRDT,
    pub row: usize,
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

///id: [u32] -> The id of the crdt being removed
///peer: [u8] -> The peer of the crdt being removed
///is_whole_line: [bool] -> Whether the target is a whole-line anchor removal
#[derive(Clone)]
pub struct RemoveOperation {
    pub file_path: PathBuf,
    pub id: u32,
    pub peer: u8,
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
