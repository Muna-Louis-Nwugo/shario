pub mod come_up_with_name;
pub mod shar;

use clap::{Parser, Subcommand};

use shar::core;

/// Shar CLI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Shar {
    #[command(subcommand)]
    commands: Command,
}

/// Defines available Shar Commands
#[derive(Subcommand, Debug)]
#[command(arg_required_else_help(true))]
enum Command {
    Init,
}

fn main() {
    let queue = core::queue::SharQueue::new();
}
