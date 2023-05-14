use crate::{Config, DupeMessage};
use num_format::{Locale, ToFormattedString};
use std::path::PathBuf;
use std::{io, io::Write, sync::mpsc::Receiver};
use tracing::debug;

use super::{mark_group, DupeGroup, DupeGroupReceiver, Mark};

pub struct BasicReceiver {
    rx: Receiver<DupeMessage>,
    config: Config,
}

impl DupeGroupReceiver for BasicReceiver {
    fn run(&mut self) -> Result<(), std::io::Error> {
        while let Ok((size, total, id, filenames)) = self.rx.recv() {
            debug!("{} {:?}", size, filenames);
            Self::handle_group(size, id, total, filenames, &self.config);
        }
        /*
        loop {
            match self.rx.try_recv() {
                Ok((size, filenames)) => {
                }
                Err(TryRecvError::Empty) => log::debug!("empty"),
                Err(TryRecvError::Disconnected) => break,
            }
        }
        log::info!("END");
        */
        Ok(())
    }
}

impl BasicReceiver {
    pub fn new(rx: Receiver<DupeMessage>, config: Config) -> Self {
        Self { rx, config }
    }

    fn process_input(buffer: &str, files: &mut DupeGroup) -> bool {
        let mut done = false;
        for choice in buffer.split(|c: char| c.is_whitespace() || c == ',') {
            let choice = choice.trim();
            if choice.is_empty() {
                continue;
            }
            match choice {
                "quit" => std::process::exit(0),
                "none" => {
                    mark_group(files, Mark::Purge);
                    done = true;
                }
                "all" => {
                    mark_group(files, Mark::Keep);
                    done = true;
                }
                val => {
                    if let Ok(val) = val.parse::<usize>() {
                        if let Some(file) = files.get_mut(val - 1) {
                            file.1 = Mark::Keep;
                            done = true;
                        }
                    }
                }
            }
        }
        done
    }

    fn handle_group(size: u64, id: usize, total: usize, filenames: Vec<PathBuf>, config: &Config) {
        if filenames.len() > 1 {
            for (id, filename) in filenames.iter().enumerate() {
                println!("[{}] {:?} (W)", id + 1, filename);
            }
            let files = loop {
                let mut files = filenames
                    .iter()
                    .map(|f| (f, Mark::Purge))
                    .collect::<DupeGroup>();
                print!(
                    "({}/{}) Preserve files [1 - {}, all, none, quit]",
                    id,
                    total,
                    filenames.len()
                );
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
                    done = Self::process_input(&buffer, &mut files);
                }

                if done {
                    break files;
                }
            };

            for (filename, mark) in files {
                if Mark::Purge == mark {
                    if config.trash {
                        if let Err(e) = trash::delete(filename) {
                            eprintln!("Failed to put {filename:?} in trash: {e}");
                        }
                    } else if let Err(e) = std::fs::remove_file(filename) {
                        eprintln!("Failed to delete {filename:?}: {e}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    lazy_static::lazy_static! {
    static ref FILE1: PathBuf = PathBuf::from("test1");
    static ref FILE2: PathBuf = PathBuf::from("test2");
    static ref FILE3: PathBuf = PathBuf::from("test3");
    }

    #[test]
    fn process_input_empty() {
        let mut files = vec![
            (&*FILE1, Mark::Purge),
            (&*FILE2, Mark::Purge),
            (&*FILE3, Mark::Purge),
        ];
        let done = BasicReceiver::process_input("", &mut files);
        assert!(!done);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{file:?} should be purged");
        }
    }

    #[test]
    fn process_input_all() {
        let mut files = vec![
            (&*FILE1, Mark::Purge),
            (&*FILE2, Mark::Purge),
            (&*FILE3, Mark::Purge),
        ];
        let done = BasicReceiver::process_input("all", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(mark == Mark::Keep, "{file:?} should be retained");
        }
    }

    #[test]
    fn process_input_none() {
        let mut files = vec![
            (&*FILE1, Mark::Purge),
            (&*FILE2, Mark::Purge),
            (&*FILE3, Mark::Purge),
        ];
        let done = BasicReceiver::process_input("none", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{file:?} should be purged");
        }
    }

    #[test]
    fn process_input_single() {
        let mut files = vec![
            (&*FILE1, Mark::Purge),
            (&*FILE2, Mark::Purge),
            (&*FILE3, Mark::Purge),
        ];
        let done = BasicReceiver::process_input("2", &mut files);
        assert!(done);
        for (file, mark) in files {
            if *file == *FILE2 {
                assert!(mark == Mark::Keep, "{file:?} should be retained");
            } else {
                assert!(mark == Mark::Purge, "{file:?} should be purged");
            }
        }
    }
}
