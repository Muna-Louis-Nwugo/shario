// Error crate

#[derive(thiserror::Error, Debug)]
pub enum Error {
    // remove this generic as the problem progresses
    #[error("Generic: {0}")]
    Generic(String),

    #[error("ReadFail: {0}")]
    ReadFail(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("InitFail: {0}")]
    InitFail(String),

    #[error("UnknownOrigin: {0}")]
    UnknownOrigin(String),

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
