use crate::shar::core::operation;
use std::collections::VecDeque;
/* The Shar operation queue */

pub struct SharQueue {
    queue: VecDeque<operation::Operation>,
}

impl SharQueue {
    pub fn new() -> SharQueue {
        /* create a new shar queue*/
        SharQueue {
            queue: VecDeque::new(),
        }
    }
}
