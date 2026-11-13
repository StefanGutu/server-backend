//! Logs go to the terminal and to log/server-info.log.
//! The file is emptied at every start, then every line is appended while the server runs.

use std::{fs::File, sync::Mutex};

use tracing_subscriber::{
    EnvFilter, field::MakeExt, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

const LOG_DIR: &str = "log";
const LOG_FILE: &str = "log/server-info.log";

pub fn init() {
    std::fs::create_dir_all(LOG_DIR).expect("cannot create the log folder");
    // File::create empties an existing file.
    let file = File::create(LOG_FILE).expect("cannot create the log file");

    // Log level comes from RUST_LOG, default is "info".
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=debug".into());

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
