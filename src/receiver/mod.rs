use num_format::{Locale, ToFormattedString};
use std::io;
use std::path::{PathBuf};
use std::{io::Write, sync::mpsc::Receiver};

use crate::{Config, DupeMessage};

type DupeGroup<'a> = Vec<(&'a PathBuf, Mark)>;

#[derive(PartialEq, Copy, Clone)]
enum Mark {
    Purge,
    Keep
}

fn mark_group(files: &mut DupeGroup, purge: Mark) {
    for file in files {
        file.1 = purge;
    }
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

fn handle_group(size: u64, filenames: Vec<PathBuf>, config: &Config) {
    if filenames.len() > 1 {
        for (id, filename) in filenames.iter().enumerate() {
            println!("[{}] {:?} (W)", id + 1, filename);
        }
        let files = loop {
            let mut files = filenames.iter().map(|f| (f, Mark::Purge)).collect::<DupeGroup>();
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
                done = process_input(&buffer, &mut files);
            }

            if done {
                break files;
            }
        };

        for (filename, mark) in files {
            if Mark::Purge == mark {
                if config.trash {
                    trash::delete(filename).unwrap();
                } else {
                    std::fs::remove_file(filename).unwrap();
                }
            }
        }
    }
}

pub fn receiver(rx: Receiver<DupeMessage>, config: Config) {
    while let Ok((size, filenames)) = rx.recv() {
        handle_group(size, filenames, &config);
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
    fn mark_group_false() {
        let mut files = vec![(&*FILE1, Mark::Keep), (&*FILE2, Mark::Keep), (&*FILE3, Mark::Keep)];
        mark_group(&mut files, Mark::Purge);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{:?} should be purged", file);
        }
    }

    #[test]
    fn mark_group_true() {
        let mut files = vec![(&*FILE1, Mark::Purge), (&*FILE2, Mark::Purge), (&*FILE3, Mark::Purge)];
        mark_group(&mut files, Mark::Keep);
        for (file, mark) in files {
            assert!(mark == Mark::Keep, "{:?} should be retained", file);
        }
    }

    #[test]
    fn process_input_empty() {
        let mut files = vec![(&*FILE1, Mark::Purge), (&*FILE2, Mark::Purge), (&*FILE3, Mark::Purge)];
        let done = process_input("", &mut files);
        assert!(!done);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{:?} should be purged", file);
        }
    }

    #[test]
    fn process_input_all() {
        let mut files = vec![(&*FILE1, Mark::Purge), (&*FILE2, Mark::Purge), (&*FILE3, Mark::Purge)];
        let done = process_input("all", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(mark == Mark::Keep, "{:?} should be retained", file);
        }
    }

    #[test]
    fn process_input_none() {
        let mut files = vec![(&*FILE1, Mark::Purge), (&*FILE2, Mark::Purge), (&*FILE3, Mark::Purge)];
        let done = process_input("none", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{:?} should be purged", file);
        }
    }

    #[test]
    fn process_input_single() {
        let mut files = vec![(&*FILE1, Mark::Purge), (&*FILE2, Mark::Purge), (&*FILE3, Mark::Purge)];
        let done = process_input("2", &mut files);
        assert!(done);
        for (file, mark) in files {
            if *file == *FILE2 {
                assert!(mark == Mark::Keep, "{:?} should be retained", file);
            } else {
                assert!(mark == Mark::Purge, "{:?} should be purged", file);
            }
        }
    }
}
