use crate::prelude::*;
use crate::shar::{core::tree::Entry, core::tree::SharDirectory, types::Operation};
use std::collections::VecDeque;
use std::path::PathBuf;
/* The Shar operation queue */

pub struct SharQueue {
    local_queue: VecDeque<Operation>,
    network_queue: VecDeque<Operation>,
    this_id: PeerIdSize,
    all_ids: Vec<PeerIdSize>,
    tree: SharDirectory,
}

impl SharQueue {
    pub fn new(
        dir_path: PathBuf,
        all_peer_ids: Vec<PeerIdSize>,
        this_peer_id: PeerIdSize,
    ) -> Result<Self> {
        /* create a new shar queue*/
        let queue = SharQueue {
            local_queue: VecDeque::new(),
            network_queue: VecDeque::new(),
            this_id: this_peer_id,
            all_ids: all_peer_ids.clone(),
            tree: SharDirectory::new(dir_path, all_peer_ids, this_peer_id)?,
        };

        Ok(queue)
    }

    pub fn add_operation(&mut self, operation: Operation) {
        /*Adds operation to the queue*/
        self.local_queue.push_front(operation);
    }
}
