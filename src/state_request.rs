//! A second, separate protocol (its own ALPN, registered alongside gossip's
//! on the same `Router`) for one-shot "send me your full state" requests
//! made directly to a peer, bypassing gossip entirely. See the discovery
//! design notes: a newly-joined peer asks 2-3 peers pulled from the
//! discovery DB for this, rather than every peer pushing to it.
//!
//! Reuses `NetworkOp` as the wire payload instead of inventing a new type --
//! a full state dump is just "every character that currently exists, in the
//! order it has to be replayed in" and `NetworkOp`'s `ADD`/`REMOVE` variants
//! are already exactly what `SharQueue::add_network_operation`/
//! `remove_network_operation` consume.
//!
//! A tombstoned character can't just be sent as a single pre-deleted add --
//! `SharDirectory::add_crdt` inserts whatever `relation.deleted` it's given
//! straight into `projection` with no check for it (tombstoning only
//! happens by `remove_crdt` excising an *existing* live entry), so a
//! pre-deleted add would wrongly show up as visible content on the
//! receiving end. Replaying an `ADD` immediately followed by a `REMOVE` for
//! anything currently tombstoned drives the same two-step path a real
//! add-then-delete already goes through, instead of a case `add_crdt` was
//! never built to handle.

use std::collections::HashMap;
use std::path::PathBuf;

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{Endpoint, EndpointAddr};

use crate::queue_wrap::QueueWrap;
use crate::shar::prelude::{CRDT, IdSize, PeerIdSize};
use crate::types::{CrdtRelation, NetworkAdd, NetworkOp, Remove};

/// `QueueWrap::full_state` hands back the raw CRDT data
/// (`(file_path, (id, peer) -> CrdtRelation)`), not something already
/// wire-shaped -- flatten it into an add (and, for anything tombstoned, a
/// following remove) per character. `row` is just a ring-search hint and
/// has no bearing on correctness (see `tree.rs`), so `0` is a safe default
/// for a one-shot bulk transfer like this.
fn to_network_ops(state: Vec<(PathBuf, HashMap<(IdSize, PeerIdSize), CrdtRelation>)>) -> Vec<NetworkOp> {
    let mut ops = Vec::new();
    for (file_path, characters) in state {
        for ((id, peer), relation) in characters {
            let deleted = relation.deleted;
            // the add always carries a live relation -- tombstoning happens
            // via the following remove, not by pre-marking this one deleted
            let mut live_relation = relation;
            live_relation.deleted = false;
            ops.push(NetworkOp::ADD(NetworkAdd::new(
                file_path.clone(),
                CRDT::new(id, peer, live_relation),
                0,
            )));
            if deleted {
                ops.push(NetworkOp::REMOVE(Remove::new(file_path.clone(), id, peer, 0)));
            }
        }
    }
    ops
}

pub const STATE_REQUEST_ALPN: &[u8] = b"shario/state-request/0";

/// Sanity ceiling on the *declared* length prefix, not a guess at how big a
/// real state will be -- just here so a buggy/malicious peer claiming an
/// absurd size (e.g. billions of bytes) gets rejected before we try to
/// allocate for it, rather than actually trusting whatever they send.
const MAX_STATE_BYTES: u64 = 1024 * 1024 * 1024;

/// Accept side: registered on the `Router` next to gossip's own handler.
/// Every incoming connection on this ALPN gets the exact same treatment --
/// dump this replica's full state and send it back. No throttling here on
/// purpose: the *asking* side controls fan-in by only querying 2-3 peers,
/// so the accept side doesn't need to know or care how many others also
/// got asked.
#[derive(Clone)]
pub struct StateRequestHandler {
    queue: QueueWrap,
}

impl StateRequestHandler {
    pub fn new(queue: QueueWrap) -> Self {
        Self { queue }
    }
}

// `ProtocolHandler` requires `Debug`, but `QueueWrap` (and `SharQueue` inside
// it, via its boxed `dyn Fn` callbacks) isn't `Debug`-able -- hand-written
// instead of derived so this doesn't force `Debug` all the way down.
impl std::fmt::Debug for StateRequestHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateRequestHandler").finish_non_exhaustive()
    }
}

impl ProtocolHandler for StateRequestHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, _recv) = connection.accept_bi().await?;
        tracing::info!(peer = ?connection.remote_id(), "state request: accepted connection from peer");

        let state = to_network_ops(self.queue.full_state().await);

        let encoded = serde_json::to_vec(&state).expect("Vec<NetworkOp> encoding is infallible");

        // 8-byte length prefix first, so the other side knows exactly how
        // much to read instead of reading-until-close with a size guess.
        let len_prefix = (encoded.len() as u64).to_be_bytes();
        send.write_all(&len_prefix)
            .await
            .map_err(std::io::Error::other)?;
        send.write_all(&encoded)
            .await
            .map_err(std::io::Error::other)?;
        send.finish()?;
        tracing::info!(
            peer = ?connection.remote_id(),
            ops = state.len(),
            "state request: sent state to peer"
        );

        connection.closed().await;
        Ok(())
    }
}

/// Connect side: dial one peer directly, ask for its state, and replay every
/// op it sends back through the normal incoming add/remove paths, in the
/// order they arrive. Safe to call this for more than one peer and apply
/// all of their responses -- both paths are idempotent, so overlapping ops
/// from multiple peers just no-op on the repeats.
pub async fn request_state(endpoint: &Endpoint, peer: EndpointAddr, queue: &QueueWrap) {
    let connection = match endpoint.connect(peer, STATE_REQUEST_ALPN).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "state request: couldn't connect to peer");
            return;
        }
    };
    tracing::info!(peer = ?connection.remote_id(), "state request: connected to peer");

    let (mut send, mut recv) = match connection.open_bi().await {
        Ok(streams) => streams,
        Err(e) => {
            tracing::warn!(error = %e, "state request: couldn't open stream");
            return;
        }
    };

    // Nothing to send -- connecting on this ALPN at all *is* the request.
    let _ = send.finish();

    let mut len_buf = [0u8; 8];
    if let Err(e) = recv.read_exact(&mut len_buf).await {
        tracing::warn!(error = %e, "state request: couldn't read length prefix");
        return;
    }
    let declared_len = u64::from_be_bytes(len_buf);

    if declared_len > MAX_STATE_BYTES {
        tracing::warn!(
            declared_len,
            max = MAX_STATE_BYTES,
            "state request: peer claimed an unreasonably large state, aborting"
        );
        return;
    }

    let mut bytes = vec![0u8; declared_len as usize];
    if let Err(e) = recv.read_exact(&mut bytes).await {
        tracing::warn!(error = %e, "state request: couldn't read state payload");
        return;
    }

    let state: Vec<NetworkOp> = match serde_json::from_slice(&bytes) {
        Ok(decoded) => decoded,
        Err(e) => {
            tracing::warn!(error = %e, "state request: couldn't decode response");
            return;
        }
    };

    for op in state {
        tracing::info!("message successfully received, applying now");
        match op {
            NetworkOp::ADD(op) => queue.add_network_operation(op).await,
            NetworkOp::REMOVE(op) => queue.remove_network_operation(op).await,
        }
    }
}
