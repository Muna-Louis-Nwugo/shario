//! A placeholder persistence layer. `SharBuffer` currently just appends raw
//! bytes to a single hardcoded local file — it exists as a stand-in for
//! whatever eventually carries operations across the network, not as a real
//! save/load mechanism yet. See [`crate::shar::io::io_info::FILE_LOCATION`].

use crate::shar::error::Error;
use crate::shar::io::io_info;
use crate::shar::prelude::*;
use tokio::fs::File;
// use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio::io::{AsyncWriteExt, BufWriter};

/// Wraps a single buffered file handle that operations get written to.
/// Read-back isn't implemented yet (see [`FileWrite::read`]).
pub struct SharBuffer {
    write_buffer: BufWriter<File>,
    // read_buffer: BufReader<File>,
}

impl SharBuffer {
    /// Opens (creating/truncating) the buffer's backing file at
    /// [`io_info::FILE_LOCATION`] and wraps it for buffered writes.
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

    /// Writes a fixed 14-byte operation to the buffer and flushes immediately.
    /// Errors are logged and swallowed rather than propagated.
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

/// A generic byte-oriented write/read interface, so buffer backends other than
/// [`SharBuffer`] can eventually be swapped in behind the same API.
pub trait FileWrite {
    /// Writes one fixed-size 14-byte operation.
    fn write(&mut self, operation: [u8; 14]) -> impl std::future::Future<Output = ()> + Send;

    /// Reads back previously-written operations. Currently a stub — always
    /// returns an empty `Vec`, nothing is actually read from disk yet.
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
