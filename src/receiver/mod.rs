
use log::error;
use num_format::{Locale, ToFormattedString};
use std::io;
use std::{io::Write, sync::mpsc::Receiver};

use crate::{Config, DupeMessage};

fn mark_group(files: &mut Vec<(&String, bool)>, active: bool) {
    for file in files {
        file.1 = active;
    }
}

fn handle_group(size: u64, filenames: Vec<String>, config: &Config) {
    if filenames.len() > 1 {
        for (id, filename) in filenames.iter().enumerate() {
            println!("[{}] {} (W)", id + 1, filename);
        }
        let files = loop {
            let mut files = filenames.iter().map(|f| (f, false)).collect::<Vec<_>>();
            print!("Preserve files [1 - {}, all, none, quit]", filenames.len());
            if config.show_sizes {
                if size == 1 {
                    print!(" ({} byte each)", size.to_formatted_string(&Locale::en_GB));
                } else {
                    print!(" ({} bytes each)", size.to_formatted_string(&Locale::en_GB));
                }
            }
            print!(": ");
            io::stdout().flush().unwrap();
            let mut done = false;
            
            let mut buffer = String::new();
            if io::stdin().read_line(&mut buffer).is_ok() {
                for choice in buffer.split(|c| c == ' ' || c == '\n' || c == ',') {
                    match choice {
                        "quit" => std::process::exit(0),
                        "all" => {
                            mark_group(&mut files, true);
                            done = true;
                        }
                        "none" => {
                            mark_group(&mut files, false);
                            done = true;
                        }
                        val => {
                            if let Ok(val) = val.parse::<usize>() {
                                if let Some(file) = files.get_mut(val - 1) {
                                    file.1 = true;
                                    done = true;
                                }
                            }
                        }
                    }
                }
            }
            
            if done {
                break files;
            }
        };

        for (filename, purge) in files {
            if purge {
                println!("rm {filename}");
            }
        }
    }
}

pub fn receiver(rx: Receiver<DupeMessage>, config: Config) {
    loop {
        let group = rx.recv().unwrap_or_else(|e| {
            error!("error: {e}");
            DupeMessage::End
        });
        if let DupeMessage::Group(size, filenames) = group {
            handle_group(size, filenames, &config);
        } else {
            break;
        }
    }
}
