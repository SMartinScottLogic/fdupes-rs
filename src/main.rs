extern crate chrono;

use clap::Parser;
use fdupes::receiver::DupeGroupReceiver;
use fdupes::{ExactGroupComparator, JsonGroupComparator, DbMessage};
use tracing::{Level, debug, info};
use tracing_subscriber::fmt::format::FmtSpan;

use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::{env, thread};

use fdupes::receiver::*;
use fdupes::{Config, DupeMessage, DupeScanner};

fn setup_logger() {
    // install global collector configured based on RUST_LOG env var.
    let level =
        env::var("RUST_LOG").map_or(Level::INFO, |v| Level::from_str(&v).unwrap_or(Level::INFO));
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::ACTIVE)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_max_level(level)
        .init();
}

fn setup(rx: Receiver<DupeMessage>, config: &Config) -> Box<dyn DupeGroupReceiver> {
    if config.classic_mode {
        setup_logger();
        Box::new(BasicReceiver::new(rx, config.to_owned()))
    } else {
        panic!()
    }
}

struct DbReceiver {
    rx: Receiver<DbMessage>,
}
impl DbReceiver {
    fn new(rx: Receiver<DbMessage>, config: &Config) -> Self {
        Self { rx }
    }

    fn run(self) {
        while let Ok(DbMessage { filename, size, partialcrc, fullcrc }) = self.rx.recv() {
            info!("To write to DB: {} {:?}", size, filename);
        }
    }
}

fn main() {
    let config = Config::parse();

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    let (file_tx, file_rx) = mpsc::channel();

    let db_connection = sqlite::Connection::open("cache.sqlite").unwrap();

    let mut receiver = setup(rx, &config);
    let scanner = DupeScanner::new(
        tx,
        db_connection,
        Arc::new(config.clone()),
        vec![
            Box::new(ExactGroupComparator::new()),
            Box::new(JsonGroupComparator::new()),
        ],
    );

    let db_receiver = DbReceiver::new(file_rx, &config);

    let db_receiver = thread::spawn(move || db_receiver.run());
    let receiver = thread::spawn(move || receiver.run());
    let scanner = thread::spawn(move || scanner.find_groups());

    receiver.join().unwrap().unwrap();
    scanner.join().unwrap();
    db_receiver.join().unwrap();
}
