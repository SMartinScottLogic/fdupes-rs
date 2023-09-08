use std::{
    io::{self, BufRead, ErrorKind, Read},
    path::{Path, PathBuf},
};

use memcmp::Memcmp;
use tracing::{debug, error, trace};
use crate::scanner::group_comparator::GroupComparator;
use crate::DupeMessage;

const BLOCK_SIZE: usize = 1024;

#[derive(Debug)]
enum TriOption<T> {
    None,
    Definite(T),
    Possible(T),
}
#[derive(Debug)]
pub struct FdupesGroup<'a> {
    pub filenames: Vec<PathBuf>,
    pub size: u64,
    pub comparator: &'a dyn GroupComparator,
    partialcrc: TriOption<u16>,
    fullcrc: TriOption<u16>,
}

impl<'a> FdupesGroup<'a> {
    pub fn into_dupe_message(self, total: usize, id: usize) -> DupeMessage {
        (self.size, total, id, self.filenames.clone())
    }

    pub fn new(file: &Path, size: u64, comparator: &'a dyn GroupComparator) -> Self {
        let mut n = Self {
            filenames: Vec::default(),
            size,
            comparator,
            partialcrc: TriOption::None,
            fullcrc: TriOption::None,
        };
        n.add(file);
        n
    }

    pub fn add(&mut self, file: &Path) {
        self.filenames.push(file.to_owned());
    }

    pub fn partialcrc(&mut self) -> io::Result<u16> {
        if let TriOption::Definite(crc) = self.partialcrc {
            Ok(crc)
        } else {
            let filename = self
                .filenames
                .get(0)
                .ok_or_else(|| std::io::Error::new(ErrorKind::Other, "No files in group"))?;
            let mut f = self.comparator.open(filename.to_str().unwrap())?;
            let mut buffer = vec![0_u8; std::cmp::min(self.size, BLOCK_SIZE as u64) as usize];

            f.reader.read_exact(&mut buffer[..])?;
            let crc = crc::Crc::<u16>::new(&crc::CRC_16_USB).checksum(&buffer[..]);
            self.partialcrc = TriOption::Definite(crc);
            if self.size <= BLOCK_SIZE as u64 {
                self.fullcrc = TriOption::Definite(crc);
            }
            Ok(crc)
        }
    }

    pub fn fullcrc(&mut self) -> io::Result<u16> {
        if let TriOption::Definite(crc) = self.fullcrc {
            Ok(crc)
        } else {
            let filename = self
                .filenames
                .get(0)
                .ok_or_else(|| std::io::Error::new(ErrorKind::Other, "No files in group"))?;
            let mut f = self.comparator.open(filename.to_str().unwrap())?;
            let digest = crc::Crc::<u16>::new(&crc::CRC_16_IBM_SDLC);
            let mut digest = digest.digest();

            loop {
                let length = {
                    let buffer = f.reader.fill_buf()?;
                    digest.update(buffer);
                    buffer.len()
                };
                if length == 0 {
                    break;
                }
                f.reader.consume(length);
            }

            let crc = digest.finalize();
            self.fullcrc = TriOption::Definite(crc);
            Ok(crc)
        }
    }

    fn open(&self) -> io::Result<Box<dyn BufRead>> {
        let filename = match self.filenames.get(0) {
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "empty group")),
            Some(name) => name,
        };
        let filename = match filename.to_str() {
            None => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("Unrepresentable file: {:?}", filename))),
            Some(name) => name,
        };
        self.comparator.open(filename)
        .map(|v| {v.reader})
    }
}

impl<'a> PartialEq<Self> for FdupesGroup<'a> {
    fn eq(&self, other: &Self) -> bool {
        if self.comparator.name() != other.comparator.name() {
            return false;
        }
        let mut reader_a = match self.open() {
            Ok(reader) => reader,
            Err(e) => {
                debug!(error = debug(e), group = debug(self), "open reader a");
                return false
            },
        };
        let mut reader_b = match other.open() {
            Ok(reader) => reader,
            Err(e) => {
                debug!(error = debug(e), group = debug(self), "open reader b");
                return false
            }
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

impl Drop for FdupesGroup<'_> {
    fn drop(&mut self) {
        trace!(self = debug(self), "drop");
    }
}

#[cfg(test)]
mod tests {
    use crate::scanner::group_comparator::{ExactGroupComparator, GroupComparator};

    use super::FdupesGroup;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    lazy_static::lazy_static! {
    static ref COLLISION_FILENAME: &'static Path = Path::new("test_data/collision_scratch.txt");
    static ref TEST_DATA1: &'static Path = Path::new("test_data/file1.txt");
    static ref TEST_DATA2: &'static Path = Path::new("test_data/file2.txt");
    static ref COMPARATOR: Box<dyn GroupComparator> = {
        let comparator: Box<dyn GroupComparator> = Box::new(ExactGroupComparator::new());
        comparator
    };
    }

    fn test_group<'a>(files: &[&Path]) -> FdupesGroup<'a> {
        let size = files[0].metadata().unwrap().len();
        let mut group = FdupesGroup::new(files[0], size, COMPARATOR.as_ref());
        for file in files.iter().skip(1) {
            group.add(file);
        }
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

        generate_test_file(&TEST_DATA1, file_a, *collision.first().unwrap());
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
