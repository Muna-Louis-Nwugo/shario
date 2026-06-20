//* Possible messages

/// A message inbound via web sockets
#[derive(Debug, serde::Deserialize)]
pub struct MessageIn {
    pub room: String,
    pub text: String,
}
/// An input message
pub struct InputMessage {
    pub command: InputCommand,
    pub arguments: Vec<String>,
}

/// All available commands for the input thread
pub enum InputCommand {
    AddCRDT,
}

#[derive(serde::Serialize)]
pub struct MessageOut {
    pub success: bool,
    pub message: String,
}
