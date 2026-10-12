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
mod state_request;
#[cfg(test)]
mod tests;
mod types;

use futures_lite::StreamExt;
use redis::{self, Commands, RedisError};
use std::{cmp, io, ops::Index, str::FromStr};
use tokio::sync::mpsc::Sender;

use axum::routing::get;
use clap::{Parser, Subcommand};
use iroh::{Endpoint, PublicKey, RelayMode, SecretKey, endpoint::presets, protocol::Router};
use iroh_gossip::{
    TopicId,
    api::{ApiError, Event, GossipReceiver, GossipTopic},
    net::Gossip,
    proto::topic,
};
use rand::{self, Rng, random_range, seq::IndexedRandom};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef, State},
};

use crate::{
    queue_wrap::QueueWrap,
    state_request::request_state,
    types::{Connect, IdeAdd, NetworkOp, Remove, Ticket},
};

/// Defines available Shar SharCommands
#[derive(Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    commmand: Option<SharCommands>,

    /// Port the IDE-facing socket.io server listens on. Lets more than one
    /// `shario` instance run on the same machine at once (for local
    /// discovery/gossip testing) without port conflicts.
    #[clap(long, default_value_t = 3000)]
    port: u16,

    /// Skips Redis/iroh entirely and just starts the IDE-facing socket.io
    /// server -- for bench/trace-runner.js, which only exercises the local
    /// IDE<->server path and has no use for peer discovery/networking. Not
    /// to be confused with `--bench-internal`, which is a different flag
    /// entirely, handled before clap ever parses anything (see `setup.rs`).
    #[clap(long, default_value_t = false)]
    socket_benchmark: bool,
}

#[derive(Debug, Subcommand)]
enum SharCommands {
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

    if args.socket_benchmark {
        // socket-mode benchmarking (bench/trace-runner.js) only exercises
        // the local IDE<->server path -- no Redis, no iroh, no discovery.
        let shared_queue = QueueWrap::default();
        let (layer, io) = SocketIo::builder().build_layer();
        let (tx, mut rx) = tokio::sync::mpsc::channel(300);
        // nothing to broadcast to in this mode -- just drain so
        // ide_add_callback/ide_remove_callback's sends never back up and block
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
        io.ns("/", |socket: SocketRef| on_connect(tx, shared_queue, socket));

        let app = axum::Router::<()>::new()
            .route("/", get(async || println!("Connecting...")))
            .layer(
                ServiceBuilder::new()
                    .layer(CorsLayer::permissive())
                    .layer(layer),
            );

        let listener = tokio::net::TcpListener::bind(&format!("127.0.0.1:{}", args.port))
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
        return Ok(());
    }

    // connect to upstash redis db
    let redis_url = env!("REDIS_URL");
    let client = redis::Client::open(redis_url).expect("Unable to connect to database");
    let mut con = client
        .get_connection()
        .expect("Unable to connect to database");

    // THESE NEXT TWO ARE FOR IROH, I'M INITIALIZING THEM HERE SO I CAN USE THEM IN DISCOVERY
    // generates private key
    let secret_key = SecretKey::generate();

    // sets up the endpoint
    let endpoint = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Default)
        .secret_key(secret_key)
        .bind()
        .await
        .expect("Unable to connect to endpoint");

    let session_name;

    let (topic, mut peers) = match args
        .commmand
        .as_ref()
        .expect("a subcommand (create/participate) is required unless --socket-benchmark is set")
    {
        SharCommands::Create { name } => {
            if let Some(shar_name) = name {
                session_name = shar_name.clone();
            } else {
                session_name = petname::petname(3, "-").unwrap();
                // prints the session name to be parsed by the client if neceessary
                println!("{}", session_name);
            }
            let topic_id = TopicId::from_bytes(rand::random());
            let peers_vec = vec![];
            let ticket = Ticket {
                topic: topic_id,
                peers: peers_vec.clone(),
            };

            // if this name is already in use,bail out
            let existing: Option<Vec<u8>> = con.get(&session_name).ok();
            if existing.is_some() {
                return Err(io::Error::other("Session name already exists"));
            }
            // creates this name and its keys in the redis db
            // the key will expire automatically after 24 hours
            let _: () = con.set_ex(&session_name, ticket.to_bytes(), 86400).unwrap();
            (topic_id, peers_vec)
        }

        SharCommands::Participate { name } => {
            session_name = name.clone();
            let ticket_bytes: Vec<u8> = con
                .get(&session_name)
                .expect("This shar name doesn't exist");

            let Ticket { topic, peers } =
                Ticket::from_bytes(&ticket_bytes).expect("Unable to generate tickets");

            (topic, peers)
        }
    };

    // ==================================================================================
    // BELOW IS THE ACTUAL SERVER IMPLEMENTATION
    // ==================================================================================

    // the queue that everyone shares
    let shared_queue = QueueWrap::default();
    let shared_queue_local = shared_queue.clone();
    let shared_queue_discovery = shared_queue.clone();
    // THIS IS A BAD NAME it stays local, but it's given to the part of the program that listens to
    // the netowrk
    let shared_queue_network = shared_queue.clone();

    // let discovery happen
    let mut random_generator = rand::rng();
    let mut discovery_peers = peers.clone();
    let mut num_discovery_peers = cmp::min(3, discovery_peers.len());

    while num_discovery_peers > 0 {
        //selects a random peer
        let random_peer = discovery_peers.choose(&mut random_generator).unwrap();

        // hydration
        request_state(&endpoint, (*random_peer).into(), &shared_queue_discovery).await;

        let _ = discovery_peers.remove(
            discovery_peers
                .iter()
                .position(|&x| x == (*random_peer))
                .unwrap(),
        );

        num_discovery_peers -= 1;
    }

    let (layer, io) = SocketIo::builder()
        // provides the state that is the queue and tree to the server
        .build_layer();

    // communication channel between callbacks and iroh gossip protocol
    let (tx, mut rx) = tokio::sync::mpsc::channel(300);
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
            // handles state requests
            let state_request_handler =
                state_request::StateRequestHandler::new(shared_queue_discovery);

            // set up for the gossip protocol
            let gossip = Gossip::builder().spawn(endpoint.clone());

            let router = Router::builder(endpoint.clone())
                .accept(iroh_gossip::ALPN, gossip.clone())
                .accept(
                    state_request::STATE_REQUEST_ALPN,
                    state_request_handler.clone(),
                )
                .spawn();

            // creates a new gossip topic

            let (sender, receiver) = GossipTopic::split(
                gossip
                    .subscribe(topic, peers.clone())
                    .await
                    .expect("couldn't set up gossip subscriber"),
            );

            // add this shar to the db
            peers.push(endpoint.id());
            let this_ticket = Ticket {
                topic: topic,
                peers: peers.clone(),
            };

            let _: () = con
                .set_ex(&session_name, this_ticket.to_bytes(), 86400)
                .unwrap();

            // spin up the subscriber to the gossip protocol
            tokio::spawn(iroh_gossip_subscriber(
                receiver,
                endpoint.clone(),
                shared_queue_network,
            ));

            while let Some(received) = rx.recv().await {
                let _ = sender
                    .broadcast(axum::body::Bytes::from(received.serialize()))
                    .await;
            }

            router.shutdown().await.expect("Unable to shutdown router");
        }
    });

    // Sets up the IDE listener
    let listener = tokio::net::TcpListener::bind(&format!("127.0.0.1:{}", args.port))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn iroh_gossip_subscriber(
    mut receiver: GossipReceiver,
    endpoint: Endpoint,
    queue: QueueWrap,
) -> Result<(), ApiError> {
    while let Some(event) = receiver.next().await {
        match event {
            Ok(event) => match event {
                Event::Received(msg) => {
                    let data = NetworkOp::deserialize(&msg.content.to_vec())
                        .expect("Deserialization is infallible");

                    match data {
                        NetworkOp::ADD(op) => {
                            queue.add_network_operation(op).await;
                        }

                        NetworkOp::REMOVE(op) => {
                            queue.remove_network_operation(op).await;
                        }
                    }
                }

                Event::NeighborUp(key) => {
                    // hydrate this shar's state with whatever the new peer has
                    request_state(&endpoint, key.into(), &queue).await;
                }
                _ => {}
            },

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
