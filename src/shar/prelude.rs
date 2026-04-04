// general imports that will be used throughout the project

pub use super::error::Error;

// Result Alias
pub type Result<T> = core::result::Result<T, Error>;

// wrapper tuple struct (newtype pattern)
pub struct W<T>(pub T);
