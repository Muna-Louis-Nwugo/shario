use crate::shar::core::operation;
use std::collections::VecDeque;
/* The Shar operation queue */

pub struct SharQueue {
    queue: VecDeque<operation::Operation>,
}
