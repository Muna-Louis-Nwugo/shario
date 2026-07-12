mod messages;
mod shar;

use crate::shar::core::tree::Entry;
use crate::shar::prelude::*;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use axum::{Router, routing::get};
use clap::{Parser, Subcommand};
use pollster::block_on;
use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef},
};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use crate::messages::InputMessage;
use crate::messages::{MessageIn, MessageOut};
use crate::shar::core::buffer::SharBuffer;
use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::SharDirectory;
use crate::shar::error;
use crate::shar::prelude;

/// Shar CLI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Shar {
    #[command(subcommand)]
    command: SharCommand,
}

/// Defines available Shar Commands
#[derive(Subcommand, Debug)]
#[command(arg_required_else_help(true))]
enum SharCommand {
    Init {
        session_id: u32,
        directory_path: PathBuf,
    },
}

/// Web socket connection handler
async fn on_connect(socket: SocketRef) {
    println!("socket connected {}", socket.id);

    // joins rooms (after receiving a join event from the client)

    socket.on("join", async |socket: SocketRef, Data::<String>(room)| {
        println!("received join {:?}", room);
        // removes client from any rooms. Makes sure that every client is only ever a  part of one
        // room
        socket.leave_all();
        socket.join(room);
    });

    // handles incoming messages from rooms
    socket.on(
        "input",
        async |socket: SocketRef, Data::<MessageIn>(data)| {
            let message: MessageOut;

            if data.room == String::from("ide") {
                message = MessageOut {
                    success: true,
                    message: String::from("Message Received"),
                }
            } else if data.room == String::from("network") {
                message = MessageOut {
                    success: true,
                    message: String::from("Message Received"),
                }
            } else {
                message = MessageOut {
                    success: false,
                    message: String::from("Room not recognized"),
                }
            }

            println!("received message {:?} from room {:?}", data.val, data.room);
            let _ = socket.within(data.room).emit("message", &message);
        },
    );
}

#[tokio::main]
async fn main() {
    println!("main started");
    let cmd = Shar::parse();

    // enable websockets
    let (layer, io) = SocketIo::new_layer();

    // pass handler into "/" namespace of the SocketIO instance
    io.ns("/", on_connect);

    let app = Router::new()
        .route(
            "/",
            get(|| async { "please select input '/in' or output '/out'" }),
        )
        .route("/in", get(SharInputer::handle))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(layer),
        );

    // start a TCP listener on localhost:1324
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1324")
        .await
        .unwrap();
    println!("Server Started on localhost:1324");

    // start server
    axum::serve(listener, app).await.unwrap();

    let _input: SharInputer;
    // TODO: write the message passing system for the output
    let _output: tokio::task::JoinHandle<()>;

    match cmd.command {
        SharCommand::Init {
            session_id,
            directory_path,
        } => {
            let shar_init = block_on(initialize_shar(session_id, directory_path));

            match shar_init {
                Ok((dir, que, buff)) => {
                    // launch threads
                    _input = SharInputer::new(que, dir);

                    _output = tokio::spawn(async move {
                        let _buffer = buff;
                    });
                }

                Err(e) => {
                    eprintln!("Something went wrong during initialization: {}", e);
                }
            }
        }
    }
}

// TODO: Make a SharOutputer
/// The shar's input manager
struct SharInputer {
    thread: thread::JoinHandle<()>,
    transmitter: mpsc::Sender<InputMessage>,
}

impl SharInputer {
    pub fn new(que: SharQueue, dir: SharDirectory) -> Self {
        let (tx, rx) = mpsc::channel();
        let input = thread::spawn(move || {
            let _queue = que;
            let _tree = dir;

            let _received: InputMessage = rx.recv().unwrap();
        });

        SharInputer {
            thread: input,
            transmitter: tx,
        }
    }

    pub async fn handle() -> &'static str {
        "Initiating input"
    }
}

// supporting functions
async fn initialize_shar(
    session_id: u32,
    directory_path: PathBuf,
) -> Result<(SharDirectory, SharQueue, SharBuffer)> {
    let _ = session_id;
    let _ = directory_path;

    let mut dummy_ids = Vec::new();
    dummy_ids.push(0);
    let dir = SharDirectory::new(directory_path.clone(), dummy_ids.clone(), 0)?;

    let buff = SharBuffer::new().await?;

    let queue = SharQueue::new(directory_path.clone(), dummy_ids.clone(), 0)?;

    Ok((dir, queue, buff))
}
