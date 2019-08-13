#[macro_use]
extern crate log;
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

const BLOCK_SIZE: usize = 1024;

fn find_files(sourceroot: std::ffi::OsString, recursive: bool) -> BTreeMap<u64, Vec<String>> {
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
            acc.entry(entry.0).or_insert(Vec::new()).push(entry.1);
            acc
        })
}

fn remove_uniq<K: std::cmp::Ord>(groups: BTreeMap<K, Vec<String>>) -> BTreeMap<K, Vec<String>> {
    groups.into_iter().filter(|(_, value)| value.len() > 1).collect()
}

fn gen_partial_crc(filename:&str) -> io::Result<(String, u16)> {
    let mut f = File::open(filename).unwrap();
    let mut buffer = [0; BLOCK_SIZE];

    f.read(&mut buffer)?;
    Ok((filename.to_string(), crc16::checksum_usb(&buffer)))
}

fn gen_partial_crcs(groups: BTreeMap<u64, Vec<String>>) -> BTreeMap<(u64, u64), Vec<String>> {
    groups.into_iter().flat_map(|(size, group)| {
        group.into_iter()
        .map(|filename| gen_partial_crc(&filename))
        .filter_map(Result::ok)
        .fold(BTreeMap::new(), |mut acc, (filename, crc)| {
            acc.entry((size, crc as u64)).or_insert(Vec::new()).push(filename);
            acc
        })
    }).collect()
}

fn gen_full_crc(filename:&str) -> io::Result<(String, u16)> {
    let file = File::open(filename)?;
    let mut reader = BufReader::new(file);
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

    Ok((filename.to_string(), digest.sum16()))
}

fn gen_full_crcs(groups: BTreeMap<(u64, u64), Vec<String>>) -> BTreeMap<(u64, u64), Vec<String>> {
    groups.into_iter().flat_map(|(key, group)| {
        group.into_iter()
        .map(|filename| gen_full_crc(&filename))
        .filter_map(Result::ok)
        .fold(BTreeMap::new(), |mut acc, (filename, crc)| {
            acc.entry((key.0, crc as u64)).or_insert(Vec::new()).push(filename);
            acc
        })
    }).collect()
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

fn partition_group_by_bytes(size: u64, group: Vec<String>) -> BTreeMap<(u64, u64), Vec<String>> {
    group.into_iter()
    .fold(BTreeMap::new(), |mut acc, filename| {
        let group: u64 = 0;
        acc.entry((size, group)).or_insert(Vec::new()).push(filename);
        acc
    })
}

fn byte_match(groups: BTreeMap<(u64, u64), Vec<String>>) -> BTreeMap<(u64, u64), Vec<String>> {
    groups.into_iter().flat_map(|((size, _), group)| {
        partition_group_by_bytes(size, group)
    }).collect()
}

fn main() {
    env_logger::init().unwrap();
    // TODO cmd-line args

    let sourceroot = env::args_os().nth(1).unwrap();
    let recursive = true;

    let groups = find_files(sourceroot, recursive);
    // Remove files with unique size
    let groups = remove_uniq(groups);
    // Get starting CRC
    let groups = gen_partial_crcs(groups);
    // Remove files with unique partial crc
    let groups = remove_uniq(groups);
    let groups = gen_full_crcs(groups);
    // Remove files with unique full crc
    let groups = remove_uniq(groups);
    let groups = byte_match(groups);
    // Remove files with unique bytes
    let groups = remove_uniq(groups);

    for bucket in groups.iter().rev() {
      debug!("{:#?}", bucket);
    }
}

