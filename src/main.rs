#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;

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

fn remove_uniq<K: std::cmp::Ord, V>(groups: BTreeMap<K, Vec<V>>) -> BTreeMap<K, Vec<V>> {
    groups.into_iter().filter(|(_, value)| value.len() > 1).collect()
}

fn gen_partial_crcs<K: std::cmp::Ord, V>(groups: BTreeMap<K, Vec<V>>) -> BTreeMap<K, Vec<V>> {
    groups
}

fn gen_full_crcs<K: std::cmp::Ord, V>(groups: BTreeMap<K, Vec<V>>) -> BTreeMap<K, Vec<V>> {
    groups
}

fn byte_match<K: std::cmp::Ord, V>(groups: BTreeMap<K, Vec<V>>) -> BTreeMap<K, Vec<V>> {
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

