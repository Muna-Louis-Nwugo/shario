/* Shar Operation Standard*/

pub struct Operation {
    // chatacter is an option since not all operations will involve characters. E.g. creating a new
    // file
    character: Option<char>,
    operation_type: OperationType,
    position: i64,
}

pub enum OperationType {
    /* represents types of operations that can be performed */
    AddChar,
    RemoveChar,
}

impl OperationType {
    pub fn value(self) -> [u8; 2] {
        match self {
            OperationType::AddChar => [0u8, 1u8],
            OperationType::RemoveChar => [1u8, 0u8],
        }
    }
}

impl Operation {
    pub fn new(character: Option<char>, operation_type: OperationType, position: i64) -> Self {
        Operation {
            character: character,
            operation_type: operation_type,
            position: position,
        }
    }

    pub fn to_bytes(self) -> [u8; 14] {
        let mut send_character: [u8; 4] = [0, 0, 0, 0];

        match self.character {
            Some(character) => {
                character.encode_utf8(&mut send_character);
            }
            // Just put 0 in the buffer. It won't be accessed anyways in this case
            None => {
                send_character = [0, 0, 0, 0];
            }
        }

        let operation_bytes: [u8; 2] = self.operation_type.value();

        let position_bytes: [u8; 8] = self.position.to_be_bytes();

        let mut output = send_character
            .into_iter()
            .chain(operation_bytes)
            .chain(position_bytes);

        std::array::from_fn(|_| output.next().unwrap())
    }
}
