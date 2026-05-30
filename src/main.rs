mod shar;

use std::thread;

use shar::error;

use clap::{Parser, Subcommand};

use crate::shar::core::buffer::SharBuffer;
use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::SharDirectory;

use pollster::block_on;

/// Shar CLI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Shar {
    #[command(subcommand)]
    command: Command,
}

/// Defines available Shar Commands
#[derive(Subcommand, Debug)]
#[command(arg_required_else_help(true))]
enum Command {
    Init {
        session_id: u32,
        directory_path: String,
    },
}

#[tokio::main]
async fn main() {
    let cmd = Shar::parse();

    let input;
    let output;
    match cmd.command {
        Command::Init {
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
