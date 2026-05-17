use tokio::io::AsyncBufReadExt;
#[cfg(test)]
mod file_io {
    use crate::core::buffer;
    use crate::core::tree;
    use tokio;

    async fn file_write() {
        let operation = [1u8; 14];
        let mut buff = buffer::Buffer::new().await;
        println!("buff finished running");

        match buff {
            Some(mut b) => {
                println!("file successfully created");
                println!("calling write_gen");
                b.write_general(operation).await;
            }

            None => {
                panic!("nothing to see here");
            }
        }
    }

    #[tokio::test]
    async fn test_file_write() {
        file_write().await;
    }

    async fn test_tree_creation() {
        // initialize the tree using a test.txt file
        let test_tree = tree::SharDirectory::new("/home/muna/projects/shario/test_material");

        print!(test_tree.to_string());
    }
}
