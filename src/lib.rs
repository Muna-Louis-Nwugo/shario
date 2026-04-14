#![allow(warnings)]
mod shar;

use shar::core;

fn main() {
    let queue = core::queue::SharQueue::new();
}
