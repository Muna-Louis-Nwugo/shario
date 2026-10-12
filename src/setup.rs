//! Startup helpers: the `--bench-internal` early-exit path and log setup.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub async fn setup_benchmark() -> bool {
    // `--bench-internal <trace.json> [label]` replays a trace directly
    // against SharQueue in-process, skipping the server entirely -- see
    // src/bench.rs. Checked before any server setup so it can't interfere
    // with the normal startup path below.
    let args: Vec<String> = std::env::args().collect();
    if let Some(idx) = args.iter().position(|a| a == "--bench-internal") {
        let trace_path = args
            .get(idx + 1)
            .expect("--bench-internal requires a trace path");
        let label = args
            .get(idx + 2)
            .map(String::as_str)
            .unwrap_or("bench-internal");
        if let Err(e) = crate::bench::run(std::path::Path::new(trace_path), label).await {
            eprintln!("bench-internal failed: {e}");
            std::process::exit(1);
        }
        return true;
    }
    return false;
}

pub fn setup_logging() {
    // logs go to both stdout and ./shar.<date>.log so a flood -- like a
    // runaway backlog -- can be reviewed in one file after the fact instead
    // of scrolling back through the terminal. Rotates daily and keeps only
    // the last 5 files so it doesn't grow forever across restarts.
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("shar")
        .filename_suffix("log")
        .max_log_files(5)
        .build(".")
        .expect("failed to set up log file appender");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // env-filter reads RUST_LOG (e.g. `RUST_LOG=shario=debug`); defaults to
    // "info" for this crate if RUST_LOG isn't set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("shario=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false), // no color codes cluttering the log file
        )
        .init();
}
