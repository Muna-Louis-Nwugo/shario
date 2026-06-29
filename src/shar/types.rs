//! Shar types for crdts and operations

// GLOBAL VARIABLES
pub const ANCHOR_BOUNDARY: usize = 250;
pub type IdSize = u8;

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

/// The two possible character sizes.
///
/// Small: u8 - a single byte character
/// Wide: char - a 4 byte character
///
/// The Shar will attempt to store the value in a single byte, and if that's impossible, it will
/// fall back to a char
pub enum Atom {
    Small(u8),
    Wide(char),
}

/// Represents the chosen CRDT: Replicated Growable Array. Anchors are used to bound tree traversal
/// and keep the footprint of the CRDT as small as possible for serialization
///
/// value: Atom -> the value of this specific character
/// id: u8 -> the id of this specific character
pub struct CRDT {
    pub value: Atom,
    pub id: IdSize,
}

impl CRDT {
    /// Creates a new CRDT
    pub fn new(value: char, id: IdSize) -> Self {
        let potential_u8 = u8::try_from(value);

        match potential_u8 {
            Ok(byte) => CRDT {
                value: Atom::Small(byte),
                id: id,
            },

            Err(_e) => CRDT {
                value: Atom::Wide(value),
                id: id,
            },
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
        let value: [u8; 4];

        match self.value {
            Atom::Small(a) => {
                value = (a as u32).to_be_bytes();
            }

            Atom::Wide(b) => {
                value = (b as u32).to_be_bytes();
            }
        }

        for (i, val) in value.into_iter().enumerate() {
            byte_array[i] = val;
        }

        // copy ID and parent_id into byte_array
        byte_array[4] = self.id;

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
    pub fn to_bytes(self) -> [u8; 15] {
        let send_character: [u8; 4] = [0, 0, 0, 0];

        let crdt = self.crdt.to_bytes();

        let operation_type: [u8; 2] = self.operation_type.value();

        let mut output = send_character.into_iter().chain(crdt).chain(operation_type);

        std::array::from_fn(|_| output.next().unwrap())
    }
}
