//! The crate's error type.

/// All the ways a `shar` operation can fail.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Catch-all for failures without their own variant yet.
    // remove this generic as the problem progresses
    #[error("Generic: {0}")]
    Generic(String),

    /// Reading a file or directory failed.
    #[error("ReadFail: {0}")]
    ReadFail(String),

    /// Wraps a `std::io::Error`.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Initialization failed.
    #[error("InitFail: {0}")]
    InitFail(String),

    /// Referenced a peer or origin this replica doesn't recognize.
    #[error("UnknownOrigin: {0}")]
    UnknownOrigin(String),

    /// A lookup or index fell outside the valid range.
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
