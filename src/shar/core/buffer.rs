use crate::shar::prelude::*;
use tokio::fs::File;
use tokio::io::{self, AsyncWriteExt, BufWriter};

use super::operation::Operation;
use crate::shar::prelude::Error;

pub struct Buffer {
    buffer: BufWriter<IOResult<File>>,
}

impl Buffer {
    pub async fn new() -> Self {
        Buffer {
            buffer: BufWriter::new(File::create("/projects/shario_output/buffer.txt").await),
        }
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
