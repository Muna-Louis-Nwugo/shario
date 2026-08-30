//! Path constants for [`crate::shar::core::buffer::SharBuffer`].

/// Where [`crate::shar::core::buffer::SharBuffer`] writes its backing file.
///
/// This is a hardcoded local path on the original dev machine, not a portable
/// or configurable location — it's a placeholder standing in for whatever the
/// real persistence/network target ends up being, and will need to change
/// before this is used anywhere but that one machine.
///
/// Kept as `&str` because `Path::new` is not yet a stable `const fn`, so a
/// `const FILE_LOCATION: &Path` cannot be constructed. `&str` still coerces to
/// `&Path` (via `AsRef<Path>`) at every use site, e.g. `File::create(FILE_LOCATION)`.
pub const FILE_LOCATION: &str = "/home/muna/projects/shario/write_buffer.txt";
