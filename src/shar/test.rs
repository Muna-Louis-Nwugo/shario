#[cfg(test)]
mod tree_tests {
    use crate::shar::core::tree::{SharDirectory, SharFile};
    use crate::shar::prelude::{CRDT, CrdtRelation};
    use std::path::PathBuf;

    #[test]
    fn test_add_crdt() {
        // SharDirectory::add_crdt just routes to the right SharFile and delegates, so
        // testing through the directory covers both the routing and the underlying
        // insertion logic in one go — no need for a separate SharFile-only version
        let dir_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_add_crdt_dir");
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
        // the ring search has to walk all the way down to line 7 to find it. "FINAL_LINE_END"
        // is 14 characters (indices 0-13), so '!' lands at index 14
        let extra = CRDT::new(last_id + 1, 0, CrdtRelation::new('!', last_id, 0));
        let position = dir
            .add_crdt(&file_path, 0, extra, false)
            .expect("failed to append after the last character");
        assert_eq!(
            position,
            Some((7, 14)),
            "'!' should land right after the last character"
        );

        // split a new line right after that character — a line-break CRDT reports the
        // position of the character it split right after, not a position of its own
        let newline = CRDT::new(last_id + 2, 0, CrdtRelation::new('\n', last_id + 1, 0));
        let position = dir
            .add_crdt(&file_path, 0, newline, false)
            .expect("failed to add a line at the end");
        assert_eq!(
            position,
            Some((7, 14)),
            "the split should report where it split"
        );

        // add the first character of the freshly-created line (line 8) — its parent is
        // the newline, which is never in the projection, so this needs start_line: true.
        // The line was empty, so this hits the empty-line fast path at column 0
        let last_char = CRDT::new(last_id + 3, 0, CrdtRelation::new('X', last_id + 2, 0));
        let position = dir
            .add_crdt(&file_path, 8, last_char, true)
            .expect("failed to add character to the new final line");
        assert_eq!(
            position,
            Some((8, 0)),
            "'X' should be the sole character on the new line"
        );

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
        std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
    }

    #[test]
    fn test_convergence() {
        let path_a = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scratch_convergence_a.txt");
        let path_b = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scratch_convergence_b.txt");
        let path_c = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scratch_convergence_c.txt");

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
                .add_crdt(0, op_x.clone(), false)
                .expect("a: failed to apply x")
                .is_some(),
            "a: x should be a real insert, not a no-op"
        );
        assert!(
            replica_a
                .add_crdt(0, op_y.clone(), false)
                .expect("a: failed to apply y")
                .is_some(),
            "a: y should be a real insert, not a no-op"
        );
        assert!(
            replica_a
                .add_crdt(0, op_z.clone(), false)
                .expect("a: failed to apply z")
                .is_some(),
            "a: z should be a real insert, not a no-op"
        );

        // ...replica_b applies them in the reverse order...
        assert!(
            replica_b
                .add_crdt(0, op_z.clone(), false)
                .expect("b: failed to apply z")
                .is_some(),
            "b: z should be a real insert, not a no-op"
        );
        assert!(
            replica_b
                .add_crdt(0, op_y.clone(), false)
                .expect("b: failed to apply y")
                .is_some(),
            "b: y should be a real insert, not a no-op"
        );
        assert!(
            replica_b
                .add_crdt(0, op_x.clone(), false)
                .expect("b: failed to apply x")
                .is_some(),
            "b: x should be a real insert, not a no-op"
        );

        // ...and replica_c applies them in yet another order
        assert!(
            replica_c
                .add_crdt(0, op_y.clone(), false)
                .expect("c: failed to apply y")
                .is_some(),
            "c: y should be a real insert, not a no-op"
        );
        assert!(
            replica_c
                .add_crdt(0, op_x.clone(), false)
                .expect("c: failed to apply x")
                .is_some(),
            "c: x should be a real insert, not a no-op"
        );
        assert!(
            replica_c
                .add_crdt(0, op_z.clone(), false)
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
        let file_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_get_id_peer.txt");
        // four lines of 9 characters each (except the last), no trailing newline
        let content = "abc123!@#\ndef456$%^\nghi789&*(\nLAST";
        std::fs::write(&file_path, content).expect("failed to write scratch file");

        let mut counter = 0;
        let mut file =
            SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

        // spot-check known positions across every line, not just the first
        assert_eq!(
            file.get_id_peer((0, 0)),
            Some((1, 0)),
            "'a' should be at (0, 0)"
        );
        assert_eq!(
            file.get_id_peer((0, 8)),
            Some((9, 0)),
            "'#' should be at (0, 8)"
        );
        assert_eq!(
            file.get_id_peer((1, 0)),
            Some((11, 0)),
            "'d' should be at (1, 0)"
        );
        assert_eq!(
            file.get_id_peer((1, 8)),
            Some((19, 0)),
            "'^' should be at (1, 8)"
        );
        assert_eq!(
            file.get_id_peer((2, 0)),
            Some((21, 0)),
            "'g' should be at (2, 0)"
        );
        assert_eq!(
            file.get_id_peer((2, 8)),
            Some((29, 0)),
            "'(' should be at (2, 8)"
        );
        assert_eq!(
            file.get_id_peer((3, 0)),
            Some((31, 0)),
            "'L' should be at (3, 0)"
        );
        assert_eq!(
            file.get_id_peer((3, 3)),
            Some((34, 0)),
            "'T' should be at (3, 3)"
        );

        // out of bounds in either dimension is None, not a panic
        assert_eq!(
            file.get_id_peer((0, 9)),
            None,
            "line 0 only has 9 characters"
        );
        assert_eq!(
            file.get_id_peer((3, 4)),
            None,
            "line 3 only has 4 characters"
        );
        assert_eq!(file.get_id_peer((10, 0)), None, "there are only 4 lines");

        // the id/peer this returns has to be usable as a real parent reference: look up
        // the last character of the last line, use it as a parent, and confirm the new
        // character lands right after it
        let (parent_id, parent_peer) = file.get_id_peer((3, 3)).expect("'T' should still be there");
        let c = CRDT::new(35, 0, CrdtRelation::new('!', parent_id, parent_peer));
        let position = file
            .add_crdt(3, c, false)
            .expect("failed to add character using looked-up parent");
        assert_eq!(position, Some((3, 4)), "'!' should land right after 'T'");

        assert_eq!(
            file.get_id_peer((3, 4)),
            Some((35, 0)),
            "'!' should have landed right after 'T'"
        );

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }

    #[test]
    fn test_front_of_line_insert_ordering() {
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scratch_front_of_line.txt");
        // messy first line ending in a real newline (id == char count), whose child
        // line (line 1) starts out empty
        let content = "some prefix line !@# \u{1f600} 123\n";
        std::fs::write(&file_path, content).expect("failed to write scratch file");

        let mut counter = 0;
        let mut file =
            SharFile::new(file_path.clone(), &mut counter).expect("failed to load file");

        let newline_id = content.chars().count() as u32;
        let new_line = 1;

        // four front-of-line inserts, applied in a deliberately scrambled (non-sorted)
        // order, all parented on the same real newline — final order must still be
        // strictly descending by id regardless of application order, which means each
        // one has to correctly walk past however many are already there. Ids are chosen
        // comfortably above the file's own real character count so none of them collide
        // with (and get silently no-op'd against) real content already loaded
        let a = CRDT::new(1050, 0, CrdtRelation::new('a', newline_id, 0));
        let position = file
            .add_crdt(new_line, a, true)
            .expect("failed to add front-of-line character 'a'");
        assert_eq!(
            position,
            Some((1, 0)),
            "'a' is the only thing on the line so far"
        );

        let b = CRDT::new(1010, 0, CrdtRelation::new('b', newline_id, 0));
        let position = file
            .add_crdt(new_line, b, true)
            .expect("failed to add front-of-line character 'b'");
        assert_eq!(
            position,
            Some((1, 1)),
            "'b' has a smaller id, so it lands after 'a'"
        );

        let c = CRDT::new(1999, 0, CrdtRelation::new('c', newline_id, 0));
        let position = file
            .add_crdt(new_line, c, true)
            .expect("failed to add front-of-line character 'c'");
        assert_eq!(
            position,
            Some((1, 0)),
            "'c' has the largest id, so it jumps to the front"
        );

        let d = CRDT::new(1500, 0, CrdtRelation::new('d', newline_id, 0));
        let position = file
            .add_crdt(new_line, d, true)
            .expect("failed to add front-of-line character 'd'");
        assert_eq!(
            position,
            Some((1, 1)),
            "'d' lands right after 'c', before 'a'"
        );

        // final order must be strictly descending: 1999, 1500, 1050, 1010
        assert_eq!(file.get_id_peer((new_line, 0)), Some((1999, 0)));
        assert_eq!(file.get_id_peer((new_line, 1)), Some((1500, 0)));
        assert_eq!(file.get_id_peer((new_line, 2)), Some((1050, 0)));
        assert_eq!(file.get_id_peer((new_line, 3)), Some((1010, 0)));

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }

    #[test]
    fn test_remove_crdt() {
        // same reasoning as test_add_crdt — SharDirectory::remove_crdt just routes to the
        // right SharFile and delegates, so test through the directory to cover routing and
        // the underlying tombstone logic together
        let dir_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scratch_remove_crdt_dir");
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
        // build an actual multi-level tombstone chain, not just a single removed node
        dir.remove_crdt(&file_path, 0, 10, 0)
            .expect("failed to remove first character in the chain");
        dir.remove_crdt(&file_path, 0, 11, 0)
            .expect("failed to remove second character in the chain");
        dir.remove_crdt(&file_path, 0, 12, 0)
            .expect("failed to remove third character in the chain");

        // retrying an already-removed one is a no-op, not an error
        dir.remove_crdt(&file_path, 0, 11, 0)
            .expect("failed to no-op a repeated removal");

        // removing something that was never added at all is an error
        assert!(
            dir.remove_crdt(&file_path, 0, 999_999, 0).is_err(),
            "removing a nonexistent crdt should fail, not succeed"
        );

        // add a character parented on the deepest tombstone (id 12) — resolving this has
        // to climb all three tombstoned levels back to the nearest live ancestor (id 9),
        // exercising find_tombstone's recursion through the full directory-routed path
        let new_char = CRDT::new(999, 0, CrdtRelation::new('!', 12, 0));
        let position = dir
            .add_crdt(&file_path, 0, new_char, false)
            .expect("failed to add a character parented on a 3-deep tombstone chain");
        assert_eq!(
            position,
            Some((0, 9)),
            "should land right after id 9, the nearest live ancestor left after the chain"
        );

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
        std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
    }
}
