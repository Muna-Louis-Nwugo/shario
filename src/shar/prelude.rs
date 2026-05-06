// general imports that will be used throughout the project

pub use super::error::Error;
pub use crate::shar::types;
pub use tokio::io;

// Result Alias
pub type Result<T> = core::result::Result<T, Error>;

// IO Result alias
pub type IOResult<T> = io::Result<T>;

// wrapper tuple struct (newtype pattern)
pub struct W<T>(pub T);

pub type CRDT = types::CRDT;

pub type OperationType = types::OperationType;

pub type Operation = types::Operation;

pub const ANCHOR_BOUNDARY: usize = types::ANCHOR_BOUNDARY;

pub type ID_SIZE = types::ID_SIZE;

pub type ANCHOR_ID_SIZE = types::ANCHOR_ID_SIZE;
