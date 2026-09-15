use crate::shar::core::tree::{SharDirectory, SharFile};
use crate::shar::prelude::{CRDT, CrdtRelation};
use std::path::PathBuf;

#[test]
fn test_add_crdt() {
    // SharDirectory::add_crdt just routes to the right SharFile and delegates, so
    // testing through the directory covers both the routing and the underlying
    // insertion logic in one go — no need for a separate SharFile-only version
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_add_crdt_dir");
    std::fs::create_dir_all(&dir_path).expect("failed to create scratch dir");
    let file_path = dir_path.join("scratch.txt");

    // deliberately messy, multi-line, mixed-script content — doesn't need to be
    // coherent, just needs to give the ring search several lines to actually search
    // through instead of finding things on the first try
    let content = "The quick brown fox jumps?! 123 @#$%^&*()_+-=\n\u{c9}lan caf\u{e9} \u{2014} na\u{ef}ve r\u{e9}sum\u{e9}, d\u{e9}j\u{e0} vu\n\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30b9}\u{30c8}\u{6587}\u{3067}\u{3059}\n\n\tTabbed\t\tline\twith\ttabs\n...   lots   of    spaces   ...\nemoji test \u{1f389}\u{1f680} done\nFINAL_LINE_END";
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let mut counter = 0;
    let mut dir =
        SharDirectory::new(dir_path.clone(), &mut counter).expect("failed to load directory");

    // add_file assigns ids 1..=n in char order, so the very last character loaded has
    // id == total char count, sitting on the last (8th, index 7) line
    let last_id = content.chars().count() as u32;

    // append a character after the very last character — hint line 0 on purpose, so
    // the ring search has to walk all the way down to line 7 to find it. Every line's
    // index 0 is now its own anchor (root sentinel for line 0, the creating newline
    // for every other line), so real content starts at index 1 — "FINAL_LINE_END" is
    // 14 characters occupying indices 1-14, so '!' lands at index 15
    let extra = CRDT::new(last_id + 1, 0, CrdtRelation::new('!', last_id, 0));
    let position = dir
        .add_crdt(&file_path, 0, extra)
        .expect("failed to append after the last character");
    assert_eq!(
        position,
        Some((7, 15)),
        "'!' should land right after the last character"
    );

    // split a new line right after that character — a line-break CRDT now reports its
    // own landing position (the new line it creates, at that line's index 0 — it
    // becomes the new line's anchor), not the position of the character it split after
    let newline = CRDT::new(last_id + 2, 0, CrdtRelation::new('\n', last_id + 1, 0));
    let position = dir
        .add_crdt(&file_path, 0, newline)
        .expect("failed to add a line at the end");
    assert_eq!(
        position,
        Some((8, 0)),
        "the newline should report its own position: the new line's anchor"
    );

    // add the first real character of the freshly-created line (line 8) — its parent
    // is the newline, which now sits at (8, 0) as line 8's own anchor: a completely
    // ordinary lookup, no special-casing needed. It lands right after the anchor
    let last_char = CRDT::new(last_id + 3, 0, CrdtRelation::new('X', last_id + 2, 0));
    let position = dir
        .add_crdt(&file_path, 8, last_char)
        .expect("failed to add character to the new final line");
    assert_eq!(
        position,
        Some((8, 1)),
        "'X' should land right after line 8's anchor"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
}

#[test]
fn test_convergence() {
    let path_a = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_convergence_a.txt");
    let path_b = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_convergence_b.txt");
    let path_c = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_convergence_c.txt");

    // same messy multi-line/mixed-script content, loaded independently into three
    // separate replicas
    let content = "Lorem ipsum dolor sit amet, consectetur 42! \u{393}\u{3b5}\u{3b9}\u{3ac} \u{3c3}\u{3bf}\u{3c5} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5} \u{2014} \u{43f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}\n\t\tmixed\ttabs   and    spaces\nlast line no newline";
    std::fs::write(&path_a, content).expect("failed to write scratch file a");
    std::fs::write(&path_b, content).expect("failed to write scratch file b");
    std::fs::write(&path_c, content).expect("failed to write scratch file c");

    let mut counter_a = 0;
    let mut counter_b = 0;
    let mut counter_c = 0;
    let mut replica_a =
        SharFile::new(path_a.clone(), &mut counter_a).expect("failed to load replica a");
    let mut replica_b =
        SharFile::new(path_b.clone(), &mut counter_b).expect("failed to load replica b");
    let mut replica_c =
        SharFile::new(path_c.clone(), &mut counter_c).expect("failed to load replica c");

    // the very last real character in the file — three peers concurrently insert
    // after it without seeing each other's ops. Chosen to exercise more than one
    // tie-break branch: x and y share the same id but different peers, z has a
    // smaller id than both
    let anchor_id = content.chars().count() as u32;
    let op_x = CRDT::new(500, 1, CrdtRelation::new('X', anchor_id, 0));
    let op_y = CRDT::new(500, 2, CrdtRelation::new('Y', anchor_id, 0));
    let op_z = CRDT::new(300, 5, CrdtRelation::new('Z', anchor_id, 0));

    // replica_a applies them in one order... each should report a real position (an
    // actual insert), not None (which would mean it got treated as a duplicate no-op —
    // a sign these three ids/peers weren't as distinct as intended)
    assert!(
        replica_a
            .add_crdt(0, op_x.clone())
            .expect("a: failed to apply x")
            .is_some(),
        "a: x should be a real insert, not a no-op"
    );
    assert!(
        replica_a
            .add_crdt(0, op_y.clone())
            .expect("a: failed to apply y")
            .is_some(),
        "a: y should be a real insert, not a no-op"
    );
    assert!(
        replica_a
            .add_crdt(0, op_z.clone())
            .expect("a: failed to apply z")
            .is_some(),
        "a: z should be a real insert, not a no-op"
    );

    // ...replica_b applies them in the reverse order...
    assert!(
        replica_b
            .add_crdt(0, op_z.clone())
            .expect("b: failed to apply z")
            .is_some(),
        "b: z should be a real insert, not a no-op"
    );
    assert!(
        replica_b
            .add_crdt(0, op_y.clone())
            .expect("b: failed to apply y")
            .is_some(),
        "b: y should be a real insert, not a no-op"
    );
    assert!(
        replica_b
            .add_crdt(0, op_x.clone())
            .expect("b: failed to apply x")
            .is_some(),
        "b: x should be a real insert, not a no-op"
    );

    // ...and replica_c applies them in yet another order
    assert!(
        replica_c
            .add_crdt(0, op_y.clone())
            .expect("c: failed to apply y")
            .is_some(),
        "c: y should be a real insert, not a no-op"
    );
    assert!(
        replica_c
            .add_crdt(0, op_x.clone())
            .expect("c: failed to apply x")
            .is_some(),
        "c: x should be a real insert, not a no-op"
    );
    assert!(
        replica_c
            .add_crdt(0, op_z.clone())
            .expect("c: failed to apply z")
            .is_some(),
        "c: z should be a real insert, not a no-op"
    );

    assert_eq!(
        replica_a, replica_b,
        "replica a and b diverged after applying the same concurrent ops in different orders"
    );
    assert_eq!(
        replica_b, replica_c,
        "replica b and c diverged after applying the same concurrent ops in different orders"
    );

    std::fs::remove_file(&path_a).expect("failed to delete scratch file a");
    std::fs::remove_file(&path_b).expect("failed to delete scratch file b");
    std::fs::remove_file(&path_c).expect("failed to delete scratch file c");
}

#[test]
fn test_get_id_peer() {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_get_id_peer.txt");
    // four lines of 9 characters each (except the last), no trailing newline
    let content = "abc123!@#\ndef456$%^\nghi789&*(\nLAST";
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let mut counter = 0;
    let mut file = SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

    // index 0 of every line is now its own anchor (the root sentinel for line 0, the
    // creating newline for every other line), so real content starts at index 1 —
    // spot-check known positions across every line, not just the first
    assert_eq!(
        file.get_id_peer((0, 0)),
        Some((0, 0)),
        "(0, 0) is the root sentinel, not 'a'"
    );
    assert_eq!(
        file.get_id_peer((0, 1)),
        Some((1, 0)),
        "'a' should be at (0, 1)"
    );
    assert_eq!(
        file.get_id_peer((0, 9)),
        Some((9, 0)),
        "'#' should be at (0, 9)"
    );
    assert_eq!(
        file.get_id_peer((1, 1)),
        Some((11, 0)),
        "'d' should be at (1, 1)"
    );
    assert_eq!(
        file.get_id_peer((1, 9)),
        Some((19, 0)),
        "'^' should be at (1, 9)"
    );
    assert_eq!(
        file.get_id_peer((2, 1)),
        Some((21, 0)),
        "'g' should be at (2, 1)"
    );
    assert_eq!(
        file.get_id_peer((2, 9)),
        Some((29, 0)),
        "'(' should be at (2, 9)"
    );
    assert_eq!(
        file.get_id_peer((3, 1)),
        Some((31, 0)),
        "'L' should be at (3, 1)"
    );
    assert_eq!(
        file.get_id_peer((3, 4)),
        Some((34, 0)),
        "'T' should be at (3, 4)"
    );

    // out of bounds in either dimension is None, not a panic — every line now has one
    // extra slot for its own anchor, so the boundary shifts out by one too
    assert_eq!(
        file.get_id_peer((0, 10)),
        None,
        "line 0 only has 9 real characters plus its anchor"
    );
    assert_eq!(
        file.get_id_peer((3, 5)),
        None,
        "line 3 only has 4 real characters plus its anchor"
    );
    assert_eq!(file.get_id_peer((10, 0)), None, "there are only 4 lines");

    // the id/peer this returns has to be usable as a real parent reference: look up
    // the last character of the last line, use it as a parent, and confirm the new
    // character lands right after it
    let (parent_id, parent_peer) = file.get_id_peer((3, 4)).expect("'T' should still be there");
    let c = CRDT::new(35, 0, CrdtRelation::new('!', parent_id, parent_peer));
    let position = file
        .add_crdt(3, c)
        .expect("failed to add character using looked-up parent");
    assert_eq!(position, Some((3, 5)), "'!' should land right after 'T'");

    assert_eq!(
        file.get_id_peer((3, 5)),
        Some((35, 0)),
        "'!' should have landed right after 'T'"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
}

#[test]
fn test_front_of_line_insert_ordering() {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_front_of_line.txt");
    // messy first line ending in a real newline (id == char count), whose child
    // line (line 1) starts out empty (apart from its own anchor: the newline itself)
    let content = "some prefix line !@# \u{1f600} 123\n";
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let mut counter = 0;
    let mut file = SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

    let newline_id = content.chars().count() as u32;
    let new_line = 1;

    // four front-of-line inserts, applied in a deliberately scrambled (non-sorted)
    // order, all parented directly on the real newline that anchors line 1 — no
    // special-casing needed, it's an ordinary parent lookup now. Final order must
    // still be strictly descending by id regardless of application order, which
    // means each one has to correctly walk past however many are already there.
    // Ids are chosen comfortably above the file's own real character count so none
    // of them collide with (and get silently no-op'd against) real content already
    // loaded
    let a = CRDT::new(1050, 0, CrdtRelation::new('a', newline_id, 0));
    let position = file
        .add_crdt(new_line, a)
        .expect("failed to add front-of-line character 'a'");
    assert_eq!(
        position,
        Some((1, 1)),
        "'a' is the only real thing on the line so far, right after the anchor"
    );

    let b = CRDT::new(1010, 0, CrdtRelation::new('b', newline_id, 0));
    let position = file
        .add_crdt(new_line, b)
        .expect("failed to add front-of-line character 'b'");
    assert_eq!(
        position,
        Some((1, 2)),
        "'b' has a smaller id, so it lands after 'a'"
    );

    let c = CRDT::new(1999, 0, CrdtRelation::new('c', newline_id, 0));
    let position = file
        .add_crdt(new_line, c)
        .expect("failed to add front-of-line character 'c'");
    assert_eq!(
        position,
        Some((1, 1)),
        "'c' has the largest id, so it jumps right after the anchor"
    );

    let d = CRDT::new(1500, 0, CrdtRelation::new('d', newline_id, 0));
    let position = file
        .add_crdt(new_line, d)
        .expect("failed to add front-of-line character 'd'");
    assert_eq!(
        position,
        Some((1, 2)),
        "'d' lands right after 'c', before 'a'"
    );

    // final order must be: anchor, then strictly descending by id: 1999, 1500, 1050, 1010
    assert_eq!(
        file.get_id_peer((new_line, 0)),
        Some((newline_id, 0)),
        "index 0 is still the line's own anchor"
    );
    assert_eq!(file.get_id_peer((new_line, 1)), Some((1999, 0)));
    assert_eq!(file.get_id_peer((new_line, 2)), Some((1500, 0)));
    assert_eq!(file.get_id_peer((new_line, 3)), Some((1050, 0)));
    assert_eq!(file.get_id_peer((new_line, 4)), Some((1010, 0)));

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
}

#[test]
fn test_remove_crdt() {
    // same reasoning as test_add_crdt — SharDirectory::remove_crdt just routes to the
    // right SharFile and delegates, so test through the directory to cover routing and
    // the underlying tombstone logic together
    let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_remove_crdt_dir");
    std::fs::create_dir_all(&dir_path).expect("failed to create scratch dir");
    let file_path = dir_path.join("scratch.txt");
    let content =
        "chain: a-b-c-d-e-f end of chain, more filler text here 12345 \u{2603}\u{2764}\u{fe0f}";
    std::fs::write(&file_path, content).expect("failed to write scratch file");

    let mut counter = 0;
    let mut dir =
        SharDirectory::new(dir_path.clone(), &mut counter).expect("failed to load directory");

    // ids 10, 11, 12 are three real, consecutive characters (each one's parent is
    // the one before it, per add_file's sequential chain) — tombstone all three to
    // build an actual multi-level tombstone chain, not just a single removed node.
    // None of them are line anchors, so is_line is false throughout
    dir.remove_crdt(&file_path, 0, 10, 0)
        .expect("failed to remove first character in the chain");
    dir.remove_crdt(&file_path, 0, 11, 0)
        .expect("failed to remove second character in the chain");
    dir.remove_crdt(&file_path, 0, 12, 0)
        .expect("failed to remove third character in the chain");

    // retrying an already-removed one is a no-op, not an error
    assert_eq!(
        dir.remove_crdt(&file_path, 0, 11, 0)
            .expect("failed to no-op a repeated removal"),
        None,
        "removing an already-deleted crdt should report None, not a fresh position"
    );

    // removing something that was never added at all is an error
    assert!(
        dir.remove_crdt(&file_path, 0, 999_999, 0).is_err(),
        "removing a nonexistent crdt should fail, not succeed"
    );

    // add a character parented on the deepest tombstone (id 12) — resolving this has
    // to climb all three tombstoned levels back to the nearest live ancestor (id 9),
    // exercising find_tombstone's recursion through the full directory-routed path.
    // id 9 sits at index 9 now (index 0 is the line's anchor), so the new character
    // lands right after it at index 10
    let new_char = CRDT::new(999, 0, CrdtRelation::new('!', 12, 0));
    let position = dir
        .add_crdt(&file_path, 0, new_char)
        .expect("failed to add a character parented on a 3-deep tombstone chain");
    assert_eq!(
        position,
        Some((0, 10)),
        "should land right after id 9, the nearest live ancestor left after the chain"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
}

#[test]
fn nested_directories_are_routed_correctly() {
    // regression test: find_file used to re-strip its own dir_name's full absolute
    // path from an already-partially-consumed path iterator on every recursive call,
    // which broke as soon as a file lived more than one directory level below the
    // shar root. None of the tests above ever exercise this because their files all
    // sit directly in the root directory passed to SharDirectory::new.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_nested_dir_root");
    let nested = root.join("subdir");
    std::fs::create_dir_all(&nested).expect("failed to create nested scratch dir");
    let file_path = nested.join("f.txt");
    std::fs::write(&file_path, "ab").expect("failed to write scratch file");

    let mut counter = 0;
    let mut dir =
        SharDirectory::new(root.clone(), &mut counter).expect("failed to load directory");

    // (0, 0) is the root sentinel now; 'a' is the first real character, at (0, 1)
    assert_eq!(
        dir.get_id_peer(&file_path, (0, 1)).expect("routing failed"),
        Some((1, 0)),
        "'a' one directory level below the root should still be found"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    std::fs::remove_dir(&nested).expect("failed to delete nested scratch dir");
    std::fs::remove_dir(&root).expect("failed to delete scratch root");
}

#[test]
fn counter_is_shared_but_parents_stay_within_each_file() {
    // regression test: add_file used to compute every character's parent as
    // `id - 1` off the shared global counter, so the first character of the
    // *second* file loaded would try to parent itself on the last character of
    // the *first* file (a different SharFile's characters map entirely) and
    // panic. add_file now tracks its own per-file `prev_id`, starting each file
    // fresh off the sentinel regardless of what the global counter already is.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_multi_file_root");
    std::fs::create_dir_all(&root).expect("failed to create scratch dir");
    let file_a = root.join("a.txt");
    let file_b = root.join("b.txt");
    std::fs::write(&file_a, "a").expect("failed to write scratch file a");
    std::fs::write(&file_b, "b").expect("failed to write scratch file b");

    let mut counter = 0;
    let mut dir =
        SharDirectory::new(root.clone(), &mut counter).expect("failed to load directory");

    // both files should have loaded successfully and independently, each with its
    // own first character correctly anchored on the sentinel, not on the other
    // file's last character
    assert!(dir.get_id_peer(&file_a, (0, 0)).is_ok());
    assert!(dir.get_id_peer(&file_b, (0, 0)).is_ok());

    std::fs::remove_file(&file_a).expect("failed to delete scratch file a");
    std::fs::remove_file(&file_b).expect("failed to delete scratch file b");
    std::fs::remove_dir(&root).expect("failed to delete scratch dir");
}

#[test]
fn missing_parent_errors_instead_of_panicking() {
    // regression test: add_crdt used to index straight into `characters` for the
    // parent lookup (`self.characters[&(parent_id, parent_peer)]`), which panics
    // with "no entry found for key" instead of returning an Err when the parent
    // hasn't arrived yet (e.g. an out-of-order network op). This is exactly the
    // case SharQueue's add_backlog exists to catch, so add_crdt has to return Err,
    // not crash the process.
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_missing_parent.txt");
    std::fs::write(&file_path, "a").expect("failed to write scratch file");

    let mut counter = 0;
    let mut file = SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

    let orphan = CRDT::new(999, 1, CrdtRelation::new('x', 12345, 0));
    assert!(
        file.add_crdt(0, orphan).is_err(),
        "a crdt whose parent doesn't exist yet should error, not panic"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
}

#[test]
fn removing_a_newline_merges_lines_and_keeps_anchors_aligned() {
    // regression test: line_start_ids used to be indexed independently of
    // projection (push on insert, no corresponding shift/removal on delete),
    // so it silently desynced from the actual line numbers as soon as more than
    // one line existed. Now every line's own anchor lives directly in `projection`
    // (index 0), so removing it has to merge the line the same way any other
    // removal-driven line merge would, without leaving the tombstoned anchor
    // behind as a stale entry.
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_line_merge.txt");
    std::fs::write(&file_path, "a\nb\nc").expect("failed to write scratch file");

    let mut counter = 0;
    let mut file = SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

    // ids: a=1, \n=2, b=3, \n=4, c=5. \n(2) is line 1's own anchor; \n(4) is line 2's.
    // remove the first newline (id 2, at (1, 0)): "b" should merge onto line 0, right
    // after "a"
    file.remove_crdt(1, 2, 0)
        .expect("failed to remove first newline");
    assert_eq!(
        file.get_id_peer((0, 2)),
        Some((3, 0)),
        "'b' should have merged onto line 0 right after 'a'"
    );
    assert_eq!(
        file.get_id_peer((0, 1)),
        Some((1, 0)),
        "'a' should be undisturbed at (0, 1)"
    );

    // the tombstoned newline itself should not linger as a stale entry in the
    // merged line — projection is documented to drop tombstones immediately
    assert_eq!(
        file.get_id_peer((0, 3)),
        None,
        "line 0 should have exactly the anchor, 'a', and 'b' — no leftover tombstone"
    );

    // remove the second newline (id 4, now at (1, 0) after the previous merge shifted
    // line 2 down to line 1): the merged line and "c" should merge too
    file.remove_crdt(1, 4, 0)
        .expect("failed to remove second newline");
    assert_eq!(
        file.get_id_peer((0, 3)),
        Some((5, 0)),
        "'c' should have merged onto the single remaining line"
    );

    std::fs::remove_file(&file_path).expect("failed to delete scratch file");
}
