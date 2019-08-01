#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;
use crc::crc16;
use std::io;
use std::io::prelude::*;
use std::fs::File;

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
    let mut buffer = [0; 1024];

    // read up to 10 bytes
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
    let mut f = File::open(filename).unwrap();
    let mut buffer = Vec::new();
    // read the whole file
    f.read_to_end(&mut buffer)?;
    Ok((filename.to_string(), crc16::checksum_usb(&buffer)))
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

fn byte_match<K: std::cmp::Ord>(groups: BTreeMap<K, Vec<String>>) -> BTreeMap<K, Vec<String>> {
    groups
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

