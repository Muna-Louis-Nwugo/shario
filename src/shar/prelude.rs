// general imports that will be used throughout the project

pub use super::error::Error;
pub use crate::shar::crdt;
pub use tokio::io;

// Result Alias
pub type Result<T> = core::result::Result<T, Error>;

// IO Result alias
pub type IOResult<T> = io::Result<T>;

// wrapper tuple struct (newtype pattern)
pub struct W<T>(pub T);
