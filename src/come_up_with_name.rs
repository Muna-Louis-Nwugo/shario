use crate::core::buffer::SharBuffer;
use std::sync::OnceLock;

use crate::shar::core::{queue::SharQueue, tree::SharDirectory};

static TREE: OnceLock<SharDirectory> = OnceLock::new();
static QUEUE: OnceLock<SharQueue> = OnceLock::new();
static BUFFER: OnceLock<SharBuffer> = OnceLock::new();
