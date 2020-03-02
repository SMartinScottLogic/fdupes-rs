#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;
extern crate serde;

use chrono::Local;
use env_logger::{Builder, Env};
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::io::prelude::*;
use walkdir::WalkDir;

mod data {
    use crc::{crc16, Hasher16};
    use std::fs::File;
    use std::io;
    use std::io::prelude::*;
    use std::io::BufReader;
    use serde::{Deserialize, Serialize};
    use serde_json::Result;
    use memcmp::Memcmp;

    const BLOCK_SIZE: usize = 1024;

    use std::sync::atomic::{AtomicUsize, Ordering};
    pub static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Serialize, Deserialize)]
    pub struct FdupesGroup {
        pub filenames: Vec<String>,
        pub size: u64,
        partialcrc: u16,
        fullcrc: u16
    }

    impl FdupesGroup {
        pub fn add(&mut self, file: FdupesFile) {
            self.filenames.push(file.filename);
        }

        pub fn len(&self) -> usize {
            self.filenames.len()
        }

        pub fn partialcrc(&self) -> u16 {
            self.partialcrc
        }

        pub fn fullcrc(&self) -> u16 {
            self.fullcrc
        }
    }

    impl From<FdupesFile> for FdupesGroup {
        fn from(file: FdupesFile) -> Self {
            FdupesGroup { filenames: [file.filename].to_vec(), size: file.size, partialcrc: file.partialcrc.2, fullcrc: file.fullcrc.2 }
        }
    }

    #[derive(Clone, Debug)]
    pub struct FdupesFile {
        pub filename: String,
        partialcrc: (bool, bool, u16),
        fullcrc: (bool, bool, u16),
        pub size: u64,
    }

    impl FdupesFile {
        pub const fn new(filename: String, size: u64) -> Self {
            Self {
                filename,
                partialcrc: (false, false, 0_u16),
                fullcrc: (false, false, 0_u16),
                size,
            }
        }

        fn gen_partialcrc(filename: &str) -> io::Result<u16> {
            let mut f = File::open(filename)?;
            let mut buffer = [0; BLOCK_SIZE];

            let size = f.read(&mut buffer)?;
            Ok(crc16::checksum_usb(&buffer[..size]))
        }

        fn gen_fullcrc(filename: &str) -> io::Result<u16> {
            let f = File::open(filename)?;
            let mut reader = BufReader::new(f);
            let mut digest = crc16::Digest::new(crc16::X25);

            loop {
                let length = {
                    let buffer = reader.fill_buf()?;
                    digest.write(buffer);
                    buffer.len()
                };
                if length == 0 {
                    break;
                }
                reader.consume(length);
            }

            Ok(digest.sum16())
        }

        pub fn calculate_metrics(&mut self) {
            if !self.partialcrc.0 {
                self.partialcrc = match Self::gen_partialcrc(&self.filename) {
                    Ok(crc) => (true, true, crc),
                    _ => (true, false, 0_u16),
                };
            }

            /*
            if !self.fullcrc.0 {
                self.fullcrc = match Self::gen_fullcrc(&self.filename) {
                    Ok(crc) => (true, true, crc),
                    _ => (true, false, 0_u16),
                };
            }
            */
        }

        pub fn partialcrc(&self) -> io::Result<u16> {
            if self.partialcrc.1 {
                Ok(self.partialcrc.2)
            } else {
                Err(io::Error::new(
                    std::io::ErrorKind::Other,
                    "crc generation error",
                ))
            }
        }

        pub fn fullcrc(&self) -> io::Result<u16> {
            if self.fullcrc.1 {
                Ok(self.fullcrc.2)
            } else {
                Err(io::Error::new(
                    std::io::ErrorKind::Other,
                    "crc generation error",
                ))
            }
        }
    }

    impl PartialEq<FdupesGroup> for FdupesFile {
        fn eq(&self, other: &FdupesGroup) -> bool {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);

            let mut reader_a = match File::open(&self.filename) {
                Ok(f) => BufReader::new(f),
                _ => return false,
            };
            let mut reader_b = match File::open(other.filenames.get(0).unwrap()) {
                Ok(f) => BufReader::new(f),
                _ => return false,
            };

            loop {
                let mut buf_a = [0_u8; BLOCK_SIZE];
                let mut buf_b = [0_u8; BLOCK_SIZE];
                let read_bytes_a = match reader_a.read(&mut buf_a) {
                    Ok(size) => size,
                    _ => return false,
                };
                let read_bytes_b = match reader_b.read(&mut buf_b) {
                    Ok(size) => size,
                    _ => return false,
                };
                if read_bytes_a != read_bytes_b {
                    return false;
                }
                if read_bytes_a == 0 {
                    return true;
                }
                if !buf_a.memcmp(&buf_b) {
                    return false;
                }
            }
        }
    }
}

fn find_files(
    sourceroot: Vec<std::ffi::OsString>,
    recursive: bool,
    skip_empty: bool,
) -> BTreeMap<u64, Vec<String>> {
    info!(
        "find all files in {:?} (recursive: {}, skip_empty: {})",
        sourceroot, recursive, skip_empty
    );
    let all_groups = sourceroot.iter().flat_map(|root| {
    let walk = WalkDir::new(root);
    let walk = if recursive {
        walk.into_iter()
    } else {
        walk.max_depth(1).into_iter()
    };
    walk
    })
    .map(std::result::Result::unwrap)
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            (
                entry.metadata().unwrap().len(),
                entry.path().to_str().unwrap().to_string(),
            )
        })
        .fold(BTreeMap::new(), |mut acc, entry| {
            let size = entry.0;
            if size > 0 || !skip_empty {
                acc.entry(size)
                    .or_insert_with(Vec::new)
                    .push(entry.1);
            }
            acc
        });
    info!("{} non-unique groups (by size)", all_groups.len());
    all_groups.into_iter().filter(|(_size, files)| files.len() > 1).collect()
}

fn remove_uniq(groups: Vec<data::FdupesGroup>) -> Vec<data::FdupesGroup> {
    groups.into_iter().filter(|value| value.len() > 1).collect()
}

fn matches(file: &mut data::FdupesFile, group: &data::FdupesGroup) -> io::Result<bool> {
    file.calculate_metrics();

    {
        let filecrc = file.partialcrc()?;
        let groupcrc = group.partialcrc();
        if filecrc != groupcrc {
            return Ok(false);
        }
    }

    /*
    {
        let filecrc = file.fullcrc()?;
        let groupcrc = group.fullcrc();
        if filecrc != groupcrc {
            return Ok(false);
        }
    }
    */
    Ok(file == group)
}

fn build_matches(groups: BTreeMap<u64, Vec<String>>) -> Vec<data::FdupesGroup> {
    groups
        .into_iter()
        .flat_map(|(size, files)| {
            debug!("build_matches {}: {:?}", size, files);
            let mut result: Vec<data::FdupesGroup> = Vec::new();
            for file in files {
                let mut file = data::FdupesFile::new(file, size);
                file.calculate_metrics();
                if let Some(existing) = result
                    .iter_mut()
                    .find(|existing| matches(&mut file, existing).unwrap_or(false))
                {
                    existing.add(file);
                } else {
                    result.push(data::FdupesGroup::from(file));
                }
            }
            debug!("result: {:?}", result);
            result
        })
        .collect()
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

    let groups = find_files(sourceroot, recursive, skip_empty);
    info!("{} total groups (by size)", groups.len());
    let groups = build_matches(groups);

    let groups = remove_uniq(groups);

    for bucket in &groups {
        println!("{}", serde_json::to_string(&bucket).unwrap_or(String::from("")));
    }
    info!("{} non-unique groups (by exact content)", groups.len());
    println!("eq called: {}", data::CALL_COUNT.load(std::sync::atomic::Ordering::SeqCst));
}
