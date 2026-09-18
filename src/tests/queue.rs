use crate::shar::core::queue::SharQueue;
use crate::shar::prelude::{CRDT, CrdtRelation, IdSize, PeerIdSize};
use crate::types::{IdeAdd, IdeAddConfirmed, NetworkAdd, Remove};
use std::cell::RefCell;
use std::path::PathBuf;

// SharQueue's callbacks are boxed closures with no captured state in these tests, so
// there's nowhere to stash observed calls except somewhere outside the closure itself.
// thread_local storage lets each #[tokio::test] (which cargo test still runs on its own
// thread, same as any #[test]) observe exactly which callbacks fired, without any
// locking and without cross-test contamination even if the harness reuses a thread.
thread_local! {
    static ADD_CALLS: RefCell<Vec<(usize, usize)>> = RefCell::new(Vec::new());
    static REMOVE_CALLS: RefCell<Vec<(usize, usize)>> = RefCell::new(Vec::new());
    static IDE_ADD_CONFIRM_CALLS: RefCell<Vec<(u32, IdSize, PeerIdSize)>> = RefCell::new(Vec::new());
    static IDE_ADD_BROADCAST_CALLS: RefCell<Vec<NetworkAdd>> = RefCell::new(Vec::new());
    static IDE_REMOVE_CALLS: RefCell<Vec<Remove>> = RefCell::new(Vec::new());
}

fn reset_calls() {
    ADD_CALLS.with(|c| c.borrow_mut().clear());
    REMOVE_CALLS.with(|c| c.borrow_mut().clear());
    IDE_ADD_CONFIRM_CALLS.with(|c| c.borrow_mut().clear());
    IDE_ADD_BROADCAST_CALLS.with(|c| c.borrow_mut().clear());
    IDE_REMOVE_CALLS.with(|c| c.borrow_mut().clear());
}

async fn record_add(row: usize, col: usize) {
    ADD_CALLS.with(|c| c.borrow_mut().push((row, col)));
}

async fn record_remove(row: usize, col: usize) {
    REMOVE_CALLS.with(|c| c.borrow_mut().push((row, col)));
}

async fn record_ide_add_confirm(confirmed: IdeAddConfirmed) {
    IDE_ADD_CONFIRM_CALLS.with(|c| c.borrow_mut().push((confirmed.tag, confirmed.id, confirmed.peer)));
}

async fn record_ide_add(op: NetworkAdd) {
    IDE_ADD_BROADCAST_CALLS.with(|c| c.borrow_mut().push(op));
}

async fn record_ide_remove(op: Remove) {
    IDE_REMOVE_CALLS.with(|c| c.borrow_mut().push(op));
}

fn add_calls() -> Vec<(usize, usize)> {
    ADD_CALLS.with(|c| c.borrow().clone())
}

fn remove_calls() -> Vec<(usize, usize)> {
    REMOVE_CALLS.with(|c| c.borrow().clone())
}

fn ide_add_confirm_calls() -> Vec<(u32, IdSize, PeerIdSize)> {
    IDE_ADD_CONFIRM_CALLS.with(|c| c.borrow().clone())
}

fn ide_add_broadcast_calls() -> Vec<NetworkAdd> {
    IDE_ADD_BROADCAST_CALLS.with(|c| c.borrow().clone())
}

fn ide_remove_calls() -> Vec<Remove> {
    IDE_REMOVE_CALLS.with(|c| c.borrow().clone())
}

/// Creates a fresh scratch directory containing a single file with the given content,
/// and a SharQueue loaded against it. Returns the queue and the file's path.
fn setup(dir_name: &str, content: &str) -> (SharQueue, PathBuf) {
    reset_calls();
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_name);
    std::fs::create_dir_all(&dir_path).expect("failed to create scratch dir");
    let file_path = dir_path.join("f.txt");
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let queue = SharQueue::new(
        dir_path,
        0,
        Box::new(|row, col| Box::pin(record_add(row, col))),
        Box::new(|row, col| Box::pin(record_remove(row, col))),
        Box::new(|confirmed| Box::pin(record_ide_add_confirm(confirmed))),
        Box::new(|op| Box::pin(record_ide_add(op))),
        Box::new(|op| Box::pin(record_ide_remove(op))),
    )
    .expect("failed to load queue");
    (queue, file_path)
}

fn teardown(dir_name: &str) {
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_name);
    std::fs::remove_file(dir_path.join("f.txt")).expect("failed to delete scratch file");
    std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
}

#[tokio::test]
async fn add_ide_operation_applies_and_fires_callback() {
    let (mut queue, file_path) = setup("scratch_queue_add_ide", "ab");

    // content "ab" loads as: sentinel (0,0), 'a' -> id 1, 'b' -> id 2, all peer 0 --
    // parent 'c' directly on 'b's already-known identity
    queue
        .add_ide_operation(IdeAdd::new(file_path.clone(), Some(2), Some(0), None, 'c', 42, 0))
        .await
        .expect("failed to add via the ide path");

    let confirms = ide_add_confirm_calls();
    assert_eq!(confirms.len(), 1, "the add should have been confirmed immediately");
    let (confirmed_tag, c_id, c_peer) = confirms[0];
    assert_eq!(confirmed_tag, 42, "the confirmation should echo back the tag it was sent");

    let broadcasts = ide_add_broadcast_calls();
    assert_eq!(broadcasts.len(), 1, "ide_add_callback should have fired once, once actually inserted");
    let op = broadcasts[0].clone();
    assert_eq!(op.file_path, file_path);
    assert_eq!(op.crdt.relation.value, 'c');
    assert_eq!(
        (op.crdt.id, op.crdt.peer),
        (c_id, c_peer),
        "the broadcast's identity should match what was confirmed"
    );

    // 'd' parents directly on 'c' by its now-known, confirmed identity
    queue
        .add_ide_operation(IdeAdd::new(file_path.clone(), Some(c_id), Some(c_peer), None, 'd', 43, 0))
        .await
        .expect("failed to add follow-up character");

    let broadcasts = ide_add_broadcast_calls();
    assert_eq!(broadcasts.len(), 2, "ide_add_callback should have fired twice");
    let follow_up = broadcasts[1].clone();
    assert_eq!(
        (follow_up.crdt.relation.parent_id, follow_up.crdt.relation.parent_peer),
        (c_id, c_peer),
        "'d' should have parented on 'c'"
    );

    teardown("scratch_queue_add_ide");
}

#[tokio::test]
async fn add_ide_operation_confirms_before_its_parent_tag_resolves() {
    // regression test for the whole point of this redesign: confirming a
    // character must not wait on its parent chain resolving at all -- here
    // the parent is referenced by tag (not yet confirmed itself), and the
    // child must still get its own, immediate confirmation.
    let (mut queue, file_path) = setup("scratch_queue_add_ide_tag", "a");

    // 'b' parents on tag 100, which nothing has sent yet -- backlogged, but
    // still gets its own identity + confirmation right away
    queue
        .add_ide_operation(IdeAdd::new(file_path.clone(), None, None, Some(100), 'b', 7, 0))
        .await
        .expect("failed to add 'b'");

    assert_eq!(
        ide_add_confirm_calls().len(),
        1,
        "'b' should be confirmed immediately even though its parent tag hasn't resolved"
    );
    assert_eq!(
        ide_add_broadcast_calls().len(),
        0,
        "'b' shouldn't be inserted into the tree yet -- its parent doesn't exist"
    );

    // now the parent itself arrives, using tag 100 as promised, parented on
    // the sentinel (0, 0) -- this should resolve 'b' too, transitively
    queue
        .add_ide_operation(IdeAdd::new(file_path.clone(), Some(0), Some(0), None, 'x', 100, 0))
        .await
        .expect("failed to add the promised parent");

    assert_eq!(
        ide_add_confirm_calls().len(),
        2,
        "the parent should also be confirmed immediately"
    );
    assert_eq!(
        ide_add_broadcast_calls().len(),
        2,
        "both the parent and 'b' should now be inserted and broadcast"
    );

    teardown("scratch_queue_add_ide_tag");
}

#[tokio::test]
async fn remove_ide_operation_applies_and_fires_callback() {
    // guarding the root sentinel from removal now lives at the IDE layer
    // (extension.js never emits a remove targeting it), not here -- this only
    // covers an ordinary removal.
    let (mut queue, file_path) = setup("scratch_queue_remove_sentinel", "a");

    // (0, 1) is 'a' -- index 0 is the line's own anchor (the sentinel for line 0),
    // not 'a' itself
    queue
        .remove_ide_operation(Remove::new(file_path.clone(), 1, 0, 0))
        .await;
    let calls = ide_remove_calls();
    assert_eq!(calls.len(), 1, "ide_remove_callback should have fired");
    assert_eq!(calls[0].id, 1);

    teardown("scratch_queue_remove_sentinel");
}

#[tokio::test]
async fn add_network_operation_applies_and_fires_callback() {
    let (mut queue, file_path) = setup("scratch_queue_add_network", "a");

    // id 1 is 'a', the only real character -- parent it on the sentinel (0, 0)
    let relation = CrdtRelation::new('z', 0, 0);
    let op = NetworkAdd::new(file_path, CRDT::new(2, 1, relation), 0);
    queue.add_network_operation(op).await;

    assert_eq!(
        add_calls().len(),
        1,
        "a network add whose parent already exists should apply immediately"
    );

    teardown("scratch_queue_add_network");
}

#[tokio::test]
async fn out_of_order_add_resolves_once_parent_arrives() {
    // regression test: add_crdt used to panic ("no entry found for key") instead of
    // returning Err when the parent hadn't arrived yet, which meant add_network_operation
    // never reached its backlog branch at all for exactly the case it exists to handle.
    let (mut queue, file_path) = setup("scratch_queue_out_of_order", "a");

    // the child arrives first, parented on id 999 which doesn't exist yet
    let child_relation = CrdtRelation::new('x', 999, 0);
    let child = NetworkAdd::new(file_path.clone(), CRDT::new(1000, 1, child_relation), 0);
    queue.add_network_operation(child).await;
    assert_eq!(
        add_calls().len(),
        0,
        "should be backlogged silently, not applied and not panicking"
    );

    // now the parent arrives, itself parented on the sentinel
    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = NetworkAdd::new(file_path, CRDT::new(999, 0, parent_relation), 0);
    queue.add_network_operation(parent).await;

    assert_eq!(
        add_calls().len(),
        2,
        "both the parent and the previously-backlogged child should have applied"
    );

    teardown("scratch_queue_out_of_order");
}

#[tokio::test]
async fn three_backlogged_children_of_same_parent_all_resolve() {
    // regression test: the backlog-replay loop used a fixed 0..len() range with
    // remove(i) inside, which either skipped the element that shifted into i or ran
    // past the shrunk Vec's bounds -- and separately, a compensating `i -= 1` on a
    // usize underflowed whenever the match happened to be at index 0.
    let (mut queue, file_path) = setup("scratch_queue_three_children", "a");

    for (id, c) in [(1000, 'x'), (1001, 'y'), (1002, 'z')] {
        let relation = CrdtRelation::new(c, 999, 0);
        let op = NetworkAdd::new(file_path.clone(), CRDT::new(id, 1, relation), 0);
        queue.add_network_operation(op).await;
    }
    assert_eq!(add_calls().len(), 0, "all three should still be backlogged");

    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = NetworkAdd::new(file_path, CRDT::new(999, 0, parent_relation), 0);
    queue.add_network_operation(parent).await;

    assert_eq!(
        add_calls().len(),
        4,
        "the parent plus all three children should have drained, none skipped"
    );

    teardown("scratch_queue_three_children");
}

#[tokio::test]
async fn chained_dependency_resolves_transitively() {
    let (mut queue, file_path) = setup("scratch_queue_chained", "a");

    // A depends on B (id 998), B depends on the not-yet-arrived id 999
    let relation_b = CrdtRelation::new('b', 999, 0);
    let op_b = NetworkAdd::new(file_path.clone(), CRDT::new(998, 1, relation_b), 0);
    let relation_a = CrdtRelation::new('a', 998, 1);
    let op_a = NetworkAdd::new(file_path.clone(), CRDT::new(1000, 2, relation_a), 0);

    // deliver the dependent before its own dependency, in both cases
    queue.add_network_operation(op_a).await;
    queue.add_network_operation(op_b).await;
    assert_eq!(add_calls().len(), 0, "both should still be backlogged");

    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = NetworkAdd::new(file_path, CRDT::new(999, 0, parent_relation), 0);
    queue.add_network_operation(parent).await;

    assert_eq!(
        add_calls().len(),
        3,
        "parent, then B, then A should all have resolved transitively"
    );

    teardown("scratch_queue_chained");
}

#[tokio::test]
async fn remove_network_operation_applies_and_fires_callback() {
    let (mut queue, file_path) = setup("scratch_queue_remove_network", "ab");

    // id 1 is 'a', at (0, 1) now that index 0 is the line's anchor
    let op = Remove::new(file_path, 1, 0, 0);
    queue.remove_network_operation(op).await;

    assert_eq!(
        remove_calls(),
        vec![(0, 1)],
        "removing 'a' should fire the callback with its position"
    );

    teardown("scratch_queue_remove_network");
}

#[tokio::test]
async fn remove_arriving_before_its_target_backlogs_then_resolves() {
    // regression test: the remove_backlog replay loop declared `j` non-mut and never
    // incremented it, so any incoming add would spin forever the moment
    // remove_backlog was non-empty -- confirmed by actually hanging `cargo test`.
    let (mut queue, file_path) = setup("scratch_queue_remove_before_add", "a");

    // a remove arrives for id 999, which doesn't exist in the tree yet
    let pending_remove = Remove::new(file_path.clone(), 999, 0, 0);
    queue.remove_network_operation(pending_remove).await;
    assert_eq!(
        remove_calls().len(),
        0,
        "should be backlogged silently, not applied and not panicking or hanging"
    );

    // now id 999 itself arrives, parented on the sentinel
    let relation = CrdtRelation::new('z', 0, 0);
    let op = NetworkAdd::new(file_path, CRDT::new(999, 0, relation), 0);
    queue.add_network_operation(op).await;

    assert_eq!(add_calls().len(), 1, "the add itself should have applied");
    assert_eq!(
        remove_calls().len(),
        1,
        "the pending remove for the same id should have drained and applied right after"
    );

    teardown("scratch_queue_remove_before_add");
}

#[tokio::test]
async fn remove_backlog_uses_its_own_index_not_the_add_backlogs_leftover() {
    // regression test: the remove_backlog branch called `self.remove_backlog.remove(i)`
    // using `i` -- the *add*-backlog loop's index variable, still in scope -- instead
    // of its own `j`. This only misbehaves when the add-backlog loop has already run
    // and left `i` at some nonzero value by the time the remove-backlog loop needs to
    // remove at a *different* (smaller) index.
    let (mut queue, file_path) = setup("scratch_queue_wrong_index", "a");

    // one unrelated, still-unresolved add sitting in add_backlog, so its loop's `i`
    // ends up at 1 (not 0) after failing to match this call's incoming id
    let unrelated_relation = CrdtRelation::new('u', 12345, 0);
    let unrelated = NetworkAdd::new(file_path.clone(), CRDT::new(9000, 1, unrelated_relation), 0);
    queue.add_network_operation(unrelated).await;

    // one pending remove for id 999, sitting at index 0 (the only valid index) of
    // remove_backlog
    let pending_remove = Remove::new(file_path.clone(), 999, 0, 0);
    queue.remove_network_operation(pending_remove).await;

    // id 999 now arrives -- the add-backlog loop runs first (its one unrelated entry
    // doesn't match, so i ends at 1), then the remove-backlog loop must remove its
    // own match at j == 0, not at the leftover i == 1
    let relation = CrdtRelation::new('z', 0, 0);
    let op = NetworkAdd::new(file_path, CRDT::new(999, 0, relation), 0);
    queue.add_network_operation(op).await;

    assert_eq!(add_calls().len(), 1, "the id-999 add itself should have applied");
    assert_eq!(
        remove_calls().len(),
        1,
        "the pending remove should have drained using its own index"
    );

    teardown("scratch_queue_wrong_index");
}
