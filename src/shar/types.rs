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
/// value: Value -> the UTF-8 bytes of this specific character (one char per node)
/// id: u8 -> the id of this specific character
#[derive(Clone, Debug)]
pub struct CRDT {
    pub value: Value,
    pub id: IdSize,
    pub peer_id: PeerIdSize,
}

impl CRDT {
    /// Creates a new CRDT
    pub fn new(value: char, id: IdSize, peer_id: PeerIdSize) -> Self {
        // one character per node: store its UTF-8 bytes (1-4 bytes)
        let mut buf = [0u8; 4];
        let bytes = value.encode_utf8(&mut buf).as_bytes().to_vec();

        CRDT {
            value: bytes,
            id: id,
            peer_id: peer_id,
        }
    }

    /// Serializes a CRDT into a length-prefixed, big-endian byte layout:
    ///
    /// [value_len]  [value]              [id]        [peer_id]
    /// [1 byte]     [value_len bytes]    [4 bytes]   [1 byte]
    ///
    /// `value` is the raw UTF-8 bytes of the character (1-4 bytes for a single
    /// Unicode scalar), so the value section is byte-identical on every machine.
    /// `id` is written big-endian. A single scalar is at most 4 bytes, so the
    /// 1-byte length prefix is always sufficient.
    pub fn to_bytes(self) -> Vec<u8> {
        // 1 length byte + value + 4-byte id + 1-byte peer_id
        let mut output = Vec::with_capacity(1 + self.value.len() + 5);

        output.push(self.value.len() as u8);
        output.extend_from_slice(&self.value);
        output.extend_from_slice(&self.id.to_be_bytes());
        output.push(self.peer_id);

        output
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
    ///[crdt]              [operation_type]
    ///[length-prefixed]   [2 bytes]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut output = self.crdt.to_bytes();
        output.extend_from_slice(&self.operation_type.value());

        output
    }
}
