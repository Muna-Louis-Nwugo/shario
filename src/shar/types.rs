//! Shar types for crdts and operations

// GLOBAL VARIABLES
/// Types of operations that can be made
pub enum OperationType {
    AddChar,
    RemoveChar,
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
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
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
#[derive(PartialEq, Clone, Debug)]
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
///operation_type: [OperationType] -> The type of operation being performed
///peer: u32 -> The user_id that created the operation
pub struct Operation {
    crdt: CRDT,
    operation_type: OperationType,
}

impl Operation {
    pub fn new(crdt: CRDT, operation_type: OperationType) -> Self {
        Operation {
            crdt: crdt,
            operation_type: operation_type,
        }
    }
}
