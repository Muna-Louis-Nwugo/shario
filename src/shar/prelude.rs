// general imports that will be used throughout the project

pub use super::error::Error;
pub use crate::shar::types;
pub use tokio::io;

// Result Alias
pub type Result<T> = std::result::Result<T, Error>;

// IO Result alias
pub type IOResult<T> = io::Result<T>;

// wrapper tuple struct (newtype pattern)
pub struct W<T>(pub T);

// GLOBAL STRUCTS

pub type CRDT = types::CRDT;

pub type OperationType = types::OperationType;

pub type Operation = types::Operation;

// GLOBAL VARIABLES / PRIMITIVE TYPE ALIASES
pub const ANCHOR_LENGTH: usize = types::ANCHOR_BOUNDARY;

pub type ID_SIZE = types::ID_SIZE;

pub type ANCHOR_ID_SIZE = types::ANCHOR_ID_SIZE;
