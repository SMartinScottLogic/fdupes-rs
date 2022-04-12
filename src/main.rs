extern crate chrono;
extern crate env_logger;
extern crate log;

use chrono::Local;
use clap::StructOpt;
use env_logger::{Builder, Env};
use std::io::prelude::*;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use fdupes::{receiver::receiver, Config, DupeMessage, DupeScanner};

fn setup_logger() {
    let env = Env::default().filter_or("RUST_LOG", "info");
    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
}

fn main() {
    let config = Config::parse();

    setup_logger();

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    let scanner = DupeScanner::new(tx, Arc::new(config.clone()));

    let receiver = thread::spawn(move || receiver(rx, config));
    let scanner = thread::spawn(move || scanner.find_groups());

    receiver.join().unwrap();
    scanner.join().unwrap();
}
