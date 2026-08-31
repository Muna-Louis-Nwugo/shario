//! Path constants for [`crate::shar::core::buffer::SharBuffer`].

/// Where [`crate::shar::core::buffer::SharBuffer`] writes its backing file.
/// Hardcoded to one dev machine, not portable yet.
///
/// `&str` rather than `&Path`: `Path::new` isn't a stable `const fn` yet.
pub const FILE_LOCATION: &str = "/home/muna/projects/shario/write_buffer.txt";
