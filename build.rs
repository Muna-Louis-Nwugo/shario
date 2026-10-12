//! Reads a local, gitignored `.env` file and forwards each `KEY=VALUE` line
//! to cargo as a compile-time env var, so `env!("REDIS_URL")` in main.rs
//! picks it up automatically without needing a manual `export` every time.
//! If `.env` doesn't exist, this just does nothing -- a plain shell
//! `export` still works as a fallback.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=.env");

    let Ok(contents) = fs::read_to_string(".env") else {
        return;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            // strip one matching pair of surrounding quotes, if present --
            // .env values are often quoted (`KEY="value"`), but that's a
            // shell-sourcing convention, not something split_once/trim
            // strips, so without this the literal quote chars end up baked
            // into the compiled-in string.
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            println!("cargo:rustc-env={}={}", key.trim(), value);
        }
    }
}
