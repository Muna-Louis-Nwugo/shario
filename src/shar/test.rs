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
                b.write_gen(operation).await;
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
