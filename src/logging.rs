//! Logs go to the terminal and to log/server-info.log.
//! The file is emptied at every start, then every line is appended while the server runs. `LOG_DIR` changes the folder.

use std::{fs::File, sync::Mutex};

use tracing_subscriber::{
    EnvFilter, field::MakeExt, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

const LOG_DIR: &str = "log";

/// Where the log file is: `log/server-info.log`, or the folder of `LOG_DIR` when that is set. A second server started
/// only to try something (a test server) sets it, so that it does not empty the log of the server that is really running.
fn log_file() -> (String, String) {
    let dir = std::env::var("LOG_DIR")
        .ok()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| LOG_DIR.to_string());
    let file = format!("{dir}/server-info.log");
    (dir, file)
}

pub fn init() {
    let (dir, path) = log_file();
    std::fs::create_dir_all(&dir).expect("cannot create the log folder");
    // File::create empties an existing file.
    let file = File::create(&path).expect("cannot create the log file");

    // Log level comes from RUST_LOG, default is "info".
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=debug".into());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer()) // terminal
        .with(
            // File, no colors. `map_fmt_fields` gives it its own field formatter: without it,
            // the file would reuse the terminal's formatted fields, colors included.
            fmt::layer()
                .with_ansi(false)
                .map_fmt_fields(|fields| fields.delimited(" "))
                .with_writer(Mutex::new(file)),
        )
        .init();
}
