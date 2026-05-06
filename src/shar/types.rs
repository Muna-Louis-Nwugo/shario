//! Shar types for crdts and operations

// GLOBAL VARIABLES
pub const ANCHOR_BOUNDARY: usize = 250;
pub type ID_SIZE = u8;
pub type ANCHOR_ID_SIZE = u16;

/// Types of operations that can be made
pub enum OperationType {
    AddChar,
    RemoveChar,
    ChangeChar,
}

impl OperationType {
    /// Converts operation types into bytes for serializtion:  
    ///
    ///     AddChar -> 0x00FF  
    ///     RemoveChar -> 0XFF00  
    ///     ChangeChar -> 0XFFFF  
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
/// value: char -> Character value of the type
/// id: u8 -> the id of this specific instance
/// parent_id: u8 -> ID of the parent of this instance (which will be relative to its anchor)
/// anchor_id: u16 -> ID of this instance's anchor
///
/// Note: anchors will be inserted as "characters" in the tree. The first character after an
/// ancor will have a parent_id of 0x0
pub struct CRDT {
    pub value: char,
    pub id: ID_SIZE,
    pub parent_id: ID_SIZE,
    pub anchor_id: ANCHOR_ID_SIZE,
    peer_id: ID_SIZE,
}

impl CRDT {
    /// Creates a new CRDT
    pub fn new(
        value: char,
        id: ID_SIZE,
        parent_id: ID_SIZE,
        anchor_id: ANCHOR_ID_SIZE,
        peer_id: ID_SIZE,
    ) -> Self {
        CRDT {
            value: value,
            id: id,
            parent_id: parent_id,
            anchor_id: anchor_id,
            peer_id: peer_id,
        }
    }

    /// Serializes a CRDT into bytes as follows:
    ///
    /// [value]     [id]        [parent_id]     [anchor_id]     [peer_id]
    /// [4 bytes]   [1 byte]    [1 byte]        [2 bytes]       [1 byte]
    ///
    /// Yes, I know it's not memory aligned.  Boo hoo.
    pub fn to_bytes(self) -> [u8; 9] {
        // initialize byte array
        let mut byte_array: [u8; 9] = [0; 9];

        // turn value into 4-byte array and move to byte_array
        let value: [u8; 4] = (self.value as u32).to_be_bytes();

        for (i, val) in value.into_iter().enumerate() {
            byte_array[i] = val;
        }

        // copy ID and parent_id into byte_array
        byte_array[4] = self.id;
        byte_array[5] = self.parent_id;

        // turn anchor_id into 2-byte array and move to byte_array
        let anchor: [u8; 2] = self.anchor_id.to_be_bytes();

        for (i, val) in anchor.into_iter().enumerate() {
            byte_array[i + 5] = val;
        }

        // copy peer_id into the array
        byte_array[8] = self.peer_id;

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
    pub fn new(crdt: CRDT, operation_type: OperationType, position: i64) -> Self {
        Operation {
            crdt: crdt,
            operation_type: operation_type,
        }
    }

    ///Converts operations into bytes:
    ///
    ///[crdt] [operation_type]
    pub fn to_bytes(self) -> [u8; 15] {
        let mut send_character: [u8; 4] = [0, 0, 0, 0];

        let crdt = self.crdt.to_bytes();

        let operation_type: [u8; 2] = self.operation_type.value();

        let mut output = send_character.into_iter().chain(crdt).chain(operation_type);

        std::array::from_fn(|_| output.next().unwrap())
    }
}
