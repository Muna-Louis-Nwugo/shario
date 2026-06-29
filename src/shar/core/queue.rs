use crate::shar::{core::tree::SharDirectory, types::Operation};
use std::collections::VecDeque;
/* The Shar operation queue */

pub struct SharQueue {
    local_queue: VecDeque<Operation>,
    network_queue: VecDeque<Operation>,
    tree: SharDirectory,
}

impl SharQueue {
    pub fn new() -> Self {
        /* create a new shar queue*/
        SharQueue {
            local_queue: VecDeque::new(),
            network_queue: VecDeque::new(),
            tree: SharDirectory::new(),
        }
    }

    pub fn add_operation(&mut self, operation: Operation) {
        /*Adds operation to the queue*/
        self.local_queue.push_front(operation);
    }
}
