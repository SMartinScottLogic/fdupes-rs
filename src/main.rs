extern crate log;
extern crate chrono;
extern crate env_logger;
extern crate serde;

use chrono::Local;
use env_logger::{Builder, Env};
use std::env;
use std::io::prelude::*;

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use num_format::{Locale, ToFormattedString};

use fdupes::DupeMessage;
use fdupes::DupeScanner;

fn handle_group(size: u64, filenames: Vec<String>) {
    if filenames.len() > 1 {
        if size == 1 {
            println!("{} byte each:", size.to_formatted_string(&Locale::en_GB));
        } else {
            println!("{} bytes each:", size.to_formatted_string(&Locale::en_GB));
        }
        for (id, filename) in filenames.iter().enumerate() {
            println!("[{}] {} (W)", id + 1, filename);
        }
        println!("Preserve files [1 - {}, all, none, quit]", filenames.len());

        let id = 0;
        if let Some(filename) = filenames.get(id) {
        }
        println!();
    }
}

fn main() {
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
    // TODO cmd-line args

    let sourceroot = env::args_os().skip(1).collect::<Vec<_>>();
    let recursive = true;
    let skip_empty = true;

    let (tx, rx): (Sender<DupeMessage>, Receiver<DupeMessage>) = mpsc::channel();

    let finder = thread::spawn(move || {
        let scanner = DupeScanner::new(tx, sourceroot, recursive, skip_empty);
        scanner.find_groups();
    });

    loop {
        let group = rx.recv();
        match group {
            Ok(DupeMessage::End) => break,
            Ok(DupeMessage::Group(size, group)) => handle_group(size, group),
            Err(e) => {
                println!("error: {}", e);
                break;
            }
        }
    }

    finder.join();
}
