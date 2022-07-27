use std::fmt::Debug;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{collections::BTreeMap, sync::mpsc::Sender};

use bool_ext::BoolExt;
use log::{debug, info, trace};
use walkdir::WalkDir;

use crate::{Config, DupeMessage};

mod fdupesgroup;

use self::fdupesgroup::FdupesGroup;

pub struct DupeScanner {
    tx: Sender<DupeMessage>,
    config: Arc<Config>,
    group_comparators: Vec<Box<dyn GroupComparator>>,
}

pub trait GroupComparator: Debug + Send {
    fn name(&self) -> &str;
    fn can_analyse(&self, path: &Path) -> bool;
    fn open(&self, path: &str) -> io::Result<GroupReader>;
}

pub struct GroupReader {
    reader: Box<dyn Read>,
}

impl From<File> for GroupReader {
    fn from(file: File) -> Self {
        Self {
            reader: Box::new(file),
        }
    }
}
#[derive(Debug)]
pub struct ExactGroupComparator {}
impl Default for ExactGroupComparator {
    fn default() -> Self {
        Self::new()
    }
}
impl GroupComparator for ExactGroupComparator {
    fn name(&self) -> &str {
        "exact"
    }

    fn can_analyse(&self, _path: &Path) -> bool {
        true
    }

    fn open(&self, path: &str) -> io::Result<GroupReader> {
        File::open(path).map(|f| f.into())
    }
}

impl ExactGroupComparator {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub struct JsonGroupComparator {}
impl Default for JsonGroupComparator {
    fn default() -> Self {
        Self::new()
    }
}
impl GroupComparator for JsonGroupComparator {
    fn name(&self) -> &str {
        "json"
    }

    fn can_analyse(&self, path: &Path) -> bool {
        let reader = match File::open(path) {
            Ok(f) => BufReader::new(f),
            Err(_) => return false,
        };
        log::debug!("can analyse {path:?}");
        serde_json::from_reader::<BufReader<_>, serde_json::Value>(reader).is_ok()
    }

    fn open(&self, path: &str) -> io::Result<GroupReader> {
        File::open(path).map(|f| f.into())
    }
}

impl JsonGroupComparator {
    pub fn new() -> Self {
        Self {}
    }
}

impl DupeScanner {
    pub fn new(
        tx: Sender<DupeMessage>,
        config: Arc<Config>,
        group_comparators: Vec<Box<dyn GroupComparator>>,
    ) -> Self {
        info!("group_comparators: {group_comparators:?}");
        Self {
            tx,
            config,
            group_comparators,
        }
    }
}

impl DupeScanner {
    pub fn find_groups(&self) {
        let groups = self.find_files();
        info!("{} total groups (by size)", groups.len());
        debug!("groups: {groups:#?}");
        //self.build_matches(groups).unwrap();
    }

    fn send(&self, (id, total, groups): (usize, usize, Vec<FdupesGroup>)) -> Result<(), io::Error> {
        for bucket in groups {
            if bucket.filenames.len() > 1 {
                // TODO: Handle send failures
                log::debug!("send: {:?}", bucket);
                self.tx.send(bucket.into_dupe_message(total, id)).unwrap();
            }
        }
        Ok(())
    }

    fn find_files_root(
        root: String,
        non_recursive: bool,
        min_size: u64,
    ) -> std::thread::JoinHandle<std::vec::Vec<(u64, std::path::PathBuf)>>
    {
        std::thread::spawn(move || {
            info!("scanning {:?}...", root);
            let r = WalkDir::new(&root)
                .max_depth(non_recursive.map(usize::MAX, 1))
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_file())
                .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_owned()))
                .collect();
            info!("scanning {:?} complete.", root);
            r
        })
    }

    fn find_files(&self) -> BTreeMap<u64, Vec<(&Box<dyn GroupComparator>, PathBuf)>> {
        info!(
            "find all files in {:?} (non-recursive: {}, min_size: {})",
            self.config.roots, self.config.non_recursive, self.config.min_size
        );

        let all_groups = self
            .config
            .roots
            .iter()
            .map(|r| {
                Self::find_files_root(
                    r.to_owned(),
                    self.config.non_recursive,
                    self.config.min_size,
                )
            })
            .filter_map(|h| h.join().ok())
            .flatten()
            .fold(BTreeMap::new(), |mut acc, entry| {
                for comparator in &self.group_comparators {
                    if comparator.can_analyse(&entry.1) {
                        let size = entry.0;
                        if size > self.config.min_size {
                            acc.entry(size)
                                .or_insert_with(Vec::new)
                                .push((comparator, entry.1.clone()));
                        }
                    }
                }
                acc
            });
        info!("{} non-unique groups (by size)", all_groups.len());
        if log::log_enabled!(log::Level::Debug) {
            for (size, files) in &all_groups {
                debug!("group {size} => {files:?}");
            }
        }
        all_groups
            .into_iter()
            .filter(|(_size, files)| files.len() > 1)
            .collect()
    }

    fn build_matches(
        &self,
        groups: BTreeMap<u64, Vec<PathBuf>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let total = groups.len();
        for (id, (size, filenames)) in groups.iter().rev().enumerate() {
            debug!("build matches {}: {} files", size, filenames.len());
            let mut result = Vec::new();
            for filename in filenames {
                self.update_matches(filename, *size, &mut result);
            }
            debug!(
                " => {:?}",
                result.iter().map(|r| r.filenames.len()).collect::<Vec<_>>()
            );
            self.send((id, total, result))?;
        }
        Ok(())
    }

    fn update_matches(&self, filename: &Path, size: u64, result: &mut Vec<FdupesGroup>) {
        let mut file = FdupesGroup::new(filename, size);
        for r in result.iter_mut() {
            if let Ok(true) = self.matches(&mut file, r) {
                r.add(filename);
                return;
            }
        }
        result.push(file);
    }

    fn matches(&self, file: &mut FdupesGroup, group: &mut FdupesGroup) -> io::Result<bool> {
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
}
