#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;
extern crate serde;

use chrono::Local;
use env_logger::{Builder, Env};
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::io::prelude::*;
use walkdir::WalkDir;

use std::sync::atomic::Ordering;

use fdupes::FdupesGroup;

fn find_files(
    sourceroot: Vec<std::ffi::OsString>,
    recursive: bool,
    skip_empty: bool,
) -> BTreeMap<u64, Vec<String>> {
    info!(
        "find all files in {:?} (recursive: {}, skip_empty: {})",
        sourceroot, recursive, skip_empty
    );
    let all_groups = sourceroot
        .iter()
        .flat_map(|root| {
            info!("scanning {:?}...", root);
            let walk = WalkDir::new(root);
            if recursive {
                walk.into_iter()
            } else {
                walk.max_depth(1).into_iter()
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
            if size > 0 || !skip_empty {
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

fn remove_uniq(groups: Vec<FdupesGroup>) -> Vec<FdupesGroup> {
    groups.into_iter().filter(|value| value.len() > 1).collect()
}

fn matches(file: &mut FdupesGroup, group: &mut FdupesGroup) -> io::Result<bool> {
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

fn update_matches(filename: &str, size: u64, result: &mut Vec<FdupesGroup>) {
    let mut file = FdupesGroup::new(filename, size);
    for r in result.iter_mut() {
        if let Ok(true) = matches(&mut file, r) {
            r.add(filename);
            return;
        }
    }
    result.push(file);
}

fn build_matches(groups: BTreeMap<u64, Vec<String>>) -> Vec<FdupesGroup> {
    let mut matches = Vec::new();
    for (size, filenames) in groups.iter().rev() {
        debug!("build matches {}: {} files", size, filenames.len());
        let mut result = Vec::new();
        for filename in filenames {
            update_matches(filename, *size, &mut result);
        }
        let mut result = remove_uniq(result);
        debug!(" => {:?}", result.iter().map(|r|r.filenames.len()).collect::<Vec<_>>());
        matches.append(&mut result);
        debug!("matches has {} groups", matches.len());
    }
    trace!("{} partial crc calcs", fdupes::GLOBAL_PARTIAL_CRC_COUNT.load(Ordering::SeqCst));
    trace!("{} full crc calcs", fdupes::GLOBAL_FULL_CRC_COUNT.load(Ordering::SeqCst));
    trace!("{} full file reads", fdupes::GLOBAL_FULL_READ_COUNT.load(Ordering::SeqCst));
    matches
}

fn main() {
    let env = Env::default().filter_or("RUST_LOG", "info");
    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
    // TODO cmd-line args

    let sourceroot = env::args_os().skip(1).collect::<Vec<_>>();
    let recursive = true;
    let skip_empty = true;

    let groups = find_files(sourceroot, recursive, skip_empty);
    info!("{} total groups (by size)", groups.len());
    let groups = build_matches(groups);

    let groups = remove_uniq(groups);

    for bucket in &groups {
        if bucket.size == 1 {
            println!("{} byte each:", bucket.size);
        } else {
            println!("{} bytes each:", bucket.size);
        }
        for filename in &bucket.filenames {
            println!("{} (W)", filename);
        }
        println!();
    }
    info!("{} non-unique groups (by exact content)", groups.len());
}
