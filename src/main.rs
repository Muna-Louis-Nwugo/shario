mod come_up_with_name;
mod shar;

use shar::error;

use clap::{Parser, Subcommand};

use crate::shar::core::buffer::SharBuffer;
use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::SharDirectory;

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

fn main() {
    let cmd = Shar::parse();

    match cmd.command {
        Command::Init {
            session_id,
            directory_path,
        } => {
            initialize_shar(session_id, directory_path);
        }
    }
}

// supporting functions
fn initialize_shar<'a>(
    session_id: u32,
    directory_path: String,
) -> Result<(&'a SharDirectory, &'a SharQueue, &'a SharBuffer), error::Error> {
    let _ = session_id;
    let _ = directory_path;

    let dir = SharDirectory::new(directory_path)?;

    let buff = SharBuffer::new();

    let queue = SharQueue::new();

    Ok((&dir, &queue, &buff))
}
