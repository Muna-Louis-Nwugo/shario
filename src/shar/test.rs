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

    // #[tokio::test]
    // async fn test_file_write() {
    //     file_write().await;
    // }

    #[test]
    fn test_tree_creation() {
        print!("entered test_tree_creation \n");
        // initialize the tree using a test.txt file
        let test_tree = tree::SharDirectory::new("/home/muna/projects/shario/test_material");
        // let test_tree = tree::SharFile::new("/home/muna/projects/shario/test_material/test.txt");

        let tree_string: String;

        match test_tree {
            Ok(tree) => {
                tree_string = tree.to_string();
            }

            Err(e) => {
                print!("Something went wrong: {e}\n");
                return;
            }
        }

        print!("{}\n", tree_string);
    }
}
