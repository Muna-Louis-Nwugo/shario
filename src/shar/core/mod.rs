//! The actual CRDT engine: the tree that stores/resolves characters
//! ([`tree`]), the queue that mediates access to it ([`queue`]), and a
//! placeholder persistence layer ([`buffer`]).

pub mod buffer;
pub mod queue;
pub mod tree;
