//! Wraps `SharQueue` for shared access across every socket.io connection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use socketioxide::extract::SocketRef;
use tokio::sync::RwLock;
use tokio::sync::mpsc::Sender;

use crate::network_callbacks::{
    ide_add_callback, ide_add_confirm_callback, ide_remove_callback, network_add_callback,
    network_remove_callback,
};
use crate::shar::core::queue::SharQueue;
use crate::shar::error::Error;
use crate::shar::prelude::{IdSize, PeerIdSize};
use crate::types::{CrdtRelation, IdeAdd, NetworkAdd, NetworkOp, Remove};

/// Wraps the queue for use across rooms
#[derive(Clone, Default, Debug)]
pub struct QueueWrap {
    queue: Arc<RwLock<SharQueue>>,
}

impl QueueWrap {
    pub async fn new(
        &mut self,
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        socket: SocketRef,
        tx: Sender<NetworkOp>,
    ) -> Result<(), Error> {
        let socket1 = socket.clone();
        let socket2 = socket.clone();
        let socket3 = socket.clone();
        let tx1 = tx.clone();
        let tx2 = tx.clone();

        let real_queue = SharQueue::new(
            dir_path,
            this_peer_id,
            Box::new(move |op| Box::pin(network_add_callback(socket1.clone(), op))),
            Box::new(move |op| Box::pin(network_remove_callback(socket2.clone(), op))),
            Box::new(move |op_return| {
                Box::pin(ide_add_confirm_callback(socket3.clone(), op_return))
            }),
            Box::new(move |op_broadcast| Box::pin(ide_add_callback(tx1.clone(), op_broadcast))),
            Box::new(move |remove| Box::pin(ide_remove_callback(tx2.clone(), remove))),
        )?;
        let mut guard = self.queue.write().await; // locks the *shared* RwLock every clone points at
        if guard.network_add_callback.is_none() {
            *guard = real_queue; // overwrites its contents, not the Arc itself
        }
        Ok(())
    }

    pub async fn add_ide_operation(&self, op: IdeAdd) -> Result<(), Error> {
        let mut queue = self.queue.write().await;
        queue.add_ide_operation(op).await
    }

    pub async fn remove_ide_operation(&self, op: Remove) {
        let mut queue = self.queue.write().await;
        queue.remove_ide_operation(op).await
    }

    pub async fn add_network_operation(&self, op: NetworkAdd) {
        let mut queue = self.queue.write().await;
        queue.add_network_operation(op).await
    }

    pub async fn remove_network_operation(&self, op: Remove) {
        let mut queue = self.queue.write().await;
        queue.remove_network_operation(op).await
    }

    pub async fn identities(&self) -> Vec<(PathBuf, Vec<Vec<(IdSize, PeerIdSize)>>)> {
        let queue = self.queue.read().await;
        queue.identities()
    }

    pub async fn full_state(&self) -> Vec<(PathBuf, HashMap<(IdSize, PeerIdSize), CrdtRelation>)> {
        let queue = self.queue.read().await;
        queue.full_state()
    }
}
