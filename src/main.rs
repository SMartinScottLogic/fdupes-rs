#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;

fn main() {
    env_logger::init().unwrap();
    // TODO cmd-line args

    let sourceroot = env::args_os().nth(1).unwrap();

    let recursive = true;

    let walk = WalkDir::new(sourceroot);
    let walk = match recursive {
        true => walk.into_iter(),
        false => walk.max_depth(1).into_iter()
    };

    let mut i: BTreeMap<u64, Vec<String> > = walk
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
}

