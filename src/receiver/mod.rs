use log::error;
use num_format::{Locale, ToFormattedString};
use std::io;
use std::{io::Write, sync::mpsc::Receiver};

use crate::{Config, DupeMessage};

fn mark_group(files: &mut Vec<(&String, bool)>, purge: bool) {
    for file in files {
        file.1 = purge;
    }
}

fn process_input(buffer: &str, files: &mut Vec<(&String, bool)>) -> bool {
    let mut done = false;
    for choice in buffer.split(|c: char| c.is_whitespace() || c == ',') {
        let choice = choice.trim();
        if choice.is_empty() {
            continue;
        }
        match choice {
            "quit" => std::process::exit(0),
            "none" => {
                mark_group(files, false);
                done = true;
            }
            "all" => {
                mark_group(files, true);
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
    done
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
                done = process_input(&buffer, &mut files);
            }

            if done {
                break files;
            }
        };

        for (filename, preserve) in files {
            if !preserve {
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn mark_group_false() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        mark_group(&mut files, false);
        for (file, mark) in files {
            assert!(!mark, "{} should be unmarked", file);
        }
    }

    #[test]
    fn mark_group_true() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        mark_group(&mut files, true);
        for (file, mark) in files {
            assert!(mark, "{} should be marked", file);
        }
    }

    #[test]
    fn process_input_empty() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        let done = process_input("", &mut files);
        assert!(!done);
        for (file, mark) in files {
            assert!(!mark, "{} should be unmarked", file);
        }
    }

    #[test]
    fn process_input_all() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        let done = process_input("all", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(mark, "{} should be marked", file);
        }
    }

    #[test]
    fn process_input_none() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        let done = process_input("none", &mut files);
        assert!(done);
        for (file, mark) in files {
            assert!(!mark, "{} should be unmarked", file);
        }
    }

    #[test]
    fn process_input_single() {
        let file1 = "test1".to_string();
        let file2 = "test2".to_string();
        let file3 = "test3".to_string();
        let mut files = vec![(&file1, false), (&file2, false), (&file3, false)];
        let done = process_input("2", &mut files);
        assert!(done);
        for (file, mark) in files {
            if *file == file2 {
                assert!(mark, "{} should be marked", file);
            } else {
                assert!(!mark, "{} should be unmarked", file);
            }
        }
    }
}
