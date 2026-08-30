//! `shar` is shario's CRDT engine: a tree/RGA-style, id-keyed character store
//! (see [`core::tree`]) mediated by a queue (see [`core::queue::SharQueue`])
//! that sits between the IDE on one side and the network on the other. Nothing
//! outside `shar` talks to the tree directly — everything goes through
//! `SharQueue`'s `add_ide_crdt`/`remove_ide_crdt` (local edits, applied
//! synchronously) and `add_network_operation`/`remove_network_operation`
//! (remote edits, which can arrive out of order and get backlogged until their
//! dependency shows up).

pub mod core;
pub mod error;
pub mod io;
pub mod prelude;
#[cfg(test)]
mod tests;
mod types;
