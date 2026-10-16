//! Callbacks `SharQueue` fires on state changes, wired to emit back over the
//! owning connection's socket. See [`crate::queue_wrap::QueueWrap::new`] for
//! how these get wired up per-connection.

use socketioxide::extract::SocketRef;
use tokio::sync::mpsc::Sender;

use crate::types::{IdeAddConfirmed, NetworkAdd, NetworkOp, Remove};

pub async fn network_add_callback(socket: SocketRef, op: NetworkAdd) {
    tracing::debug!(?op, "add applied");
    loop {
        match socket.within("local").emit("network-add", &op).await {
            Ok(_) => break,
            Err(e) => {
                tracing::warn!(?op, error = %e, "network-add emit failed, retrying");
                tokio::time::sleep(std::time::Duration::from_micros(1)).await;
            }
        }
    }
}

pub async fn network_remove_callback(socket: SocketRef, op: Remove) {
    tracing::debug!(?op, "remove applied");
    loop {
        match socket.within("local").emit("network-remove", &op).await {
            Ok(_) => break,
            Err(e) => {
                tracing::warn!(?op, error = %e, "network-remove emit failed, retrying");
                tokio::time::sleep(std::time::Duration::from_micros(1)).await;
            }
        }
    }
}

pub async fn ide_remove_callback(producer: Sender<NetworkOp>, op: Remove) {
    tracing::debug!(?op, "remove applied");
    producer.send(NetworkOp::REMOVE(op)).await.expect("Inside ide_remove_callback, for some reason the consumer of this producer has stopped listening");
}

pub async fn ide_add_callback(producer: Sender<NetworkOp>, op_broadcast: NetworkAdd) {
    tracing::debug!(?op_broadcast, "add applied");
    producer.send(NetworkOp::ADD(op_broadcast)).await.expect("Inside ide_add_callback, for some reason the consumer of this producer has stopped listening");
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
