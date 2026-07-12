// NOTE: kept as `&str` because `Path::new` is not yet a stable `const fn`, so a
// `const FILE_LOCATION: &Path` cannot be constructed. `&str` still coerces to `&Path`
// (via `AsRef<Path>`) at every use site, e.g. `File::create(FILE_LOCATION)`.
pub const FILE_LOCATION: &str = "/home/muna/projects/shario/write_buffer.txt";
