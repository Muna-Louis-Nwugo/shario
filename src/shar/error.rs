// Error crate

#[derive(thiserror::Error, Debug)]
pub enum Error {
    // remove this generic as the problem progresses
    #[error("Generic: {0}")]
    Generic(String),

    #[error("ReadFail: {0}")]
    ReadFail(String),
}
