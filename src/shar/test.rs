#[cfg(test)]
mod tree_tests {
    use crate::shar::core::tree::{Entry, SharDirectory, SharFile};
    use crate::shar::prelude::{CrdtRelation, CRDT};
    use std::path::PathBuf;

    #[test]
    fn test_add_crdt() {
        // SharDirectory::add_crdt just routes to the right SharFile and delegates, so
        // testing through the directory covers both the routing and the underlying
        // insertion logic in one go — no need for a separate SharFile-only version
        let dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_add_crdt_dir");
        std::fs::create_dir_all(&dir_path).expect("failed to create scratch dir");
        let file_path = dir_path.join("scratch.txt");
        std::fs::write(&file_path, "ab").expect("failed to write scratch file");

        let mut dir = SharDirectory::new(dir_path.clone()).expect("failed to load directory");

        // add a character: append 'c' after 'b' on line 0
        let c = CRDT::new(3, 0, CrdtRelation::new('c', 2, 0));
        dir.add_crdt(&file_path, 0, &c, false)
            .expect("failed to add character");

        // add a line: split right after 'c', pushing everything past it onto a new line
        let newline = CRDT::new(4, 0, CrdtRelation::new('\n', 3, 0));
        dir.add_crdt(&file_path, 0, &newline, false)
            .expect("failed to add line");

        // add another character onto the new, now-empty second line — its parent is the
        // newline itself, which is never in the projection, so this needs start_line: true
        let d = CRDT::new(5, 0, CrdtRelation::new('d', 4, 0));
        dir.add_crdt(&file_path, 1, &d, true)
            .expect("failed to add character to new line");

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
        std::fs::remove_dir(&dir_path).expect("failed to delete scratch dir");
    }

    #[test]
    fn test_convergence() {
        let path_a = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_convergence_a.txt");
        let path_b = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_convergence_b.txt");
        std::fs::write(&path_a, "ab").expect("failed to write scratch file a");
        std::fs::write(&path_b, "ab").expect("failed to write scratch file b");

        let mut replica_a = SharFile::new(path_a.clone()).expect("failed to load replica a");
        let mut replica_b = SharFile::new(path_b.clone()).expect("failed to load replica b");

        // two peers concurrently insert after 'b' (id 2, peer 0) without seeing each other's op
        let op_x = CRDT::new(10, 1, CrdtRelation::new('x', 2, 0));
        let op_y = CRDT::new(7, 2, CrdtRelation::new('y', 2, 0));

        // replica_a applies them in one order...
        replica_a
            .add_crdt(&path_a, 0, &op_x, false)
            .expect("a: failed to apply x");
        replica_a
            .add_crdt(&path_a, 0, &op_y, false)
            .expect("a: failed to apply y");

        // ...replica_b applies the exact same ops in the opposite order
        replica_b
            .add_crdt(&path_b, 0, &op_y, false)
            .expect("b: failed to apply y");
        replica_b
            .add_crdt(&path_b, 0, &op_x, false)
            .expect("b: failed to apply x");

        assert_eq!(
            replica_a, replica_b,
            "replicas diverged after applying the same concurrent ops in different orders"
        );

        std::fs::remove_file(&path_a).expect("failed to delete scratch file a");
        std::fs::remove_file(&path_b).expect("failed to delete scratch file b");
    }

    #[test]
    fn test_get_id_peer() {
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_get_id_peer.txt");
        std::fs::write(&file_path, "ab").expect("failed to write scratch file");

        let mut file = SharFile::new(file_path.clone()).expect("failed to load file");

        // known positions on the one line loaded so far
        assert_eq!(file.get_id_peer((0, 0)), Some((1, 0)), "'a' should be at (0, 0)");
        assert_eq!(file.get_id_peer((0, 1)), Some((2, 0)), "'b' should be at (0, 1)");

        // out of bounds in either dimension is None, not a panic
        assert_eq!(file.get_id_peer((0, 2)), None, "line 0 only has 2 characters");
        assert_eq!(file.get_id_peer((5, 0)), None, "there's only one line");

        // the id/peer this returns has to be usable as a real parent reference: look up 'b',
        // use it as the parent for a new character, and confirm it lands right after 'b'
        let (parent_id, parent_peer) = file
            .get_id_peer((0, 1))
            .expect("'b' should still be there");
        let c = CRDT::new(3, 0, CrdtRelation::new('c', parent_id, parent_peer));
        file.add_crdt(&file_path, 0, &c, false)
            .expect("failed to add character using looked-up parent");

        assert_eq!(
            file.get_id_peer((0, 2)),
            Some((3, 0)),
            "'c' should have landed right after 'b'"
        );

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }

    #[test]
    fn test_front_of_line_insert_ordering() {
        let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_front_of_line.txt");
        // "z\n" gives a real newline (id 2) whose child line (line 1) starts out empty
        std::fs::write(&file_path, "z\n").expect("failed to write scratch file");

        let mut file = SharFile::new(file_path.clone()).expect("failed to load file");

        // two front-of-line inserts on line 1, parented on the real newline that created
        // it (id 2), in descending id order — the second one has to walk past the first
        // and land right after it, which requires the walk to actually reach the end of
        // the line instead of running out of range before it gets there
        let a = CRDT::new(30, 0, CrdtRelation::new('a', 2, 0));
        file.add_crdt(&file_path, 1, &a, true)
            .expect("failed to add first front-of-line character");

        let b = CRDT::new(20, 0, CrdtRelation::new('b', 2, 0));
        file.add_crdt(&file_path, 1, &b, true)
            .expect("failed to add second front-of-line character");

        assert_eq!(file.get_id_peer((1, 0)), Some((30, 0)));
        assert_eq!(
            file.get_id_peer((1, 1)),
            Some((20, 0)),
            "second front-of-line sibling should land right after the first, not be dropped"
        );

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }
}
