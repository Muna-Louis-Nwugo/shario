//! CRDT node types and the `Operation` types used to move edits around.

// GLOBAL VARIABLES

use core::fmt;
use std::{net::IpAddr, path::PathBuf, str::FromStr};

use iroh::PublicKey;
use iroh_gossip::TopicId;

use serde::{Deserialize, Serialize};

use crate::shar::prelude::{IdSize, PeerIdSize};

/// A node's value, parent pointer, and deleted flag.
#[derive(Copy, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct CrdtRelation {
    /// The character this node represents.
    pub value: char,
    /// Parent's id. `(0, 0)` is the root sentinel.
    pub parent_id: IdSize,
    /// Parent's peer.
    pub parent_peer: PeerIdSize,
    /// Whether this node has been tombstoned.
    pub deleted: bool,
}

impl CrdtRelation {
    pub fn new(value: char, parent_id: IdSize, parent_peer: PeerIdSize) -> Self {
        CrdtRelation {
            value: value,
            parent_id: parent_id,
            parent_peer: parent_peer,
            deleted: false,
        }
    }
}

/// A [`CrdtRelation`] plus the `(id, peer)` identity of the node it describes.
#[derive(Copy, PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct CRDT {
    /// Unique when paired with `peer`.
    pub id: IdSize,
    /// The peer/replica that created this node.
    pub peer: PeerIdSize,
    pub relation: CrdtRelation,
}

impl CRDT {
    pub fn new(id: IdSize, peer: PeerIdSize, relation: CrdtRelation) -> Self {
        CRDT {
            id: id,
            peer: peer,
            relation: relation,
        }
    }
}

/// A single character remote insertion. See
/// [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct NetworkAdd {
    /// The file this insertion belongs to.
    pub file_path: PathBuf,
    /// The character being inserted, with its resolved parent.
    pub crdt: CRDT,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
}

impl NetworkAdd {
    pub fn new(path: PathBuf, crdt: CRDT, row: usize) -> Self {
        NetworkAdd {
            file_path: path,
            crdt: crdt,
            row: row,
        }
    }
}

/// A single character remote removal — just a target `(id, peer)`,
/// no value or parent needed. See [`crate::shar::core::queue::SharQueue`].
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Remove {
    /// The file this removal belongs to.
    pub file_path: PathBuf,
    /// Target id.
    pub id: IdSize,
    /// Target peer.
    pub peer: PeerIdSize,
    /// Ring-search hint line, not a guaranteed final position.
    pub row: usize,
}

impl Remove {
    pub fn new(file_path: PathBuf, id: IdSize, peer: PeerIdSize, row: usize) -> Self {
        Remove {
            file_path: file_path,
            id: id,
            peer: peer,
            row: row,
        }
    }
}

/// A single character IDE insertion. See
/// [`crate::shar::core::queue::SharQueue`].
///
/// The parent is referenced by real identity (`parent_id`/`parent_peer`) if
/// already known -- pre-existing content, or something already confirmed --
/// or, if the parent is this same connection's own not-yet-confirmed add, by
/// `parent_tag` instead. Exactly one of the two should be set. Position isn't
/// used at all: a position can go stale (something earlier gets deleted) in a
/// way an identity or a tag can't.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdeAdd {
    /// The file this addition belongs to
    pub file_path: PathBuf,
    pub parent_id: Option<IdSize>,
    pub parent_peer: Option<PeerIdSize>,
    /// Set instead of `parent_id`/`parent_peer` when the parent is this same
    /// connection's own add, sent moments ago and not yet confirmed.
    pub parent_tag: Option<u32>,
    /// the value of this addition
    pub val: char,
    pub tag: u32,
    /// Ring-search starting point only -- has no bearing on correctness.
    /// Unlike `parent_id`/`parent_peer`/`parent_tag`, this is allowed to be
    /// stale: a wrong hint just costs a wider search, never a wrong result.
    pub line_hint: usize,
}

impl IdeAdd {
    pub fn new(
        file_path: PathBuf,
        parent_id: Option<IdSize>,
        parent_peer: Option<PeerIdSize>,
        parent_tag: Option<u32>,
        val: char,
        tag: u32,
        line_hint: usize,
    ) -> Self {
        IdeAdd {
            file_path: file_path,
            parent_id: parent_id,
            parent_peer: parent_peer,
            parent_tag: parent_tag,
            val: val,
            tag: tag,
            line_hint: line_hint,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkOp {
    ADD(NetworkAdd),
    REMOVE(Remove),
}

impl NetworkOp {
    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// A Websocket connection message
#[derive(Debug, Deserialize)]
pub struct Connect {
    /// Is this a local IDE connection? True -> Yes, False -> No (network connection)
    pub local: bool,
    /// The path of the connection
    pub path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct IdeAddConfirmed {
    pub tag: u32,
    pub id: IdSize,
    pub peer: PeerIdSize,
}

impl IdeAddConfirmed {
    pub fn new(tag: u32, id: IdSize, peer: PeerIdSize) -> Self {
        IdeAddConfirmed {
            tag: tag,
            id: id,
            peer: peer,
        }
    }
}

/// A broadcast
#[derive(Debug, Deserialize, Serialize)]
pub struct BroadcastMessage {
    shar_id: String,
    ip_address: IpAddr,
}

impl BroadcastMessage {
    pub fn new(id: String, ip: IpAddr) -> Self {
        BroadcastMessage {
            shar_id: id,
            ip_address: ip,
        }
    }
}

/// A ticket for purposes of signalling
#[derive(Debug, Deserialize, Serialize)]
pub struct Ticket {
    pub topic: TopicId,
    pub peers: Vec<PublicKey>,
}

impl Ticket {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut text = data_encoding::BASE32_NOPAD.encode(&self.to_bytes()[..]);
        text.make_ascii_lowercase();
        write!(f, "{}", text)
    }
}

impl FromStr for Ticket {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = data_encoding::BASE32_NOPAD.decode(s.to_ascii_uppercase().as_bytes())?;
        match Self::from_bytes(&bytes) {
            Ok(st) => Ok(st),
            Err(e) => Err(e.into()),
        }
    }
}
