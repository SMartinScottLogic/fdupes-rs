extern crate chrono;

use clap::Parser;
use fdupes::receiver::DupeGroupReceiver;
use fdupes::{ExactGroupComparator, JsonGroupComparator};
use tracing::Level;
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

fn main() {
    let config = Config::parse();

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    let mut receiver = setup(rx, &config);
    let scanner = DupeScanner::new(
        tx,
        Arc::new(config.clone()),
        vec![
            Box::new(ExactGroupComparator::new()),
            Box::new(JsonGroupComparator::new()),
        ],
    );

    let receiver = thread::spawn(move || receiver.run());
    let scanner = thread::spawn(move || scanner.find_groups());

    receiver.join().unwrap().unwrap();
    scanner.join().unwrap();
}
