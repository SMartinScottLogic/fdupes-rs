extern crate chrono;
extern crate env_logger;
extern crate log;

use chrono::Local;
use clap::StructOpt;
use env_logger::{Builder, Env};
use std::io::prelude::*;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
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

    // setup_logger();
    tui_logger::init_logger(log::LevelFilter::Trace).unwrap();

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    let scanner = DupeScanner::new(tx, Arc::new(config.clone()));
    let mut ui = Arc::new(Mutex::new(fdupes::ui::UI::new()));

    let scanner = DupeScanner::new(tx, config.clone());
    let receiver = fdupes::ui::UI::new(rx, config);

    let receiver = thread::spawn(move || receiver.run());
    let scanner = thread::spawn(move || scanner.find_groups());

    ui.lock().unwrap().test_tui();

    receiver.join().unwrap();
    scanner.join().unwrap();
}
