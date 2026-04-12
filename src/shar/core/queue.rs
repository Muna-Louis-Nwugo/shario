use crate::core::operation::Operation;
use std::collections::VecDeque;
/* The Shar operation queue */

pub struct SharQueue {
    queue: VecDeque<Operation>,
}

impl SharQueue {
    pub fn new() -> Self {
        /* create a new shar queue*/
        SharQueue {
            queue: VecDeque::new(),
        }
    }

    pub fn add_operation(&mut self, operation: Operation) {
        /*Adds operation to the queue*/
        self.queue.push_front(operation);
    }
}
