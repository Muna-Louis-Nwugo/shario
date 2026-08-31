//! Common imports/aliases: `use crate::shar::prelude::*;`.

pub use super::error::Error;
use crate::types;
use tokio::io;

/// The crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Alias for `tokio::io::Result`.
pub type IOResult<T> = io::Result<T>;

/// Generic newtype wrapper.
pub struct W<T>(pub T);

// GLOBAL STRUCTS

/// See [`crate::types::CrdtRelation`].
pub type CrdtRelation = types::CrdtRelation;

/// See [`crate::types::CRDT`].
pub type CRDT = types::CRDT;

// GLOBAL VARIABLES / PRIMITIVE TYPE ALIASES

/// A CRDT node's id. Unique only when paired with [`PeerIdSize`].
pub type IdSize = u32;

/// The peer/replica id half of a node's `(id, peer)` identity.
pub type PeerIdSize = u8;
