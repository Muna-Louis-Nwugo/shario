mod messages;
mod shar;

use std::sync::mpsc;
use std::thread;

use shar::error;

use axum::{Router, routing::get};
use clap::{Parser, Subcommand};
use pollster::block_on;
use socketioxide::{SocketIo, extract::SocketRef};

use crate::messages::InputCommand;
use crate::messages::InputMessage;
use crate::shar::core::buffer::SharBuffer;
use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::SharDirectory;

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
        directory_path: String,
    },
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
        .layer(layer);

    // start a TCP listener on localhost:3000
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

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
                        let buffer = buff;
                    });
                }

                Err(e) => {
                    eprintln!("Something went wrong during initialization: {}", e);
                }
            }
        }
    }

    // practice InputMessage
    let input_message = InputMessage {
        command: InputCommand::AddCRDT,
        arguments: vec![String::from("Message got through")],
    };
}

//TODO: Make a SharOutputer
/// The shar's input manager
struct SharInputer {
    thread: thread::JoinHandle<()>,
    transmitter: mpsc::Sender<SharInput>,
}

struct SharInput {
    val: String,
}

impl SharInputer {
    pub fn new(que: SharQueue, dir: SharDirectory) -> Self {
        let (tx, rx) = mpsc::channel();
        let input = thread::spawn(move || {
            let queue = que;
            let tree = dir;

            let received: SharInput = rx.recv().unwrap();
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

/// Web socket connection handler
async fn on_connect(socket: SocketRef) {
    println!("socket connected {}", socket.id);
}

// supporting functions
async fn initialize_shar(
    session_id: u32,
    directory_path: String,
) -> Result<(SharDirectory, SharQueue, SharBuffer), error::Error> {
    let _ = session_id;
    let _ = directory_path;

    let dir = SharDirectory::new(directory_path)?;

    let buff = SharBuffer::new().await?;

    let queue = SharQueue::new();

    Ok((dir, queue, buff))
}
