#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;
use crc::{crc16, Hasher16};
use std::io;
use std::io::BufReader;
use std::io::prelude::*;
use std::fs::File;
use std::io::SeekFrom;
use env_logger::{Builder,Env};
use log::LevelFilter;
use chrono::Local;

const BLOCK_SIZE: usize = 1024;

#[derive(Clone, Debug)]
struct FdupesFile {
    filename: String,
    size: u64
}

fn find_files(sourceroot: std::ffi::OsString, recursive: bool, skip_empty: bool) -> Vec<Vec<FdupesFile>> {
    info!("find all files in {:?} (recursive: {}, skip_empty: {})", sourceroot, recursive, skip_empty);

    let walk = WalkDir::new(sourceroot);
    let walk = match recursive {
        true => walk.into_iter(),
        false => walk.max_depth(1).into_iter()
    };

    walk
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_str().unwrap().to_string()))
        .fold(BTreeMap::new(), |mut acc, entry| {
            let size = entry.0;
            if size > 0 || !skip_empty {
                acc.entry(size).or_insert(Vec::new()).push(FdupesFile {filename: entry.1, size});
            }
            acc
        }).values().cloned().collect()
}

fn remove_uniq(groups: Vec<Vec<FdupesFile>>) -> Vec<Vec<FdupesFile>> {
    groups.into_iter().filter(|value| value.len() > 1).collect()
}

fn gen_partial_crc(file:FdupesFile) -> io::Result<(FdupesFile, u16)> {
    let mut f = File::open(&file.filename).unwrap();
    let mut buffer = [0; BLOCK_SIZE];

    f.read(&mut buffer)?;
    Ok((file, crc16::checksum_usb(&buffer)))
}

fn gen_partial_crcs(groups: Vec<Vec<FdupesFile>>) -> Vec<Vec<FdupesFile>> {
    groups.into_iter().flat_map(|group| {
        group.into_iter()
        .map(|file| gen_partial_crc(file))
        .filter_map(Result::ok)
        .fold(BTreeMap::new(), |mut acc, (filename, crc)| {
            acc.entry(crc as u64).or_insert(Vec::new()).push(filename);
            acc
        })
    }).map(|(_, group)| group).collect()
}

fn gen_full_crc(file:FdupesFile) -> io::Result<(FdupesFile, u16)> {
    let f = File::open(&file.filename)?;
    let mut reader = BufReader::new(f);
    let mut digest = crc16::Digest::new(crc16::X25);

    loop {
        let length = {
            let buffer = reader.fill_buf()?;
            digest.write(&buffer);
            buffer.len()
        };
        if length == 0 {
            break;
        }
        reader.consume(length);
    }

    Ok((file, digest.sum16()))
}

fn gen_full_crcs(groups: Vec<Vec<FdupesFile>>) -> Vec<Vec<FdupesFile>> {
    groups.into_iter().flat_map(|group| {
        group.into_iter()
        .map(|file| gen_full_crc(file))
        .filter_map(Result::ok)
        .fold(BTreeMap::new(), |mut acc, (file, crc)| {
            acc.entry(crc as u64).or_insert(Vec::new()).push(file);
            acc
        })
    }).map(|(_, group)| group).collect()
}

fn open_files(size: u64, group: Vec<String>) -> BTreeMap<u64, Vec<(String, std::fs::File)>> {
    group.into_iter()
    .fold(BTreeMap::new(), |mut acc, filename| {
        let nom = filename.clone();
        match File::open(filename) {
            Ok(file) => acc.entry(size).or_insert(Vec::new()).push((nom, file)),
            Err(_) => {}
        }
        acc
    })
}

fn read_file_block(file: &mut std::fs::File, block_start: usize) -> io::Result<(usize, [u8; BLOCK_SIZE])> {
        file.seek(SeekFrom::Start(block_start as u64))?;
        let mut buffer = [0; BLOCK_SIZE];
        let read = file.read(&mut buffer)?;
        Ok((read, buffer))
}

fn read_block(size: u64, group: Vec<(String, std::fs::File)>, block_start: usize) -> BTreeMap<u64, Vec<(String, std::fs::File, usize, [u8; BLOCK_SIZE])>> {
    group.into_iter()
    .fold(BTreeMap::new(), |mut acc, (filename, mut file)| {
        match read_file_block(&mut file, block_start) {
            Ok((read, buffer)) => acc.entry(size).or_insert(Vec::new()).push((filename, file, read, buffer)),
            Err(_) => {}
        };
        acc
    })
}

fn partition_group_by_bytes(group: Vec<FdupesFile>) -> Vec<Vec<FdupesFile>> {
    group.into_iter()
    .fold(BTreeMap::new(), |mut acc, file| {
        let group: u64 = 0;
        acc.entry(group).or_insert(Vec::new()).push(file);
        acc
    }).into_iter().map(|(_, group)| group).collect()
}

fn byte_match(groups: Vec<Vec<FdupesFile>>) -> Vec<Vec<FdupesFile>> {
    groups.into_iter().flat_map(|group| {
        partition_group_by_bytes(group)
    }).collect()
}

fn main() {
    let env = Env::default()
        .filter_or("RUST_LOG", "info");
    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(buf,
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
    // Remove files with unique size
    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by size)", groups.len());

    // Remove files with unique partial crc
    let groups = gen_partial_crcs(groups);
    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by partial crc)", groups.len());

    // Remove files with unique full crc
    let groups = gen_full_crcs(groups);
    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by full crc)", groups.len());

    // Remove files with unique bytes
    let groups = byte_match(groups);
    let groups = remove_uniq(groups);
    info!("{} non-unique groups (by exact content)", groups.len());

    let groups = {
        let mut ngroups = groups.clone();
        ngroups.sort_unstable_by(|b, a| a.get(0).map(|f| f.size + 1).unwrap_or(0).cmp(&b.get(0).map(|f| f.size + 1).unwrap_or(0)));
        ngroups
    };

    for bucket in groups.iter() {
      debug!("{:#?}", bucket);
    }
}

