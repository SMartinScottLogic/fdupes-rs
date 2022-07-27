use std::{
    fs::File,
    io::{self, BufRead, BufReader, ErrorKind, Read},
    path::{Path, PathBuf},
};

use crc::{crc16, Hasher16};
use memcmp::Memcmp;

use crate::DupeMessage;

const BLOCK_SIZE: usize = 1024;

#[derive(Debug, Default)]
pub struct FdupesGroup {
    pub filenames: Vec<PathBuf>,
    pub size: u64,
    partialcrc: Option<u16>,
    fullcrc: Option<u16>,
}

impl FdupesGroup {
    pub fn into_dupe_message(self, total: usize, id: usize) -> DupeMessage {
        (self.size, total, id, self.filenames)
    }

    pub fn new(file: &Path, size: u64) -> Self {
        let mut n = Self {
            size,
            ..Default::default()
        };
        n.add(file);
        n
    }

    pub fn add(&mut self, file: &Path) {
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

#[cfg(test)]
mod tests {
    use super::FdupesGroup;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    lazy_static::lazy_static! {
    static ref COLLISION_FILENAME: &'static Path = Path::new("test_data/collision_scratch.txt");
    static ref TEST_DATA1: &'static Path = Path::new("test_data/file1.txt");
    static ref TEST_DATA2: &'static Path = Path::new("test_data/file2.txt");
    }

    fn test_group(files: &[&Path]) -> FdupesGroup {
        let mut group = FdupesGroup::default();
        for file in files {
            group.add(file);
        }
        group.size = fs::File::open(files[0]).unwrap().metadata().unwrap().len();
        group
    }

    #[test]
    fn partialcrc_diff() {
        let mut group1 = test_group(&[&TEST_DATA1]);
        let mut group2 = test_group(&[&TEST_DATA2]);

        assert_eq!(group1.partialcrc().unwrap(), group2.partialcrc().unwrap());
    }

    #[test]
    fn fullcrc_diff() {
        let mut group1 = test_group(&[&TEST_DATA1]);
        let mut group2 = test_group(&[&TEST_DATA2]);

        assert_ne!(group1.fullcrc().unwrap(), group2.fullcrc().unwrap());
    }

    fn generate_test_file(source: &Path, target: &Path, trail: u64) {
        fs::copy(source, target).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(target)
            .unwrap();
        write!(&mut file, "{:08}", trail).unwrap();
    }

    #[test]
    fn collision() {
        let mut fullcrcs = std::collections::HashMap::new();
        for i in 0..=u64::MAX {
            generate_test_file(&TEST_DATA1, &COLLISION_FILENAME, i);
            let crc = test_group(&[&COLLISION_FILENAME]).fullcrc().unwrap();
            fullcrcs.entry(crc).or_insert_with(Vec::new).push(i);
            if fullcrcs.get(&crc).unwrap().len() > 1 {
                break;
            }
        }
        fs::remove_file(*COLLISION_FILENAME).unwrap();
        let (_crc, collision) = fullcrcs.iter().find(|(_crc, e)| e.len() > 1).unwrap();

        let file_a = Path::new("test_data\\collision_file_a");
        let file_b = Path::new("test_data\\collision_file_b");

        generate_test_file(&TEST_DATA1, file_a, *collision.get(0).unwrap());
        generate_test_file(&TEST_DATA1, file_b, *collision.get(1).unwrap());

        let mut group_a = test_group(&[file_a]);
        let mut group_b = test_group(&[file_b]);

        assert_eq!(group_a.partialcrc().unwrap(), group_b.partialcrc().unwrap());
        assert_eq!(group_a.fullcrc().unwrap(), group_b.fullcrc().unwrap());
        assert_eq!(group_a.size, group_b.size);
        assert_ne!(group_a, group_b);

        fs::remove_file(file_a).unwrap();
        fs::remove_file(file_b).unwrap();
    }
}
