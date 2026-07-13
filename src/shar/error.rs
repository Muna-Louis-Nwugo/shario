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
