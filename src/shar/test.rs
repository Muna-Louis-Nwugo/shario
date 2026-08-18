#[cfg(test)]
mod tree_tests {
    use crate::shar::core::tree::{Entry, SharFile};
    use crate::shar::prelude::CrdtRelation;
    use std::path::PathBuf;

    #[test]
    fn test_add_crdt() {
        let file_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_material/scratch_add_crdt.txt");
        std::fs::write(&file_path, "ab").expect("failed to write scratch file");

        let mut file = SharFile::new(file_path.clone()).expect("failed to load file");

        // add a character: append 'c' after 'b' on line 0
        let c = CrdtRelation::new('c', 2, 0);
        file.add_crdt(&file_path, 0, 3, 0, &c, false)
            .expect("failed to add character");

        // add a line: split right after 'c', pushing everything past it onto a new line
        let newline = CrdtRelation::new('\n', 3, 0);
        file.add_crdt(&file_path, 0, 4, 0, &newline, false)
            .expect("failed to add line");

        // add another character onto the new, now-empty second line — its parent is the
        // newline itself, which is never in the projection, so this needs start_line: true
        let d = CrdtRelation::new('d', 4, 0);
        file.add_crdt(&file_path, 1, 5, 0, &d, true)
            .expect("failed to add character to new line");

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }

    #[test]
    fn convergence_two_replicas_different_op_order() {
        let path_a = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_convergence_a.txt");
        let path_b = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_material/scratch_convergence_b.txt");
        std::fs::write(&path_a, "ab").expect("failed to write scratch file a");
        std::fs::write(&path_b, "ab").expect("failed to write scratch file b");

        let mut replica_a = SharFile::new(path_a.clone()).expect("failed to load replica a");
        let mut replica_b = SharFile::new(path_b.clone()).expect("failed to load replica b");

        // two peers concurrently insert after 'b' (id 2, peer 0) without seeing each other's op
        let op_x = CrdtRelation::new('x', 2, 0); // id 10, peer 1
        let op_y = CrdtRelation::new('y', 2, 0); // id 7, peer 2

        // replica_a applies them in one order...
        replica_a
            .add_crdt(&path_a, 0, 10, 1, &op_x, false)
            .expect("a: failed to apply x");
        replica_a
            .add_crdt(&path_a, 0, 7, 2, &op_y, false)
            .expect("a: failed to apply y");

        // ...replica_b applies the exact same ops in the opposite order
        replica_b
            .add_crdt(&path_b, 0, 7, 2, &op_y, false)
            .expect("b: failed to apply y");
        replica_b
            .add_crdt(&path_b, 0, 10, 1, &op_x, false)
            .expect("b: failed to apply x");

        assert_eq!(
            replica_a, replica_b,
            "replicas diverged after applying the same concurrent ops in different orders"
        );

        std::fs::remove_file(&path_a).expect("failed to delete scratch file a");
        std::fs::remove_file(&path_b).expect("failed to delete scratch file b");
    }
}
