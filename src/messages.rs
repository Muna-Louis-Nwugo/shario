//* Possible messages

/// An input message
pub struct InputMessage {
    pub command: InputCommand,
    pub arguments: Vec<String>,
}

/// All available commands for the input thread
pub enum InputCommand {
    AddCRDT,
}

