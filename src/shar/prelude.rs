// general imports that will be used throughout the project

pub use super::error::Error;
use crate::shar::types;
use tokio::io;

// Result Alias
pub type Result<T> = core::result::Result<T, Error>;

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

pub type IdSize = types::IdSize;

pub type LineSize = u16;
