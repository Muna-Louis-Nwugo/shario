use crate::shar::prelude::*;
use tokio::fs::File;
use tokio::io::{self, AsyncWriteExt, BufWriter};

use super::operation::Operation;
use crate::shar::prelude::Error;

pub struct Buffer {
    buffer: File,
}

impl Buffer {
    pub async fn new() {
        /* Creates a new buffer*/
        // TODO: WHEN THE TIME COMES, UPDATE THIS TO SOMEHOW TRANSMIT ACROSS A NETWORK
        // make the buffer
        let make_buffer = File::open("/projects/shario_output/buffer.txt").await;

        let buffer_result = match make_buffer {
            Ok(file) => {
                Buffer { buffer: file };
            }

            // TODO: Come up with something else other than panicking
            Err(error) => {
                panic!("File failed to open")
            }
        };
    }
}

pub trait FileWrite {
    fn write(operation: Operation);

    fn read() -> Result<Vec<u8>>;
}

impl FileWrite for Buffer {
    fn write(operation: Operation) {
        let mut file = File::create("/projects/shario_output/buffer.txt");
        return;
    }

    fn read() -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
