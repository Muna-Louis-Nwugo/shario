/* Shar Operation Standard*/

pub struct Operation {
    character: Option<char>,
    operation_type: OperationType,
    position: i32,
}

pub enum OperationType {
    AddChar,
    RemoveChar,
}

