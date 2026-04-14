use crate::shar::prelude::*;
use tokio::fs::File;
// use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::io::{AsyncWriteExt, BufReader, BufWriter};

use super::operation::Operation;
use crate::shar::prelude::Error;

pub struct Buffer {
    write_buffer: BufWriter<File>,
    // read_buffer: BufReader<File>,
}

impl Buffer {
    pub async fn new() -> Option<Buffer> {
        /* Creates a new write_buffer*/
        // TODO: WHEN THE TIME COMES, UPDATE THIS TO SOMEHOW TRANSMIT ACROSS A NETWORK
        // make the write_buffer
        let make_write_buffer = File::create("/home/muna/projects/shario/write_buffer.txt").await;
        // let make_read_buffer = File::open("projects/shario_output/write_buffer.txt").await;

        let write_buffer_result = match make_write_buffer {
            Ok(file) => {
                println!("write didn't buffer");
                return Some(Buffer {
                    write_buffer: BufWriter::new(file),
                    // read_buffer: BufReader::new(),
                });
            }

            // TODO: Come up with something else other than panicking
            Err(error) => {
                println!("File create Errored: {}", error);
                return None;
            }
        };
    }

    pub async fn write_gen(&mut self, operation: [u8; 14]) {
        // For now, just write to the file.
        self.write_buffer.write(&operation).await;
    }
}

pub trait FileWrite {
    async fn write(&mut self, operation: [u8; 14]);

    fn read(self) -> Result<Vec<u8>>;
}

impl FileWrite for Buffer {
    async fn write(&mut self, operation: [u8; 14]) {
        // For now, just write to the file.
        self.write_buffer.write(&operation).await;
    }

    fn read(self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
