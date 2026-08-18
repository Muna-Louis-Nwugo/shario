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
        file.add_crdt(&file_path, (0, 1), 3, 0, &c, false)
            .expect("failed to add character");

        // add a line: split right after 'c', pushing everything past it onto a new line
        let newline = CrdtRelation::new('\n', 3, 0);
        file.add_crdt(&file_path, (0, 2), 4, 0, &newline, false)
            .expect("failed to add line");

        // add another character onto the new, now-empty second line
        let d = CrdtRelation::new('d', 4, 0);
        file.add_crdt(&file_path, (1, 0), 5, 0, &d, false)
            .expect("failed to add character to new line");

        std::fs::remove_file(&file_path).expect("failed to delete scratch file");
    }
}
