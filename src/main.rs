mod shar;
#[cfg(test)]
mod tests;
mod types;

use std::{path::PathBuf, sync::Arc};

use axum::routing::get;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

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
        let real_queue = SharQueue::new(dir_path, this_peer_id, add_callback, remove_callback)?;
        let mut guard = self.queue.write().await; // locks the *shared* RwLock every clone points at
        if guard.add_callback.is_none() {
            *guard = real_queue; // overwrites its contents, not the Arc itself
        }
        Ok(())
    }

    pub async fn add_ide_operation(&self, op: IdeAdd) -> Result<NetworkAdd, Error> {
        let mut queue = self.queue.write().await;
        queue.add_ide_operation(op)
    }

    pub async fn remove_ide_operation(&self, op: IdeRemove) -> Result<NetworkRemove, Error> {
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
    socket.on(
        "join",
        async |socket: SocketRef, Data::<Connect>(data), mut queue: State<QueueWrap>| {
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
                    eprintln!("failed to initialize queue: {e}");
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
            let add_attempt = queue.add_ide_operation(data).await;

            match add_attempt {
                Ok(packet) => {
                    let _ = socket.within("network").emit("network-add", &packet).await;
                }

                Err(e) => {
                    let _ = socket.within("local").emit("ide-add-failed", &e).await;
                }
            }
        },
    );

    socket.on(
        "remove",
        async |socket: SocketRef, Data::<IdeRemove>(data), queue: State<QueueWrap>| {
            let remove_attempt = queue.remove_ide_operation(data).await;

            match remove_attempt {
                Ok(packet) => {
                    let _ = socket
                        .within("network")
                        .emit("network-remove", &packet)
                        .await;
                }

                Err(e) => {
                    let _ = socket.within("local").emit("ide-remove-failed", &e).await;
                }
            }
        },
    )
}

fn network_add_callback(_row: usize, _col: usize) {
    println!("network_add_callback reached");
}

fn network_remove_callback(_row: usize, _col: usize, _is_line: bool) {
    println!("network_remove_callback reached");
}
