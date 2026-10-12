// Logging: `cargo run` prints to stdout at the default `shario=info` level
// (connects/joins/errors only). For per-keystroke `debug!` lines from the
// socket handlers and SharQueue, run `RUST_LOG=shario=debug cargo run`
// instead. This is server-side logging only -- separate from the "Shar"
// output channel on the VS Code extension side, which only shows what the
// client sent, not what the server did with it.

mod bench;
mod network_callbacks;
mod queue_wrap;
mod setup;
mod shar;
#[cfg(test)]
mod tests;
mod types;

use futures_lite::StreamExt;
use std::io;

use axum::routing::get;
use clap::Parser;
use iroh::{
    Endpoint, SecretKey,
    endpoint::presets::{self, Preset},
    protocol::Router,
};
use iroh_gossip::{
    TopicId,
    api::{
        ApiError,
        Event::{self, Received},
        GossipReceiver, GossipTopic,
    },
    net::Gossip,
};
use petname::petname;
use rand::{self, Rng};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef, State},
};

use crate::{
    queue_wrap::QueueWrap,
    types::{BroadcastMessage, Connect, IdeAdd, Remove, Ticket},
};

/// Defines available Shar Commands

#[tokio::main]
async fn main() -> io::Result<()> {
    // setup benckmark and logging
    if setup::setup_benchmark().await {
        return Ok(());
    }
    setup::setup_logging();

    // ==================================================================================
    // BELOW IS THE ACTUAL SERVER IMPLEMENTATION
    // ==================================================================================

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

    let _session_name = petname(3, "-").unwrap();

    // IROH
    tokio::spawn(async move {
        {
            // generates private key
            let secret_key = SecretKey::generate();

            // sets up the endpoint
            let endpoint = Endpoint::builder(presets::N0)
                .secret_key(secret_key)
                .bind()
                .await
                .expect("Unable to connect to endpoint");

            // set up for the gossip protocol
            let gossip = Gossip::builder().spawn(endpoint.clone());

            let router = Router::builder(endpoint.clone())
                .accept(iroh_gossip::ALPN, gossip.clone())
                .spawn();

            // creates a new gossip topic
            //TODO: figure out how to get the topic_id to come from petname
            let mut pet_name = petname(3, "-").unwrap();
            let mut name_buf: [u8; 32] = [0; 32];

            if pet_name.bytes().len() > name_buf.len() {
                pet_name.truncate(name_buf.len());
            } else if pet_name.bytes().len() < name_buf.len() {
                let repeated = "-".repeat(name_buf.len() - pet_name.len());

                pet_name.push_str(&repeated);
            }

            // create a new iterator for the petname
            name_buf.copy_from_slice(&pet_name.into_bytes());

            let id = TopicId::from_bytes(name_buf);
            let peer_ids = vec![];

            let (sender, receiver) = GossipTopic::split(
                gossip
                    .subscribe(id, peer_ids)
                    .await
                    .expect("couldn't set up gossip subscriber"),
            );

            tokio::spawn(iroh_gossip_subscriber(receiver));

            let ticket = Ticket {
                topic: id,
                peers: vec![],
            };
            println!("ticket to join us: {}", ticket);

            sender
                .broadcast("sup".into())
                .await
                .expect("broadcast failed");

            router.shutdown().await.expect("Unable to shutdown router");
        }
    });

    let listener = tokio::net::TcpListener::bind(&"127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn iroh_gossip_subscriber(mut receiver: GossipReceiver) -> Result<(), ApiError> {
    while let Some(event) = receiver.next().await {
        match event {
            Ok(event) => {
                match event {
                    Event::Received(_msg) => {
                        // TODO: Figure out what to do once the message has arrived
                    }

                    Event::NeighborDown(_msg) => {
                        // TODO: Figure out what to do once this event has been received
                    }

                    Event::NeighborUp(_msg) => {
                        // TODO: figure out what to do once this event has been recieved
                    }

                    Event::Lagged => {
                        // TODO: Figure out what to do when this event has been recieved
                    }
                }
            }

            Err(e) => {
                eprintln!("{}", format!("Failed to read directory entry: \n {e} \n"));
                // return Err(e);
            }
        }
    }

    Ok(())
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
        async |socket: SocketRef, mut queue: State<QueueWrap>, Data::<Connect>(data)| {
            tracing::info!(socket_id = %socket.id, local = data.local, path = ?data.path, "join");

            // leave all existing rooms
            let _ = socket.leave_all();
            let id = rand::rng().next_u32();

            // join the appropriate room on the connect message
            if data.local {
                // creates a new "queue"
                // right now, this is just a stub
                if let Err(e) = queue
                    .new(data.path, id, socket.clone())
                    .await
                {
                    tracing::error!(socket_id = %socket.id, error = %e, "failed to initialize queue");
                }
                socket.join("local");

                // tells the joining IDE the real (id, peer) behind every
                // character it didn't type itself -- see extension.js's
                // "initial-state" handler
                for (file_path, lines) in queue.identities().await {
                    let _ = socket
                        .within("local").emit(
                            "initial-state",
                            &serde_json::json!({ "file_path": file_path, "lines": lines }),
                        )
                        .await;
                }
            } else {
                socket.join("network");
            }
        },
    );

    socket.on(
        "ide-add",
        // extracts from serde_json Value type containing event's arguments into the extract type
        // Data
        async |socket: SocketRef, queue: State<QueueWrap>, Data::<IdeAdd>(data)| {
            tracing::debug!(socket_id = %socket.id, ?data, "ide-add received");
            let add_attempt = queue.add_ide_operation(data).await;

            match add_attempt {
                // TODO: Move all of this stuff into the callbacks
                Ok(_) => {}

                Err(e) => {
                    tracing::warn!(socket_id = %socket.id, error = %e, "ide-add failed");
                    let _ = socket.within("local").emit("ide-add-failed", &e).await;
                }
            }
        },
    );

    socket.on(
        "remove",
        async |socket: SocketRef, queue: State<QueueWrap>, Data::<Remove>(data)| {
            tracing::debug!(socket_id = %socket.id, ?data, "remove received");
            let _ = queue.remove_ide_operation(data).await;

            tracing::debug!(socket_id = %socket.id, "remove received ");
        },
    )
}
