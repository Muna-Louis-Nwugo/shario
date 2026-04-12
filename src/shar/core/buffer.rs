use crate::shar::prelude::*;
use tokio::fs::File;
// use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::io::{AsyncWriteExt, BufWriter};

use super::operation::Operation;
use crate::shar::prelude::Error;

pub struct Buffer {
    buffer: BufWriter<File>,
}

impl Buffer {
    pub async fn new() {
        /* Creates a new buffer*/
        // TODO: WHEN THE TIME COMES, UPDATE THIS TO SOMEHOW TRANSMIT ACROSS A NETWORK
        // make the buffer
        let make_buffer = File::create("/projects/shario_output/buffer.txt").await;

        let buffer_result = match make_buffer {
            Ok(file) => {
                Buffer {
                    buffer: BufWriter::new(file),
                };
            }

            // TODO: Come up with something else other than panicking
            Err(error) => {
                panic!("File failed to open")
            }
        };
    }
}

pub trait FileWrite {
    async fn write(&mut self, operation: [u8; 14]);

    fn read(self) -> Result<Vec<u8>>;
}

impl FileWrite for Buffer {
    async fn write(&mut self, operation: [u8; 14]) {
        // For now, just write to the file.
        self.buffer.write(&operation).await;
    }

    fn read(self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
