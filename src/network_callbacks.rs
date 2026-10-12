//! Callbacks `SharQueue` fires on state changes, wired to emit back over the
//! owning connection's socket. See [`crate::queue_wrap::QueueWrap::new`] for
//! how these get wired up per-connection.

use socketioxide::extract::SocketRef;

use crate::types::{IdeAddConfirmed, NetworkAdd, Remove};

pub async fn network_add_callback(_socket: SocketRef, row: usize, col: usize) {
    tracing::debug!(row, col, "add applied");
}

pub async fn network_remove_callback(_socket: SocketRef, row: usize, col: usize) {
    tracing::debug!(row, col, "remove applied");
}

pub async fn ide_remove_callback(socket: SocketRef, op: Remove) {
    tracing::debug!(?op, "remove applied");
    let _ = socket.within("network").emit("network-remove", &op).await;
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

pub async fn ide_add_callback(socket: SocketRef, op_broadcast: NetworkAdd) {
    tracing::debug!(?op_broadcast, "add applied");
    let _ = socket
        .within("network")
        .emit("network-add", &op_broadcast)
        .await;
}
