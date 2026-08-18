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

pub type CrdtRelation = types::CrdtRelation;

pub type CRDT = types::CRDT;

pub type OperationType = types::OperationType;

// GLOBAL VARIABLES / PRIMITIVE TYPE ALIASES

pub type IdSize = u32;

pub type PeerIdSize = u8;

/// The value of a single node: one Unicode scalar value.
pub type Value = char;
