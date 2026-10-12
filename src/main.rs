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
use std::{io, str::FromStr};
use tokio::sync::mpsc::Sender;

use axum::routing::get;
use clap::{Parser, Subcommand};
use iroh::{
    Endpoint, RelayMode, SecretKey,
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
    types::{BroadcastMessage, Connect, IdeAdd, NetworkAdd, NetworkOp, Remove, Ticket},
};

/// Defines available Shar Commands
#[derive(Parser, Debug)]
struct Args {
    //TODO: Check if this is really needed
    /// disable relay completely
    no_relay: bool,

    #[clap(subcommand)]
    commmand: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Creates a new shar session
    Create { name: Option<String> },

    /// Participates an existing shar session
    Participate { name: String },
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // setup benckmark and logging
    if setup::setup_benchmark().await {
        return Ok(());
    }
    setup::setup_logging();

    // ==================================================================================
    // BELOW IS THE CLAP PARSER
    // ==================================================================================
    let args = Args::parse();
    let session_name;

    let (topic, peers) = match &args.commmand {
        Commands::Create { name } => {
            if let Some(shar_name) = name {
                session_name = shar_name.clone();
            } else {
                session_name = petname::petname(3, "-").unwrap();
            }
            (TopicId::from_bytes(rand::random()), vec![])
        }

        Commands::Participate { name } => {
            let Ticket { topic, peers } =
                Ticket::from_str(name).expect("provided name doesn't deserialize");
            (topic, peers)
        }
    };

    // ==================================================================================
    // BELOW IS THE ACTUAL SERVER IMPLEMENTATION
    // ==================================================================================

    let relay_mode = RelayMode::Default;

    // the queue that everyone shares
    let shared_queue = QueueWrap::default();
    let shared_queue_local = shared_queue.clone();

    let (layer, io) = SocketIo::builder()
        // provides the state that is the queue and tree to the server
        .build_layer();

    // communication channel between callbacks and iroh gossip protocol
    let (tx, rx) = tokio::sync::mpsc::channel(300);
    // giving connect handler to the "/" namespace
    io.ns("/", |socket: SocketRef| {
        on_connect(tx, shared_queue_local, socket)
    });

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

            let (sender, receiver) = GossipTopic::split(
                gossip
                    .subscribe(topic, peers)
                    .await
                    .expect("couldn't set up gossip subscriber"),
            );

            tokio::spawn(iroh_gossip_subscriber(receiver));

            // TODO: USE tx/rx with the transmitter in the network callbacks to get here, put it in
            // a loop

            while let received = rx.recv().await {
                match received {
                    Some(op) => {
                        sender
                            .broadcast(axum::body::Bytes::from(op.into()[..]))
                            .await;
                    }

                    None => {}
                }
            }

            router.shutdown().await.expect("Unable to shutdown router");
        }
    });

    // Sets up the IDE listener
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

async fn on_connect(producer: Sender<NetworkOp>, queue: QueueWrap, socket: SocketRef) {
    tracing::info!(socket_id = %socket.id, "socket connected");

    // create clones for the listeners
    let queue_clone = queue.clone();
    let producer_clone = producer.clone();
    socket.on("join", async move |socket: SocketRef, Data::<Connect>(data)| {
        tracing::info!(socket_id = %socket.id, local = data.local, path = ?data.path, "join");
        let mut join_queue = queue_clone.clone();
        let join_producer = producer_clone.clone();

        // leave all existing rooms
        let _ = socket.leave_all();
        let id = rand::rng().next_u32();

        // join the appropriate room on the connect message
        if data.local {
            // creates a new "queue"
            // right now, this is just a stub
            if let Err(e) = join_queue
                .clone()
                .new(data.path, id, socket.clone(), join_producer)
                .await
            {
                tracing::error!(socket_id = %socket.id, error = %e, "failed to initialize queue");
            }
            socket.join("local");

            // tells the joining IDE the real (id, peer) behind every
            // character it didn't type itself -- see extension.js's
            // "initial-state" handler
            for (file_path, lines) in join_queue.identities().await {
                let _ = socket
                    .within("local")
                    .emit(
                        "initial-state",
                        &serde_json::json!({ "file_path": file_path, "lines": lines }),
                    )
                    .await;
            }
        } else {
            socket.join("network");
        }
    });

    // another clone
    let queue_clone2 = queue.clone();
    socket.on(
        "ide-add",
        // extracts from serde_json Value type containing event's arguments into the extract type
        // Data
        async move |socket: SocketRef, Data::<IdeAdd>(data)| {
            tracing::debug!(socket_id = %socket.id, ?data, "ide-add received");
            let add_attempt = queue_clone2.add_ide_operation(data).await;

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
