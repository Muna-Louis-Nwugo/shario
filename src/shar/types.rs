//! Shar types for crdts and operations

use crate::prelude::*;

// GLOBAL VARIABLES
/// Types of operations that can be made
pub enum OperationType {
    AddChar,
    RemoveChar,
    ChangeChar,
}

impl OperationType {
    /// Converts operation types into bytes for serializtion:  
    ///
    /// AddChar is 0x00FF  
    /// RemoveChar is 0XFF00  
    /// ChangeChar is 0XFFFF  
    pub fn value(self) -> [u8; 2] {
        match self {
            OperationType::AddChar => [0u8, 1u8],
            OperationType::RemoveChar => [1u8, 0u8],
            OperationType::ChangeChar => [1u8, 1u8],
        }
    }
}

#[derive(Clone, Debug)]
pub struct CrdtRelation {
    value: Vec<u8>,
    parent_id: u32,
    parent_peer: u8,
}

impl CrdtRelation {
    pub fn new(value: Vec<u8>, parent_id: u32, parent_peer: u8) -> Self {
        CrdtRelation {
            value: value,
            parent_id: parent_id,
            parent_peer: parent_peer,
        }
    }
}
