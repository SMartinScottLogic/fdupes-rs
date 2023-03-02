extern crate chrono;
extern crate env_logger;
extern crate log;

use chrono::Local;
use clap::Parser;
use env_logger::{Builder, Env};
use fdupes::receiver::DupeGroupReceiver;
use std::io::prelude::*;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use fdupes::receiver::*;
use fdupes::{Config, DupeMessage, DupeScanner};

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

fn setup(rx: Receiver<DupeMessage>, config: &Config) -> Box<dyn DupeGroupReceiver> {
    if config.classic_mode {
        setup_logger();
        Box::new(BasicReceiver::new(rx, config.to_owned()))
    } else {
        tui_logger::init_logger(log::LevelFilter::Trace).unwrap();
        tui_logger::set_default_level(log::LevelFilter::Info);
        tui_logger::set_level_for_target("fdupes::scanner", log::LevelFilter::Debug);
        tui_logger::set_level_for_target("fdupes::receiver::ui_receiver", log::LevelFilter::Debug);
        Box::new(UIReceiver::new(rx, config.to_owned()))
    }
}

fn main() {
    let config = Config::parse();

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    //let mut ui = Arc::new(Mutex::new(fdupes::ui::UI::new()));

    let mut receiver = setup(rx, &config);

    let scanner = DupeScanner::new(tx, Arc::new(config.clone()));

    let receiver = thread::spawn(move || receiver.run());
    let scanner = thread::spawn(move || scanner.find_groups());

    receiver.join().unwrap().unwrap();
    scanner.join().unwrap();
}
