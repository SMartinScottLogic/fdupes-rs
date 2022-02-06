use std::io;
use std::{collections::BTreeMap, sync::mpsc::Sender};

use log::{debug, info, trace};
use std::io::prelude::*;
use walkdir::WalkDir;

use crc::{crc16, Hasher16};
use memcmp::Memcmp;
use std::fs::File;
use std::io::BufReader;
use std::io::ErrorKind;

use crate::{DupeMessage, Config};

const BLOCK_SIZE: usize = 1024;

pub struct DupeScanner {
    tx: Sender<DupeMessage>,
    config: Config,
}

impl DupeScanner {
    pub fn new(
        tx: Sender<DupeMessage>,
        config: Config,
    ) -> Self {
        Self {
            tx,
            config,
        }
    }
} 

impl DupeScanner {
    pub fn find_groups(&self) {
        let groups = self.find_files();
        info!("{} total groups (by size)", groups.len());
        self.build_matches(groups);
        self.tx.send(DupeMessage::End).unwrap();
    }

    fn send(&self, groups: Vec<FdupesGroup>) {
        for bucket in groups {
            self.tx.send(bucket.into()).unwrap();
        }
    }

    fn find_files(&self) -> BTreeMap<u64, Vec<String>> {
        info!(
            "find all files in {:?} (non-recursive: {}, include_empty: {})",
            self.config.root, self.config.non_recursive, self.config.include_empty
        );
        let all_groups = self
            .config.root
            .iter()
            .flat_map(|root| {
                info!("scanning {:?}...", root);
                let walk = WalkDir::new(root);
                if self.config.non_recursive {
                    walk.max_depth(1).into_iter()
                } else {
                    walk.into_iter()
                }
            })
            .map(std::result::Result::unwrap)
            .filter(|entry| !entry.path().is_symlink())
            .filter(|entry| entry.path().is_file())
            .map(|entry| {
                (
                    entry.metadata().unwrap().len(),
                    entry.path().to_str().unwrap_or("").to_string(),
                )
            })
            .fold(BTreeMap::new(), |mut acc, entry| {
                let size = entry.0;
                if size > 0 || self.config.include_empty {
                    acc.entry(size).or_insert_with(Vec::new).push(entry.1);
                }
                acc
            });
        info!("{} non-unique groups (by size)", all_groups.len());
        all_groups
            .into_iter()
            .filter(|(_size, files)| files.len() > 1)
            .collect()
    }

    fn build_matches(&self, groups: BTreeMap<u64, Vec<String>>) {
        for (size, filenames) in groups.iter().rev() {
            debug!("build matches {}: {} files", size, filenames.len());
            let mut result = Vec::new();
            for filename in filenames {
                self.update_matches(filename, *size, &mut result);
            }
            debug!(
                " => {:?}",
                result.iter().map(|r| r.filenames.len()).collect::<Vec<_>>()
            );
            self.send(result);
        }
    }

    fn update_matches(&self, filename: &str, size: u64, result: &mut Vec<FdupesGroup>) {
        let mut file = FdupesGroup::new(filename, size);
        for r in result.iter_mut() {
            if let Ok(true) = self.matches(&mut file, r) {
                r.add(filename);
                return;
            }
        }
        result.push(file);
    }

    fn matches(&self, file: &mut FdupesGroup, group: &mut FdupesGroup) -> io::Result<bool> {
        trace!("Compared {file:?} vs {group:?}");
        let filecrc = file.partialcrc()?;
        let groupcrc = group.partialcrc()?;
        if filecrc != groupcrc {
            return Ok(false);
        }
        let filecrc = file.fullcrc()?;
        let groupcrc = group.fullcrc()?;
        if filecrc != groupcrc {
            return Ok(false);
        }
        Ok(file == group)
    }
}
#[derive(Debug, Default)]
pub struct FdupesGroup {
    pub filenames: Vec<String>,
    pub size: u64,
    partialcrc: Option<u16>,
    fullcrc: Option<u16>,
}

impl std::convert::Into<DupeMessage> for FdupesGroup {
    fn into(self) -> DupeMessage {
        DupeMessage::Group(self.size, self.filenames)
    }
}

impl FdupesGroup {
    pub fn new(file: &str, size: u64) -> Self {
        let mut n = Self {
            size,
            ..Default::default()
        };
        n.add(file);
        n
    }

    pub fn add(&mut self, file: &str) {
        self.filenames.push(file.to_owned());
    }

    pub fn partialcrc(&mut self) -> io::Result<u16> {
        if let Some(crc) = self.partialcrc {
            Ok(crc)
        } else {
            let filename = self
                .filenames
                .get(0)
                .ok_or_else(|| std::io::Error::new(ErrorKind::Other, "No files in group"))?;
            let mut f = File::open(filename)?;
            let mut buffer = vec![0_u8; std::cmp::min(self.size, BLOCK_SIZE as u64) as usize];

            f.read_exact(&mut buffer[..])?;
            let crc = crc16::checksum_usb(&buffer[..]);
            self.partialcrc = Some(crc);
            if self.size <= BLOCK_SIZE as u64 {
                self.fullcrc = Some(crc);
            }
            Ok(crc)
        }
    }

    pub fn fullcrc(&mut self) -> io::Result<u16> {
        if let Some(crc) = self.fullcrc {
            Ok(crc)
        } else {
            let filename = self
                .filenames
                .get(0)
                .ok_or_else(|| std::io::Error::new(ErrorKind::Other, "No files in group"))?;
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

            let crc = digest.sum16();
            self.fullcrc = Some(crc);
            Ok(crc)
        }
    }
}

impl PartialEq<FdupesGroup> for FdupesGroup {
    fn eq(&self, other: &FdupesGroup) -> bool {
        let mut reader_a = match File::open(&self.filenames.get(0).unwrap()) {
            Ok(f) => BufReader::new(f),
            _ => return false,
        };
        let mut reader_b = match File::open(other.filenames.get(0).unwrap()) {
            Ok(f) => BufReader::new(f),
            _ => return false,
        };

        loop {
            let buf_a = match reader_a.fill_buf() {
                Ok(buf) => buf,
                _ => return false,
            };
            let buf_b = match reader_b.fill_buf() {
                Ok(buf) => buf,
                _ => return false,
            };

            let length_a = buf_a.len();
            let length_b = buf_b.len();

            if length_a != length_b {
                return false;
            }

            if length_a == 0 {
                return true;
            }

            if !buf_a.memcmp(buf_b) {
                return false;
            }

            reader_a.consume(length_a);
            reader_b.consume(length_b);
        }
    }
}
