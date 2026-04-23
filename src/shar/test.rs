use tokio::io::AsyncBufReadExt;
#[cfg(test)]
mod file_io {
    use crate::core::buffer;
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
}
