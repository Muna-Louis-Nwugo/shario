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

/// Represents the chosen CRDT: Replicated Growable Array. Anchors are used to bound tree traversal
/// and keep the footprint of the CRDT as small as possible for serialization
///
/// value: Atom -> the value of this specific character
/// id: u8 -> the id of this specific character
#[derive(Clone, Debug)]
pub struct CRDT {
    pub value: Atom,
    pub id: IdSize,
    pub peer_id: PeerIdSize,
}

impl CRDT {
    /// Creates a new CRDT
    pub fn new(value: char, id: IdSize, peer_id: PeerIdSize) -> Self {
        let atom = Atom::new(value);

        CRDT {
            value: atom,
            id: id,
            peer_id: peer_id,
        }
    }

    /// Serializes a CRDT into bytes as follows:
    ///
    /// [value]     [id]        [parent_id]     [anchor_id]     [peer_id]
    /// [4 bytes]   [1 byte]    [1 byte]        [2 bytes]       [1 byte]
    ///
    /// Yes, I know it's not memory aligned.  Boo hoo.
    pub fn to_bytes(self) -> [Atom; 9] {
        // initialize byte array
        let mut byte_array: [Atom; 9] = [Atom::Small(0); 9];

        // turn value into 4-byte array and move to byte_array
        let value: Atom = self.value;

        byte_array[0] = value;

        // copy ID and parent_id into byte_array
        let id_array = self.id.to_be_bytes();
        let mut n = 4;

        for byte in id_array {
            byte_array[n] = Atom::Small(byte);
            n += 1;
        }

        byte_array
    }
}

/// Represents an operation to be sent accross the grapevine (network)
///
///crdt: [CRDT] -> A CRDT  
///operation_type: [OperationType] -> The type of operation being performed
///peer_id: u32 -> The user_id that created the operation
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

    ///Converts operations into bytes:
    ///
    ///[crdt] [operation_type]
    pub fn to_bytes(self) -> [Atom; 11] {
        let crdt = self.crdt.to_bytes();

        let [a, b, c, d, e, f, g, h, i] = crdt;

        let operation_type: [u8; 2] = self.operation_type.value();

        let [j, k] = [
            Atom::new(operation_type[0] as char),
            Atom::new(operation_type[1] as char),
        ];

        let output = [a, b, c, d, e, f, g, h, i, j, k];

        output
    }
}
