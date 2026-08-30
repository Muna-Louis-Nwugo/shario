//! The single error type shared across the whole `shar` crate.

/// All the ways a `shar` operation can fail.
///
/// This is deliberately one flat enum rather than one error type per module —
/// everything bubbles up through the crate's [`crate::shar::prelude::Result`] alias.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A catch-all for failures that don't yet have their own variant.
    // remove this generic as the problem progresses
    #[error("Generic: {0}")]
    Generic(String),

    /// Reading a file (or directory) from disk failed.
    #[error("ReadFail: {0}")]
    ReadFail(String),

    /// Wraps a `std::io::Error` from an underlying filesystem/IO call.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Something went wrong while initializing a `Shar`/`SharQueue`/`SharDirectory`.
    #[error("InitFail: {0}")]
    InitFail(String),

    /// An operation referenced a peer or origin this replica doesn't recognize.
    #[error("UnknownOrigin: {0}")]
    UnknownOrigin(String),

    /// A lookup or index fell outside the valid range (e.g. a coordinate, or a
    /// tree/ring search that couldn't find its target within bounds).
    #[error("OutOfBounds: {0}")]
    OutOfBounds(String),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Generic(a), Self::Generic(b)) => a == b,
            (Self::ReadFail(a), Self::ReadFail(b)) => a == b,
            (Self::InitFail(a), Self::InitFail(b)) => a == b,
            (Self::UnknownOrigin(a), Self::UnknownOrigin(b)) => a == b,
            (Self::OutOfBounds(a), Self::OutOfBounds(b)) => a == b,
            // Compare the underlying error kind for the std::io::Error payload
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            _ => false,
        }
    }
}
