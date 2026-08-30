//! Common imports and type aliases used throughout `shar`. Modules that need the
//! basics (the crate's `Result`, the CRDT types, or the id/peer primitive sizes)
//! pull them in with a single `use crate::shar::prelude::*;`.

pub use super::error::Error;
use crate::shar::types;
use tokio::io;

/// The crate-wide `Result` alias — every fallible `shar` operation returns this
/// instead of a bespoke per-module error type.
pub type Result<T> = core::result::Result<T, Error>;

/// Alias for `tokio::io::Result`, for code that's doing raw async IO rather than
/// a `shar`-level operation.
pub type IOResult<T> = io::Result<T>;

/// A generic single-field newtype wrapper, for giving a foreign type a distinct
/// identity where the orphan rules would otherwise block a trait impl.
pub struct W<T>(pub T);

// GLOBAL STRUCTS

/// Re-exported from [`crate::shar::types`] for convenience — see that module for
/// the real definition.
pub type CrdtRelation = types::CrdtRelation;

/// Re-exported from [`crate::shar::types`] for convenience — see that module for
/// the real definition.
pub type CRDT = types::CRDT;

// GLOBAL VARIABLES / PRIMITIVE TYPE ALIASES

/// The integer type backing every CRDT node's id. Ids are assigned sequentially
/// from a single, monotonically-increasing counter shared across an entire
/// `SharDirectory` (not per-file), and combined with [`PeerIdSize`] to form the
/// `(id, peer)` pair that's actually globally unique across replicas.
pub type IdSize = u32;

/// The integer type identifying which peer/replica created a given CRDT node.
/// Uniqueness is always on the `(IdSize, PeerIdSize)` pair, never on a bare id —
/// two peers' local counters can and will collide on the same `IdSize` value.
pub type PeerIdSize = u8;
