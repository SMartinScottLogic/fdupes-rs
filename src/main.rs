#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;

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

    const BLOCK_SIZE: usize = 1024;

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

        fn gen_partial_crc(filename: &str) -> io::Result<u16> {
            let mut f = File::open(filename)?;
            let mut buffer = [0; BLOCK_SIZE];

            let size = f.read(&mut buffer)?;
            Ok(crc16::checksum_usb(&buffer[..size]))
        }

        fn gen_full_crc(filename: &str) -> io::Result<u16> {
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
                self.partialcrc = match Self::gen_partial_crc(&self.filename) {
                    Ok(crc) => (true, true, crc),
                    _ => (true, false, 0_u16),
                };
            }

            if !self.fullcrc.0 {
                self.fullcrc = match Self::gen_full_crc(&self.filename) {
                    Ok(crc) => (true, true, crc),
                    _ => (true, false, 0_u16),
                };
            }
        }

        pub fn partial_crc(&self) -> io::Result<u16> {
            if self.partialcrc.1 {
                Ok(self.partialcrc.2)
            } else {
                Err(io::Error::new(
                    std::io::ErrorKind::Other,
                    "crc generation error",
                ))
            }
        }

        pub fn full_crc(&self) -> io::Result<u16> {
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

    impl Eq for FdupesFile {}

    impl PartialEq for FdupesFile {
        fn eq(&self, other: &Self) -> bool {
            let mut reader_a = match File::open(self.filename.clone()) {
                Ok(f) => BufReader::new(f),
                _ => return false,
            };
            let mut reader_b = match File::open(other.filename.clone()) {
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
                if buf_a[..read_bytes_a].to_vec() != buf_b[..read_bytes_b].to_vec() {
                    return false;
                }
            }
        }
    }
}

fn find_files(
    sourceroot: std::ffi::OsString,
    recursive: bool,
    skip_empty: bool,
) -> Vec<Vec<data::FdupesFile>> {
    info!(
        "find all files in {:?} (recursive: {}, skip_empty: {})",
        sourceroot, recursive, skip_empty
    );

    let walk = WalkDir::new(sourceroot);
    let walk = if recursive {
        walk.into_iter()
    } else {
        walk.max_depth(1).into_iter()
    };

    walk.map(std::result::Result::unwrap)
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
                    .push(data::FdupesFile::new(entry.1, size));
            }
            acc
        })
        .values()
        .cloned()
        .collect()
}

fn remove_uniq(groups: Vec<Vec<data::FdupesFile>>) -> Vec<Vec<data::FdupesFile>> {
    groups.into_iter().filter(|value| value.len() > 1).collect()
}

fn matches(file: &mut data::FdupesFile, group: &[data::FdupesFile]) -> io::Result<bool> {
    let group_file = group.get(0);
    let group_file = match group_file {
        Some(f) => f,
        _ => return Err(io::Error::new(std::io::ErrorKind::Other, "empty group")),
    };

    file.calculate_metrics();

    {
        let filecrc = file.partial_crc()?;
        let groupcrc = group_file.partial_crc()?;
        if filecrc != groupcrc {
            return Ok(false);
        }
    }

    {
        let filecrc = file.full_crc()?;
        let groupcrc = group_file.full_crc()?;
        if filecrc != groupcrc {
            return Ok(false);
        }
    }
    Ok(group_file == file)
}

fn build_matches(groups: Vec<Vec<data::FdupesFile>>) -> Vec<Vec<data::FdupesFile>> {
    groups
        .into_iter()
        .flat_map(|group| {
            debug!("build_matches {:?}", group);
            let mut result: Vec<Vec<data::FdupesFile>> = Vec::new();
            for mut file in group {
                if let Some(existing) = result
                    .iter_mut()
                    .find(|existing| matches(&mut file, existing).unwrap_or(false))
                {
                    existing.push(file);
                } else {
                    let mut newgroup = Vec::new();
                    newgroup.push(file);
                    result.push(newgroup);
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

    let sourceroot = env::args_os().nth(1).unwrap();
    let recursive = true;
    let skip_empty = true;

    let groups = find_files(sourceroot, recursive, skip_empty);
    info!("{} total groups (by size)", groups.len());
    // Remove files with unique size
    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by size)", groups.len());
    let groups = build_matches(groups);

    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by exact content)", groups.len());

    for bucket in groups {
        debug!(
            "{:#?}",
            bucket
                .into_iter()
                .map(|file| file.filename)
                .collect::<Vec<_>>()
        );
    }
}
