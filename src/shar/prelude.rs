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

/// The two possible character sizes.
///
/// Small: u8 - a single byte character
/// Wide: char - a 4 byte character
///
/// The Shar will attempt to store the value in a single byte, and if that's impossible, it will
/// fall back to a char
#[derive(Debug, Clone, Copy)]
pub enum Atom {
    Small(u8),
    Wide(char),
}

impl Atom {
    pub fn new(character: char) -> Atom {
        let potential_u8 = u8::try_from(character);

        match potential_u8 {
            Ok(byte) => Atom::Small(byte),

            Err(_e) => Atom::Wide(character),
        }
    }
}
