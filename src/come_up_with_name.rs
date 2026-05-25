use std::sync::OnceLock;

use crate::shar::core::tree::SharDirectory;

static TREE: OnceLock<SharDirectory> = OnceLock::new();
