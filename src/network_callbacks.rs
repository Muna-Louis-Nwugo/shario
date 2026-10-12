//! Callbacks `SharQueue` fires on state changes, wired to emit back over the
//! owning connection's socket. See [`crate::queue_wrap::QueueWrap::new`] for
//! how these get wired up per-connection.

use iroh::defaults::prod;
use socketioxide::extract::SocketRef;
use tokio::sync::mpsc::Sender;

use crate::types::{IdeAddConfirmed, NetworkAdd, NetworkOp, Remove};

pub async fn network_add_callback(_socket: SocketRef, row: usize, col: usize) {
    tracing::debug!(row, col, "add applied");
}

pub async fn network_remove_callback(_socket: SocketRef, row: usize, col: usize) {
    tracing::debug!(row, col, "remove applied");
}

pub async fn ide_remove_callback(producer: Sender<NetworkOp>, op: Remove) {
    tracing::debug!(?op, "remove applied");
    producer.send(NetworkOp::REMOVE(op));
}

pub async fn ide_add_callback(producer: Sender<NetworkOp>, op_broadcast: NetworkAdd) {
    tracing::debug!(?op_broadcast, "add applied");
    producer.send(NetworkOp::ADD(op_broadcast));
}

pub async fn ide_add_confirm_callback(socket: SocketRef, op_return: IdeAddConfirmed) {
    tracing::debug!(?op_return, "add confirmed");
    loop {
        match socket
            .within("local")
            .emit("ide-add-confirmed", &op_return)
            .await
        {
            Ok(_) => break,
            Err(e) => {
                tracing::warn!(?op_return, error = %e, "ide-add-confirmed emit failed, retrying");
                tokio::time::sleep(std::time::Duration::from_micros(1)).await;
            }
        }
    }
}
