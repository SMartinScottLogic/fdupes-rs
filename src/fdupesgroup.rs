use std::io;
use std::io::prelude::*;

use crc::{crc16, Hasher16};
use memcmp::Memcmp;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::io::ErrorKind;

const BLOCK_SIZE: usize = 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FdupesGroup {
    pub filenames: Vec<String>,
    pub size: u64,
    partialcrc: Option<u16>,
    fullcrc: Option<u16>,
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

    pub fn is_empty(&self) -> bool {
        self.filenames.is_empty()
    }

    pub fn len(&self) -> usize {
        self.filenames.len()
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
