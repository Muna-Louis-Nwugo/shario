pub mod come_up_with_name;
pub mod shar;

use shar::core;

fn main() {
    let queue = core::queue::SharQueue::new();
}
