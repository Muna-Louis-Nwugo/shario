use crate::shar::core::queue::SharQueue;
use crate::shar::prelude::{CRDT, CrdtRelation};
use crate::shar::types::{AddOperation, RemoveOperation};
use std::cell::RefCell;
use std::path::PathBuf;

// SharQueue's callbacks are plain `fn` pointers (no closures, so no capturing a local
// Vec directly) -- thread_local storage lets each #[test] (which cargo test runs on
// its own thread) observe exactly which callbacks fired, without any locking and
// without cross-test contamination even if the harness reuses a thread across tests.
thread_local! {
    static ADD_CALLS: RefCell<Vec<(usize, usize)>> = RefCell::new(Vec::new());
    static REMOVE_CALLS: RefCell<Vec<(usize, usize, bool)>> = RefCell::new(Vec::new());
}

fn reset_calls() {
    ADD_CALLS.with(|c| c.borrow_mut().clear());
    REMOVE_CALLS.with(|c| c.borrow_mut().clear());
}

fn record_add(row: usize, col: usize) {
    ADD_CALLS.with(|c| c.borrow_mut().push((row, col)));
}

fn record_remove(row: usize, col: usize, is_line_merge: bool) {
    REMOVE_CALLS.with(|c| c.borrow_mut().push((row, col, is_line_merge)));
}

fn add_calls() -> Vec<(usize, usize)> {
    ADD_CALLS.with(|c| c.borrow().clone())
}

fn remove_calls() -> Vec<(usize, usize, bool)> {
    REMOVE_CALLS.with(|c| c.borrow().clone())
}

/// Creates a fresh scratch directory containing a single file with the given content,
/// and a SharQueue loaded against it. Returns the queue and the file's path.
fn setup(dir_name: &str, content: &str) -> (SharQueue, PathBuf) {
    reset_calls();
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_name);
    std::fs::create_dir_all(&dir_path).expect("failed to create scratch dir");
    let file_path = dir_path.join("f.txt");
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let queue = SharQueue::new(dir_path, 0, record_add, record_remove)
        .expect("failed to load queue");
    (queue, file_path)
}

fn teardown(dir_name: &str) {
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_name);
    std::fs::remove_file(dir_path.join("f.txt")).expect("failed to delete scratch file");
    std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
}

#[test]
fn add_ide_crdt_returns_the_operation_it_applied() {
    let (mut queue, file_path) = setup("scratch_queue_add_ide", "ab");

    // 'a' is at (0,0), 'b' at (0,1) -- insert after 'b'
    let op = queue
        .add_ide_crdt(&file_path, 0, 1, 'c', false)
        .expect("failed to add via the ide path");

    assert_eq!(op.file_path, file_path);
    assert_eq!(op.crdt.relation.value, 'c');
    assert_eq!(op.row, 0);
    assert_eq!(op.start_line, false);

    // applied synchronously: 'c' should now be sitting at (0, 2), so a follow-up
    // insert anchored there should resolve its parent to exactly this op's crdt --
    // that's only possible if 'c' really landed where it was supposed to
    let follow_up = queue
        .add_ide_crdt(&file_path, 0, 2, 'd', false)
        .expect("failed to add follow-up character");
    assert_eq!(
        (
            follow_up.crdt.relation.parent_id,
            follow_up.crdt.relation.parent_peer
        ),
        (op.crdt.id, op.crdt.peer),
        "'d' should have parented on 'c', proving 'c' really landed at (0,2)"
    );

    teardown("scratch_queue_add_ide");
}

#[test]
fn remove_ide_crdt_guards_the_sentinel() {
    let (mut queue, file_path) = setup("scratch_queue_remove_sentinel", "a");

    // (0, 0) is 'a', not the sentinel -- a normal, valid removal
    let op = queue
        .remove_ide_crdt(&file_path, 0, 0, false)
        .expect("failed to remove a real character");
    assert_eq!(op.id, 1);

    // asking to remove "the whole line" for line 0 resolves to the sentinel (0, 0)
    // as its anchor -- this must be rejected, not passed through to remove_crdt
    // (which would underflow trying to merge line 0 into line "-1")
    let result = queue.remove_ide_crdt(&file_path, 0, 0, true);
    assert!(
        result.is_err(),
        "removing the sentinel line-anchor should error, not panic"
    );

    teardown("scratch_queue_remove_sentinel");
}

#[test]
fn add_network_operation_applies_and_fires_callback() {
    let (mut queue, file_path) = setup("scratch_queue_add_network", "a");

    // id 1 is 'a', the only real character -- parent it on the sentinel (0, 0)
    let relation = CrdtRelation::new('z', 0, 0);
    let op = AddOperation::new(file_path, CRDT::new(2, 1, relation), 0, true);
    queue.add_network_operation(op);

    assert_eq!(
        add_calls().len(),
        1,
        "a network add whose parent already exists should apply immediately"
    );

    teardown("scratch_queue_add_network");
}

#[test]
fn out_of_order_add_resolves_once_parent_arrives() {
    // regression test: add_crdt used to panic ("no entry found for key") instead of
    // returning Err when the parent hadn't arrived yet, which meant add_network_operation
    // never reached its backlog branch at all for exactly the case it exists to handle.
    let (mut queue, file_path) = setup("scratch_queue_out_of_order", "a");

    // the child arrives first, parented on id 999 which doesn't exist yet
    let child_relation = CrdtRelation::new('x', 999, 0);
    let child = AddOperation::new(file_path.clone(), CRDT::new(1000, 1, child_relation), 0, false);
    queue.add_network_operation(child);
    assert_eq!(
        add_calls().len(),
        0,
        "should be backlogged silently, not applied and not panicking"
    );

    // now the parent arrives, itself parented on the sentinel
    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = AddOperation::new(file_path, CRDT::new(999, 0, parent_relation), 0, true);
    queue.add_network_operation(parent);

    assert_eq!(
        add_calls().len(),
        2,
        "both the parent and the previously-backlogged child should have applied"
    );

    teardown("scratch_queue_out_of_order");
}

#[test]
fn three_backlogged_children_of_same_parent_all_resolve() {
    // regression test: the backlog-replay loop used a fixed 0..len() range with
    // remove(i) inside, which either skipped the element that shifted into i or ran
    // past the shrunk Vec's bounds -- and separately, a compensating `i -= 1` on a
    // usize underflowed whenever the match happened to be at index 0.
    let (mut queue, file_path) = setup("scratch_queue_three_children", "a");

    for (id, c) in [(1000, 'x'), (1001, 'y'), (1002, 'z')] {
        let relation = CrdtRelation::new(c, 999, 0);
        let op = AddOperation::new(file_path.clone(), CRDT::new(id, 1, relation), 0, false);
        queue.add_network_operation(op);
    }
    assert_eq!(add_calls().len(), 0, "all three should still be backlogged");

    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = AddOperation::new(file_path, CRDT::new(999, 0, parent_relation), 0, true);
    queue.add_network_operation(parent);

    assert_eq!(
        add_calls().len(),
        4,
        "the parent plus all three children should have drained, none skipped"
    );

    teardown("scratch_queue_three_children");
}

#[test]
fn chained_dependency_resolves_transitively() {
    let (mut queue, file_path) = setup("scratch_queue_chained", "a");

    // A depends on B (id 998), B depends on the not-yet-arrived id 999
    let relation_b = CrdtRelation::new('b', 999, 0);
    let op_b = AddOperation::new(file_path.clone(), CRDT::new(998, 1, relation_b), 0, false);
    let relation_a = CrdtRelation::new('a', 998, 1);
    let op_a = AddOperation::new(file_path.clone(), CRDT::new(1000, 2, relation_a), 0, false);

    // deliver the dependent before its own dependency, in both cases
    queue.add_network_operation(op_a);
    queue.add_network_operation(op_b);
    assert_eq!(add_calls().len(), 0, "both should still be backlogged");

    let parent_relation = CrdtRelation::new('p', 0, 0);
    let parent = AddOperation::new(file_path, CRDT::new(999, 0, parent_relation), 0, true);
    queue.add_network_operation(parent);

    assert_eq!(
        add_calls().len(),
        3,
        "parent, then B, then A should all have resolved transitively"
    );

    teardown("scratch_queue_chained");
}

#[test]
fn remove_network_operation_applies_and_fires_callback() {
    let (mut queue, file_path) = setup("scratch_queue_remove_network", "ab");

    let op = RemoveOperation::new(file_path, 1, 0, 0);
    queue.remove_network_operation(op);

    assert_eq!(
        remove_calls(),
        vec![(0, 0, false)],
        "removing 'a' should fire the callback with its position and merge-flag"
    );

    teardown("scratch_queue_remove_network");
}

#[test]
fn remove_arriving_before_its_target_backlogs_then_resolves() {
    // regression test: the remove_backlog replay loop declared `j` non-mut and never
    // incremented it, so any incoming add would spin forever the moment
    // remove_backlog was non-empty -- confirmed by actually hanging `cargo test`.
    let (mut queue, file_path) = setup("scratch_queue_remove_before_add", "a");

    // a remove arrives for id 999, which doesn't exist in the tree yet
    let pending_remove = RemoveOperation::new(file_path.clone(), 999, 0, 0);
    queue.remove_network_operation(pending_remove);
    assert_eq!(
        remove_calls().len(),
        0,
        "should be backlogged silently, not applied and not panicking or hanging"
    );

    // now id 999 itself arrives, parented on the sentinel
    let relation = CrdtRelation::new('z', 0, 0);
    let op = AddOperation::new(file_path, CRDT::new(999, 0, relation), 0, true);
    queue.add_network_operation(op);

    assert_eq!(add_calls().len(), 1, "the add itself should have applied");
    assert_eq!(
        remove_calls().len(),
        1,
        "the pending remove for the same id should have drained and applied right after"
    );

    teardown("scratch_queue_remove_before_add");
}

#[test]
fn remove_backlog_uses_its_own_index_not_the_add_backlogs_leftover() {
    // regression test: the remove_backlog branch called `self.remove_backlog.remove(i)`
    // using `i` -- the *add*-backlog loop's index variable, still in scope -- instead
    // of its own `j`. This only misbehaves when the add-backlog loop has already run
    // and left `i` at some nonzero value by the time the remove-backlog loop needs to
    // remove at a *different* (smaller) index.
    let (mut queue, file_path) = setup("scratch_queue_wrong_index", "a");

    // one unrelated, still-unresolved add sitting in add_backlog, so its loop's `i`
    // ends up at 1 (not 0) after failing to match this call's incoming id
    let unrelated_relation = CrdtRelation::new('u', 12345, 0);
    let unrelated = AddOperation::new(
        file_path.clone(),
        CRDT::new(9000, 1, unrelated_relation),
        0,
        false,
    );
    queue.add_network_operation(unrelated);

    // one pending remove for id 999, sitting at index 0 (the only valid index) of
    // remove_backlog
    let pending_remove = RemoveOperation::new(file_path.clone(), 999, 0, 0);
    queue.remove_network_operation(pending_remove);

    // id 999 now arrives -- the add-backlog loop runs first (its one unrelated entry
    // doesn't match, so i ends at 1), then the remove-backlog loop must remove its
    // own match at j == 0, not at the leftover i == 1
    let relation = CrdtRelation::new('z', 0, 0);
    let op = AddOperation::new(file_path, CRDT::new(999, 0, relation), 0, true);
    queue.add_network_operation(op);

    assert_eq!(add_calls().len(), 1, "the id-999 add itself should have applied");
    assert_eq!(
        remove_calls().len(),
        1,
        "the pending remove should have drained using its own index"
    );

    teardown("scratch_queue_wrong_index");
}
