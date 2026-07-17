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

pub type IdSize = u32;

pub type PeerIdSize = u8;

pub type LineSize = u16;

/// The value of a single node: the UTF-8 bytes of one character (1-4 bytes for a
/// single Unicode scalar). Raw bytes have no endianness, so a value is identical
/// on every machine as long as both ends agree on UTF-8.
pub type Value = Vec<u8>;
