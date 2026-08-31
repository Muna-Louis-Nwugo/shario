//! Placeholder persistence layer — writes raw bytes to a single hardcoded file.

use crate::shar::error::Error;
use crate::shar::io::io_info;
use crate::shar::prelude::*;
use tokio::fs::File;
// use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::io::{AsyncWriteExt, BufWriter};

/// A buffered file handle that operations get written to. No read-back yet.
pub struct SharBuffer {
    write_buffer: BufWriter<File>,
    // read_buffer: BufReader<File>,
}

impl SharBuffer {
    /// Opens the buffer's backing file at [`io_info::FILE_LOCATION`].
    pub async fn new() -> Result<SharBuffer> {
        /* Creates a new write_buffer*/
        // TODO: WHEN THE TIME COMES, UPDATE THIS TO SOMEHOW TRANSMIT ACROSS A NETWORK

        // make the write_buffer
        let make_write_buffer = File::create(io_info::FILE_LOCATION).await;
        // let make_read_buffer = File::open("projects/shario_output/write_buffer.txt").await;

        match make_write_buffer {
            Ok(file) => Ok(SharBuffer {
                write_buffer: BufWriter::new(file),
            }),

            // TODO: Come up with something else other than panicking
            Err(error) => Err(Error::Generic(format!("File create errored: {error}",))),
        }
    }

    /// Writes and flushes a 14-byte operation. Errors are logged, not propagated.
    pub async fn write_general(&mut self, operation: [u8; 14]) {
        println!("write_gen entered");

        // 1. Perform the write and await it
        if let Err(e) = self.write_buffer.write_all(&operation).await {
            eprintln!("Failed to write: {}", e);
            return;
        }

        // 2. Perform the flush and await it
        if let Err(e) = self.write_buffer.flush().await {
            eprintln!("Failed to flush: {}", e);
            return;
        }

        println!("buffer has been written and flushed");
    }
}

/// Generic byte-oriented write/read interface for buffer backends.
pub trait FileWrite {
    fn write(&mut self, operation: [u8; 14]) -> impl std::future::Future<Output = ()> + Send;

    /// Stub — always returns an empty `Vec`.
    fn read(self) -> Result<Vec<u8>>;
}

impl FileWrite for SharBuffer {
    async fn write(&mut self, operation: [u8; 14]) {
        // For now, just write to the file.
        let _ = self.write_buffer.write(&operation).await;
    }

    fn read(self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
