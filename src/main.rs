#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;

fn main() {
    env_logger::init().unwrap();

    let sourceroot = env::args_os().nth(1).unwrap();

    let mut i: BTreeMap<u64, Vec<String> > = WalkDir::new(sourceroot)
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_str().unwrap().to_string()))
        .fold(BTreeMap::new(), |mut acc, entry| {
            acc.entry(entry.0).or_insert(Vec::new()).push(entry.1);
            acc
        });
    for bucket in i.iter().rev() {
      debug!("{:#?}", bucket);
    }
    debug!("{:#?}", i);
}

