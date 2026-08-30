//! The CRDT node types and the `Operation` types used to move edits between the
//! IDE, the local tree, and (eventually) the network.

// GLOBAL VARIABLES

use std::path::PathBuf;

/// The mutable part of a CRDT node: its character, where it's parented, and
/// whether it's been tombstoned. Split out from [`CRDT`] because this is the
/// piece that actually changes over a node's lifetime (`deleted` flips from
/// `false` to `true` on removal; everything else is fixed at creation).
#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CrdtRelation {
    /// The character this node represents.
    pub value: char,
    /// The id half of this node's parent's `(id, peer)` identity. `(0, 0)` is
    /// reserved for the root sentinel — "nothing before this in the document."
    pub parent_id: u32,
    /// The peer half of this node's parent's `(id, peer)` identity.
    pub parent_peer: u8,
    /// Whether this node has been tombstoned (removed). Tombstones stay in
    /// `SharFile::characters` forever right now — there's no garbage collection
    /// yet — they're just dropped out of the line projection.
    pub deleted: bool,
}

impl CrdtRelation {
    /// Builds a fresh, live (`deleted: false`) relation for a newly-created node.
    pub fn new(value: char, parent_id: u32, parent_peer: u8) -> Self {
        CrdtRelation {
            value: value,
            parent_id: parent_id,
            parent_peer: parent_peer,
            deleted: false,
        }
    }
}

/// A [`CrdtRelation`] together with the identity (`id`, `peer`) of the node it
/// describes. `(id, peer)` — not a bare id — is what's actually globally unique
/// across replicas, since two peers' local id counters can collide.
#[derive(Copy, PartialEq, Clone, Debug)]
pub struct CRDT {
    /// This node's id, unique only when paired with `peer`.
    pub id: u32,
    /// The peer/replica that created this node.
    pub peer: u8,
    /// This node's value and parent-pointer state.
    pub relation: CrdtRelation,
}

impl CRDT {
    pub fn new(id: u32, peer: u8, relation: CrdtRelation) -> Self {
        CRDT {
            id: id,
            peer: peer,
            relation: relation,
        }
    }
}

/// A single local (IDE-originated) or remote character insertion, as handed
/// back by [`crate::shar::core::queue::SharQueue::add_ide_crdt`] for outbound
/// transmission, or handed to
/// [`crate::shar::core::queue::SharQueue::add_network_operation`] on receipt.
#[derive(Clone)]
pub struct AddOperation {
    /// Which file in the shar this insertion belongs to.
    pub file_path: PathBuf,
    /// The character being inserted, already carrying its resolved parent.
    pub crdt: CRDT,
    /// The projection line this insertion targets (used as a ring-search hint,
    /// not trusted as the final position — see `SharFile::find_crdt`).
    pub row: usize,
    /// Whether this is a front-of-line insert (parent is a newline or the root
    /// sentinel, neither of which live in the projection) rather than a normal
    /// insert anchored on a real projected character.
    pub start_line: bool,
}

impl AddOperation {
    pub fn new(path: PathBuf, crdt: CRDT, row: usize, start_line: bool) -> Self {
        AddOperation {
            file_path: path,
            crdt: crdt,
            row: row,
            start_line: start_line,
        }
    }
}

/// A single local or remote character removal, as handed back by
/// [`crate::shar::core::queue::SharQueue::remove_ide_crdt`] for outbound
/// transmission, or handed to
/// [`crate::shar::core::queue::SharQueue::remove_network_operation`] on receipt.
///
/// Unlike [`AddOperation`], this carries just the target's `(id, peer)` rather
/// than a full `CRDT` — a removal doesn't need a value or a parent, only an
/// identity to tombstone.
#[derive(Clone)]
pub struct RemoveOperation {
    /// Which file in the shar this removal belongs to.
    pub file_path: PathBuf,
    /// The id half of the target node's `(id, peer)` identity.
    pub id: u32,
    /// The peer half of the target node's `(id, peer)` identity.
    pub peer: u8,
    /// The projection line this removal targets (used as a ring-search hint,
    /// same as [`AddOperation::row`]).
    pub row: usize,
}

impl RemoveOperation {
    pub fn new(file_path: PathBuf, id: u32, peer: u8, row: usize) -> Self {
        RemoveOperation {
            file_path: file_path,
            id: id,
            peer: peer,
            row: row,
        }
    }
}
