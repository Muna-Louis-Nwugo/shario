use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

use super::operation::Operation;
use crate::shar::prelude::Error;

pub struct Buffer {
    buffer: BufWriter<File>,
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer {
            buffer: BufWriter::new(File::create("/projects/shario_output/buffer.txt")),
        }
    }
}

pub trait FileWrite {
    fn write(operation: Operation);

    fn read() -> Result<Vec<u8>, Error>;
}

impl FileWrite for Buffer {
    fn write(operation: Operation) {
        let mut file = File::create();
        return;
    }

    fn read() -> Result<Vec<u8>, Error> {
        Ok(Vec::new())
    }
}
