//! `SharQueue` mediates between the IDE and the CRDT tree, and backlogs
//! out-of-order remote ops until their dependency arrives.

use crate::shar::core::tree::SharDirectory;
use crate::shar::prelude::*;
use crate::types::{CrdtRelation, IdeAdd, IdeAddConfirmed, NetworkAdd, NetworkOp, Remove};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::pin::Pin;

// Pin says (keep this locked in memory)
// The reason we need this is that async functions become self-refernetial structs, and if they're
// moved in memory their references to themselves become stale, which creates undefined behaviour
//
//
// Send tells the compiler that this is safe to move to another thread to free up this one. It
// doesn't necessarily move it, it just says it can be moved
type BoxedFun = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Owns the tree for one shar session and mediates every read/write to it.
#[derive(Default)]
pub struct SharQueue {
    /// IDE adds waiting on a parent identity that hasn't landed in the tree yet.
    ide_backlog: HashMap<(IdSize, PeerIdSize), Vec<IdeAdd>>,
    /// IDE adds waiting on a parent *tag* that hasn't been registered yet --
    /// scheduling-order jitter only (the parent was sent first, but its own
    /// `add_ide_operation` call hasn't run yet), not a real dependency wait.
    ide_tag_backlog: HashMap<u32, Vec<IdeAdd>>,
    /// network operations waiting on a target that hasn't arrived yet.
    network_backlog: HashMap<(IdSize, PeerIdSize), Vec<NetworkOp>>,

    ide_worklist: VecDeque<IdeAdd>,
    network_worklist: VecDeque<NetworkOp>,
    /// Every IDE-originated tag's real, already-assigned identity. Populated
    /// the instant an `IdeAdd` is received, independent of whether it's been
    /// inserted into the tree yet -- this is what makes confirmation
    /// immediate and makes "delete something before its own add is applied"
    /// safe: the identity always exists once assigned, even if the tree
    /// insertion is still backlogged.
    tag_identities: HashMap<u32, (IdSize, PeerIdSize)>,
    /// This replica's peer id.
    peer: PeerIdSize,
    tree: SharDirectory,
    /// Shared across every file in the directory.
    counter: u32,

    /// Called with the full op once an add has actually landed -- not
    /// gated on arrival order, since a backlogged add fires this from
    /// `clear_network_backlog` instead, potentially much later than when it
    /// was first received. Carries the whole `NetworkAdd` (not just its
    /// identity/position) since this is what eventually notifies a local
    /// IDE, which has never seen this character before and needs its value
    /// and parent to insert it into its own mirror.
    pub network_add_callback: Option<Box<dyn Fn(NetworkAdd) -> BoxedFun + Send + Sync>>,
    /// Called with the full op once a remove has actually landed. See
    /// `network_add_callback` for why the whole op travels through, not
    /// just an identity.
    pub network_remove_callback: Option<Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>>,
    /// Called the instant an IDE add's identity is assigned -- not gated on
    /// tree insertion, so it's always immediate.
    pub ide_add_confirm_callback: Option<Box<dyn Fn(IdeAddConfirmed) -> BoxedFun + Send + Sync>>,
    /// Called once an IDE add is actually inserted into the tree, to
    /// broadcast it to other peers.
    pub ide_add_callback: Option<Box<dyn Fn(NetworkAdd) -> BoxedFun + Send + Sync>>,
    pub ide_remove_callback: Option<Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>>,
}

// Hand-written instead of derived -- the boxed `dyn Fn` callback fields
// can't implement `Debug`, so a derive fails on them.
impl std::fmt::Debug for SharQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharQueue")
            .field("ide_backlog", &self.ide_backlog)
            .field("ide_tag_backlog", &self.ide_tag_backlog)
            .field("network_backlog", &self.network_backlog)
            .field("ide_worklist", &self.ide_worklist)
            .field("network_worklist", &self.network_worklist)
            .field("tag_identities", &self.tag_identities)
            .field("peer", &self.peer)
            .field("tree", &self.tree)
            .field("counter", &self.counter)
            .field("network_add_callback", &self.network_add_callback.is_some())
            .field(
                "network_remove_callback",
                &self.network_remove_callback.is_some(),
            )
            .field(
                "ide_add_confirm_callback",
                &self.ide_add_confirm_callback.is_some(),
            )
            .field("ide_add_callback", &self.ide_add_callback.is_some())
            .field("ide_remove_callback", &self.ide_remove_callback.is_some())
            .finish()
    }
}

impl SharQueue {
    /// Loads the shar rooted at `dir_path`, storing `network_add_callback`/
    /// `network_remove_callback` so they're set. A `SharQueue` built any other way
    /// (e.g. `SharQueue::default()`) leaves them `None`, and calling
    /// [`Self::add_network_operation`]/[`Self::remove_network_operation`] on
    /// one will panic.
    pub fn new(
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        network_add_callback: Box<dyn Fn(NetworkAdd) -> BoxedFun + Send + Sync>,
        network_remove_callback: Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>,
        ide_add_confirm_callback: Box<dyn Fn(IdeAddConfirmed) -> BoxedFun + Send + Sync>,
        ide_add_callback: Box<dyn Fn(NetworkAdd) -> BoxedFun + Send + Sync>,
        ide_remove_callback: Box<dyn Fn(Remove) -> BoxedFun + Send + Sync>,
    ) -> Result<Self> {
        let mut counter: u32 = 0;
        let tree = SharDirectory::new(dir_path, &mut counter)?;
        /* create a new shar queue*/
        let queue = SharQueue {
            ide_backlog: HashMap::new(),
            ide_tag_backlog: HashMap::new(),
            network_backlog: HashMap::new(),
            ide_worklist: VecDeque::new(),
            network_worklist: VecDeque::new(),
            tag_identities: HashMap::new(),
            peer: this_peer_id,
            counter: counter,
            tree: tree,
            network_add_callback: Some(network_add_callback),
            network_remove_callback: Some(network_remove_callback),
            ide_add_confirm_callback: Some(ide_add_confirm_callback),
            ide_add_callback: Some(ide_add_callback),
            ide_remove_callback: Some(ide_remove_callback),
        };

        Ok(queue)
    }

    /// Every file's `(id, peer)` projection, keyed by its full path. See
    /// [`SharDirectory::identities`].
    pub fn identities(&self) -> Vec<(PathBuf, Vec<Vec<(IdSize, PeerIdSize)>>)> {
        self.tree.identities()
    }

    /// Used for a newly-joined peer's catch-up state request (see
    /// state_request.rs). Paths come back relative to this shar's own root,
    /// same as every other path that crosses to a peer -- `tree.full_state()`
    /// itself only knows this replica's own absolute paths, which are
    /// meaningless once they cross to a peer whose shar root lives
    /// somewhere else entirely (confirmed: without this, a newly-joined
    /// peer's entire catch-up backlogs forever on "file not found", since
    /// `to_local_path` can't correct for an already-absolute path with no
    /// relation to this replica's own root).
    pub fn full_state(&self) -> Vec<(PathBuf, HashMap<(IdSize, PeerIdSize), CrdtRelation>)> {
        self.tree
            .full_state()
            .into_iter()
            .map(|(path, chars)| (self.to_relative_path(&path), chars))
            .collect()
    }

    /// Strips this shar's own root off an absolute path, for putting a file
    /// path on the wire -- an absolute local path is meaningless once it
    /// crosses to a peer whose shar root lives somewhere else entirely.
    /// Falls back to the path as-is if it somehow isn't under this shar's
    /// root at all (shouldn't happen under correct IDE behavior).
    fn to_relative_path(&self, path: &PathBuf) -> PathBuf {
        path.strip_prefix(self.tree.root())
            .unwrap_or(path)
            .to_path_buf()
    }

    /// Resolves a path that arrived from the network (relative to the
    /// *sending* peer's shar root) against this replica's own root. Safe to
    /// call on an already-absolute path too -- `PathBuf::join` with an
    /// absolute argument replaces the base instead of nesting incorrectly.
    fn to_local_path(&self, path: &PathBuf) -> PathBuf {
        self.tree.root().join(path)
    }

    /// Applies a locally-typed character. Assigns its real `(id, peer)` and
    /// confirms immediately -- before its parent has necessarily resolved at
    /// all -- so a delete of this character can never end up waiting on a
    /// confirmation that depends on the whole parent chain landing first.
    /// The actual tree insertion (and any further backlogging that needs)
    /// happens in [`Self::add_ide_operation_inner`].
    pub async fn add_ide_operation(&mut self, op: IdeAdd) -> Result<()> {
        self.counter += 1;
        let id = self.counter;
        let peer = self.peer;

        self.tag_identities.insert(op.tag, (id, peer));

        (self.ide_add_confirm_callback.as_ref().unwrap())(IdeAddConfirmed::new(op.tag, id, peer))
            .await;

        // this tag becoming known might be exactly what something else in
        // ide_tag_backlog was waiting on
        if let Some(waiting) = self.ide_tag_backlog.remove(&op.tag) {
            self.ide_worklist.extend(waiting);
        }

        self.add_ide_operation_inner(op, false).await?;
        self.drain_ide_worklist().await;

        Ok(())
    }

    /// Resolves `op`'s parent -- by identity, or by tag if the parent is this
    /// same connection's own not-yet-confirmed add -- and attempts the tree
    /// insertion. Backlogs `op`, by parent tag or by parent identity, if
    /// either isn't ready yet.
    ///
    /// `clearing_worklist` is `true` only when called from within
    /// [`Self::drain_ide_worklist`]'s own loop, so a resolved dependant gets
    /// queued onto `ide_worklist` instead of recursing back into
    /// [`Self::clear_ide_backlog`] -- mirrors
    /// [`Self::add_network_operation_inner`]'s flag exactly, and for the
    /// same reason: without it, a long backlog chain recurses one stack
    /// frame per resolved link instead of looping.
    async fn add_ide_operation_inner(&mut self, op: IdeAdd, clearing_worklist: bool) -> Result<()> {
        tracing::debug!(?op, "add_ide_operation_inner called");

        let parent = if let (Some(id), Some(peer)) = (op.parent_id, op.parent_peer) {
            Some((id, peer))
        } else if let Some(parent_tag) = op.parent_tag {
            self.tag_identities.get(&parent_tag).copied()
        } else {
            None
        };

        let Some((parent_id, parent_peer)) = parent else {
            let parent_tag = op
                .parent_tag
                .expect("an IdeAdd must carry parent_id/parent_peer or parent_tag");
            tracing::debug!(
                parent_tag,
                "add_ide_operation_inner backlogged, parent tag not seen yet"
            );

            self.ide_tag_backlog
                .entry(parent_tag)
                .or_insert_with(Vec::new)
                .push(op);
            return Ok(());
        };

        let (id, peer) = *self.tag_identities.get(&op.tag).expect(
            "add_ide_operation always assigns and registers an identity before this ever runs",
        );

        let relation = CrdtRelation::new(op.val, parent_id, parent_peer);
        let crdt = CRDT::new(id, peer, relation);
        // op.line_hint is only a ring-search starting point, unrelated to
        // parent resolution -- can be stale without costing correctness.

        match self.tree.add_crdt(&op.file_path, op.line_hint, crdt) {
            Ok(Some(pos)) => {
                if clearing_worklist {
                    if let Some(backlogged) = self.ide_backlog.remove(&(id, peer)) {
                        self.ide_worklist.extend(backlogged);
                    }
                } else {
                    self.clear_ide_backlog(id, peer).await;
                }
                let network_op =
                    NetworkAdd::new(self.to_relative_path(&op.file_path), crdt.clone(), pos.0);
                (self.ide_add_callback.as_ref().unwrap())(network_op).await;
                Ok(())
            }
            Ok(None) => {
                tracing::debug!("add_ide_operation_inner was a no-op, likely already added");
                Ok(())
            }
            Err(e) => {
                tracing::debug!(error = %e, "add_ide_operation_inner backlogged, parent not in the tree yet");
                self.ide_backlog
                    .entry((parent_id, parent_peer))
                    .or_insert_with(Vec::new)
                    .push(op);
                Ok(())
            }
        }
    }

    /// Applies a locally-triggered removal and returns the `NetworkRemove` to
    /// send to peers. `is_whole_line` removes `row`'s line-start anchor
    /// instead of `(row, col)`. Rejects the root sentinel as a target.
    pub async fn remove_ide_operation(&mut self, op: Remove) {
        let operation = self.remove_operation(op.clone());

        if let Some(_) = operation {
            // op's file_path is this replica's own absolute path (correct
            // for remove_operation above, which acts on this replica's own
            // tree) -- but meaningless to a peer whose shar root lives
            // somewhere else, so the outgoing copy travels relative instead.
            let mut outgoing = op;
            outgoing.file_path = self.to_relative_path(&outgoing.file_path);
            (self.ide_remove_callback.as_ref().unwrap())(outgoing).await;
        }
    }

    /// Applies a remote `NetworkAdd`. If the parent doesn't exist yet, backlogs
    /// `op` instead. On success, replays any backlogged add/remove waiting on it.
    ///
    /// Panics if `network_add_callback` is `None` — only possible if this `SharQueue`
    /// wasn't built via [`Self::new`].
    pub async fn add_network_operation(&mut self, op: NetworkAdd) {
        self.add_network_operation_inner(op, false).await
    }

    /// `clearing_worklist` is `true` only when called from within
    /// [`Self::clear_network_backlog`]'s own drain loop, so a resolved
    /// dependant gets queued onto `network_worklist` instead of recursing
    /// back into `clear_network_backlog` itself.
    async fn add_network_operation_inner(&mut self, op: NetworkAdd, clearing_worklist: bool) {
        tracing::debug!(?op, "add_network_operation called");
        // op.file_path arrived relative to the *sending* peer's shar root,
        // meaningless as a path on this filesystem -- resolve it against
        // this replica's own root before using it for anything. Safe even
        // if it's somehow already absolute (PathBuf::join with an absolute
        // argument just replaces the base instead of nesting incorrectly).
        let mut op = op;
        op.file_path = self.to_local_path(&op.file_path);
        let crdt = op.crdt;
        let row = op.row;
        let file_path = op.file_path.clone();
        self.counter = std::cmp::max(self.counter, crdt.id);

        // add crdt
        let pos = self.tree.add_crdt(&file_path, row, crdt);

        match pos {
            Ok(pos) => {
                if let Some(position) = pos {
                    tracing::debug!(
                        row = position.0,
                        col = position.1,
                        "add_network_operation resolved position, clearing dependants"
                    );

                    // give the IDE the predecessor as the character's parent so it doesn't have to
                    // perform CRDT tie-breaks
                    if let Ok(Some((id, peer))) =
                        self.tree.predecessor_identity(&file_path, position)
                    {
                        op.crdt.relation.parent_id = id;
                        op.crdt.relation.parent_peer = peer;
                    }

                    (self.network_add_callback.as_ref().unwrap())(op).await;

                    if clearing_worklist {
                        if let Some(backlogged) = self.network_backlog.remove(&(crdt.id, crdt.peer))
                        {
                            self.network_worklist.extend(backlogged);
                        }
                    } else {
                        self.clear_network_backlog(crdt.id, crdt.peer).await;
                    }
                } else {
                    tracing::debug!("add_network_operation was a no-op, likely already added");
                    return;
                }
            }

            Err(e) => {
                tracing::debug!(error = %e, "add_network_operation backlogged, parent not found yet");
                if let Some(backlogged) = self
                    .network_backlog
                    .get_mut(&(crdt.relation.parent_id, crdt.relation.parent_peer))
                {
                    backlogged.push(NetworkOp::ADD(op));
                } else {
                    let _ = self.network_backlog.insert(
                        (crdt.relation.parent_id, crdt.relation.parent_peer),
                        vec![NetworkOp::ADD(op)],
                    );
                }
            }
        };
    }

    /// Applies a remote `NetworkRemove`. If the target doesn't exist yet,
    /// backlogs `op` instead; drained once the matching add arrives (see
    /// [`Self::add_network_operation`]). A retry of an already-removed target
    /// is a no-op.
    ///
    /// Panics if `network_remove_callback` is `None` — only possible if this
    /// `SharQueue` wasn't built via [`Self::new`].
    pub async fn remove_network_operation(&mut self, op: Remove) {
        // same reasoning as add_network_operation_inner -- op.file_path is
        // relative to the sender's root, resolve it against this replica's
        // own root before it touches the tree or goes back out to the IDE.
        let mut op = op;
        op.file_path = self.to_local_path(&op.file_path);
        let op_clone = op.clone();
        let operation = self.remove_operation(op);

        if operation.is_some() {
            (self.network_remove_callback.as_ref().unwrap())(op_clone).await;
        }
    }

    fn remove_operation(&mut self, op: Remove) -> Option<(usize, usize)> {
        tracing::debug!(?op, "remove_network_operation called");
        let op_clone = op.clone();
        let file_path = op_clone.file_path;
        let id = op_clone.id;
        let peer = op_clone.peer;
        let row = op_clone.row;

        if (id, peer) == (0, 0) {
            return None;
        }

        let removed = self.tree.remove_crdt(&file_path, row, id, peer);

        match removed {
            Ok(pos) => {
                if let Some(val) = pos {
                    tracing::debug!(
                        row = val.0,
                        col = val.1,
                        "remove_network_operation resolved target"
                    );
                    return Some((val.0, val.1));
                } else {
                    // if the remove returns none, the value has already been removed so do nothing
                    tracing::debug!("remove_network_operation target already removed, no-op");
                    return None;
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "remove_network_operation backlogged, target not found yet");
                if let Some(backlogged) = self.network_backlog.get_mut(&(id, peer)) {
                    backlogged.push(NetworkOp::REMOVE(op));
                    return None;
                } else {
                    let _ = self
                        .network_backlog
                        .insert((id, peer), vec![NetworkOp::REMOVE(op)]);
                    return None;
                }
            }
        }
    }

    async fn clear_network_backlog(&mut self, id: IdSize, peer: PeerIdSize) {
        // get the list of dependancies
        let dependancies = self.network_backlog.remove(&(id, peer));

        if let Some(list) = dependancies {
            self.network_worklist.extend(list);
            self.drain_network_worklist().await;
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }

    /// Drains `network_worklist` until empty, retrying each item's tree
    /// insertion/removal. Mirrors [`Self::drain_ide_worklist`] exactly,
    /// except each item is a `NetworkOp` (add or remove) rather than always
    /// an `IdeAdd`, so it has to dispatch on the variant.
    async fn drain_network_worklist(&mut self) {
        while !self.network_worklist.is_empty() {
            let item = self.network_worklist.pop_front().unwrap();
            match item {
                NetworkOp::ADD(op) => {
                    Box::pin(self.add_network_operation_inner(op, true)).await;
                }

                NetworkOp::REMOVE(op) => {
                    self.remove_network_operation(op).await;
                }
            }
        }
    }

    async fn clear_ide_backlog(&mut self, id: IdSize, peer: PeerIdSize) {
        // get the list of dependancies
        let dependancies = self.ide_backlog.remove(&(id, peer));

        if let Some(list) = dependancies {
            self.ide_worklist.extend(list);
            self.drain_ide_worklist().await;
        } else {
            // there's no dependancy list therefore nothing to do
            return;
        }
    }

    /// Drains `ide_worklist` until empty, retrying each item's tree insertion.
    /// Shared by [`Self::add_ide_operation`] (for anything a newly-registered
    /// tag unblocked) and [`Self::clear_ide_backlog`] (for anything a newly-
    /// inserted identity unblocked) -- both just push onto the same queue.
    async fn drain_ide_worklist(&mut self) {
        while !self.ide_worklist.is_empty() {
            let item = self.ide_worklist.pop_front().unwrap();
            let _ = Box::pin(self.add_ide_operation_inner(item, true)).await;
        }
    }
}
