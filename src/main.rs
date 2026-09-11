// Logging: `cargo run` prints to stdout at the default `shario=info` level
// (connects/joins/errors only). For per-keystroke `debug!` lines from the
// socket handlers and SharQueue, run `RUST_LOG=shario=debug cargo run`
// instead. This is server-side logging only -- separate from the "Shar"
// output channel on the VS Code extension side, which only shows what the
// client sent, not what the server did with it.

mod shar;
#[cfg(test)]
mod tests;
mod types;

use std::{path::PathBuf, sync::Arc};

use axum::routing::get;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef, State},
};

use crate::{
    shar::core::queue::SharQueue,
    shar::error::Error,
    shar::prelude::PeerIdSize,
    types::{Connect, IdeAdd, IdeRemove, NetworkAdd, NetworkRemove},
};

/// Defines available Shar Commands

/// Wraps the queue for use across rooms
#[derive(Clone, Default)]
struct QueueWrap {
    queue: Arc<RwLock<SharQueue>>,
}

impl QueueWrap {
    pub async fn new(
        &mut self,
        dir_path: PathBuf,
        this_peer_id: PeerIdSize,
        add_callback: fn(usize, usize),
        remove_callback: fn(usize, usize, bool),
    ) -> Result<(), Error> {
        let real_queue = SharQueue::new(
            dir_path,
            this_peer_id,
            network_add_callback,
            network_remove_callback,
            ide_add_callback,
            ide_remove_callback,
        )?;
        let mut guard = self.queue.write().await; // locks the *shared* RwLock every clone points at
        if guard.network_add_callback.is_none() {
            *guard = real_queue; // overwrites its contents, not the Arc itself
        }
        Ok(())
    }

    pub async fn add_ide_operation(&self, op: IdeAdd) -> Result<(), Error> {
        let mut queue = self.queue.write().await;
        queue.add_ide_operation(op)
    }

    pub async fn remove_ide_operation(&self, op: IdeRemove) -> Result<(), Error> {
        let mut queue = self.queue.write().await;
        queue.remove_ide_operation(op)
    }

    pub async fn add_network_operation(&self, op: NetworkAdd) {
        let mut queue = self.queue.write().await;
        queue.add_network_operation(op);
    }

    pub async fn remove_network_operation(&self, op: NetworkRemove) {
        let mut queue = self.queue.write().await;
        queue.remove_network_operation(op);
    }
}

#[tokio::main]
async fn main() {
    // logs go to both stdout and ./shar.<date>.log so a flood -- like a
    // runaway backlog -- can be reviewed in one file after the fact instead
    // of scrolling back through the terminal. Rotates daily and keeps only
    // the last 5 files so it doesn't grow forever across restarts. `_guard`
    // has to stay alive for the program to keep flushing to the file; it's
    // held here for the rest of main's scope, which is main's entire
    // lifetime since axum::serve below blocks until shutdown.
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("shar")
        .filename_suffix("log")
        .max_log_files(5)
        .build(".")
        .expect("failed to set up log file appender");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // env-filter reads RUST_LOG (e.g. `RUST_LOG=shario=debug`); defaults to
    // "info" for this crate if RUST_LOG isn't set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("shario=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false), // no color codes cluttering the log file
        )
        .init();

    // set up web server
    let (layer, io) = SocketIo::builder()
        // provides the state that is the queue and tree to the server
        .with_state(QueueWrap::default())
        .build_layer();

    // giving connect handler to the "/" namespace
    io.ns("/", on_connect);

    // .route() sets up the HTTP handler
    let app = axum::Router::<()>::new()
        .route("/", get(async || println!("Connecting...")))
        .layer(
            // handles CORS for us
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                // the actual SocketIO Layer
                .layer(layer),
        );

    let listener = tokio::net::TcpListener::bind(&"127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn on_connect(socket: SocketRef) {
    tracing::info!(socket_id = %socket.id, "socket connected");

    // Note: we log with plain `tracing::info!` events, not by entering a
    // span and holding its guard across an `.await`. A span guard isn't
    // `Send`, and each `.await` in these closures is a point where tokio
    // can move the task to a different worker thread -- holding a guard
    // across that either won't compile or attributes the span to the wrong
    // task. `#[tracing::instrument]` sidesteps this (it re-enters the span
    // around every poll), but it only works on named fns, not closures, so
    // plain events are the simplest correct option here.
    socket.on(
        "join",
        async |socket: SocketRef, Data::<Connect>(data), mut queue: State<QueueWrap>| {
            tracing::info!(socket_id = %socket.id, local = data.local, path = ?data.path, "join");

            // leave all existing rooms
            let _ = socket.leave_all();

            // join the appropriate room on the connect message
            if data.local {
                // creates a new "queue"
                // right now, this is just a stub
                if let Err(e) = queue
                    .new(data.path, 1, network_add_callback, network_remove_callback)
                    .await
                {
                    tracing::error!(socket_id = %socket.id, error = %e, "failed to initialize queue");
                }
                socket.join("local");
            } else {
                socket.join("network");
            }
        },
    );

    socket.on(
        "ide-add",
        // extracts from serde_json Value type containing event's arguments into the extract type
        // Data
        async |socket: SocketRef, Data::<IdeAdd>(data), queue: State<QueueWrap>| {
            tracing::debug!(socket_id = %socket.id, ?data, "ide-add received");
            let add_attempt = queue.add_ide_operation(data).await;

            match add_attempt {
                // TODO: Move all of this stuff into the callbacks
                Ok(packet) => {
                    tracing::debug!(socket_id = %socket.id, ?packet, "ide-add succeeded, broadcasting network-add");
                    let _ = socket.within("network").emit("network-add", &packet).await;
                }

                Err(e) => {
                    tracing::warn!(socket_id = %socket.id, error = %e, "ide-add failed");
                    let _ = socket.within("local").emit("ide-add-failed", &e).await;
                }
            }
        },
    );

    socket.on(
        "remove",
        async |socket: SocketRef, Data::<IdeRemove>(data), queue: State<QueueWrap>| {
            tracing::debug!(socket_id = %socket.id, ?data, "remove received");
            let remove_attempt = queue.remove_ide_operation(data).await;

            match remove_attempt {
                Ok(packet) => {
                    tracing::debug!(socket_id = %socket.id, ?packet, "remove succeeded, broadcasting network-remove");
                    let _ = socket
                        .within("network")
                        .emit("network-remove", &packet)
                        .await;
                }

                Err(e) => {
                    tracing::warn!(socket_id = %socket.id, error = %e, "remove failed");
                    let _ = socket.within("local").emit("ide-remove-failed", &e).await;
                }
            }
        },
    )
}

fn network_add_callback(row: usize, col: usize) {
    tracing::debug!(row, col, "add applied");
}

fn network_remove_callback(row: usize, col: usize, is_line: bool) {
    tracing::debug!(row, col, is_line, "remove applied");
}

fn ide_remove_callback(op: NetworkRemove) {
    tracing::debug!(?op, "remove applied");
}

fn ide_add_callback(op: NetworkAdd) {
    tracing::debug!(?op, "add applied");
}
