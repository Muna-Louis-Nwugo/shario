mod shar;
#[cfg(test)]
mod tests;
mod types;

use std::sync::mpsc;
use std::thread;

use shar::error;

use axum::routing::get;
use clap::{Parser, Subcommand};
use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef},
};

use crate::shar::core::buffer::SharBuffer;
use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::SharDirectory;
use serde_json::Value;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use pollster::block_on;

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

    let input;
    let (input_tx, input_rx) = mpsc::channel();
    let output;

    // set up web server
    let (layer, io) = SocketIo::builder().build_layer();

    // giving connect handler to the "/" namespace
    io.ns("/", on_ide_connect);

    // .route() sets up the HTTP handler
    let app = axum::Router::<()>::new()
        .route("/", get(async || println!("Hello World!")))
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

    match cmd.command {
        SharCommand::Init {
            session_id,
            directory_path,
        } => {
            let shar_init = block_on(initialize_shar(session_id, directory_path));

            match shar_init {
                Ok((dir, que, buff)) => {
                    // launch threads
                    input = thread::spawn(move || {
                        let queue = que;
                        let tree = dir;

                        let mut num_received = 0;

                        while num_received < 5 {
                            let received = input_rx.recv().unwrap();
                            println!("{}", received);
                            num_received += 1;
                        }
                    });

                    output = tokio::spawn(async move {
                        let buffer = buff;
                    });
                }

                Err(e) => {
                    eprintln!("Something went wrong during initialization: {}", e);
                }
            }
        }
    }

    for i in 0..10 {
        if i % 2 == 0 {
            input_tx.send("hi").unwrap();
            println!("message sent");
        }
    }
}

async fn on_ide_connect(socket: SocketRef) {
    socket.on(
        "add",
        // extracts from serde_json Value type containing event's arguments into the extract type
        // Data
        async |socket: SocketRef, Data::<Value>(data)| {},
    );
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
