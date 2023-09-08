use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{collections::BTreeMap, sync::mpsc::Sender};

use bool_ext::BoolExt;
use tracing::{debug, info, trace, error};
use walkdir::WalkDir;
use itertools::Itertools;

use crate::{Config, DbMessage, DupeMessage};

mod fdupesgroup;
pub(crate) mod group_comparator;

use self::fdupesgroup::FdupesGroup;
use group_comparator::GroupComparator;

pub struct DupeScanner {
    tx: Sender<DupeMessage>,
    db_connection: sqlite::Connection,
    config: Arc<Config>,
    group_comparators: BTreeMap<String, Box<dyn GroupComparator>>,
}

impl DupeScanner {
    pub fn new(
        tx: Sender<DupeMessage>,
        db_connection: sqlite::Connection,
        config: Arc<Config>,
        group_comparators: Vec<Box<dyn GroupComparator>>,
    ) -> Self {
        info!("setup db");
        let query = "CREATE TABLE IF NOT EXISTS files (path TEXT, size INTEGER, PRIMARY KEY(path ASC));";
        db_connection.execute(query).unwrap();
        info!("group_comparators: {group_comparators:?}");
        let group_comparators =
            group_comparators
                .into_iter()
                .fold(BTreeMap::new(), |mut acc, v| {
                    let name = v.name().to_owned();
                    debug!(name, comparator = debug(&v), "Add group_comparator");
                    if let Some(existing) = acc.insert(name.clone(), v) {
                        panic!(
                            "Unexpectedly already had comparator with name {}: {:?}",
                            name, existing
                        );
                    }
                    acc
                });
        Self {
            tx,
            db_connection,
            config,
            group_comparators,
        }
    }
}

impl DupeScanner {
    pub fn find_groups(&self) {
        let groups = self.find_files();
        if tracing::enabled!(tracing::Level::DEBUG) {
            debug!("{} total groups (by size): {:#?}", groups.len(), &groups);
        } else {
            info!("{} total groups (by size)", groups.len());
        }

        self.build_matches(groups).unwrap();
    }

    fn send(&self, (id, total, groups): (usize, usize, Vec<FdupesGroup>)) -> Result<(), io::Error> {
        for bucket in groups {
            if bucket.filenames.len() > 1 {
                // TODO: Handle send failures
                debug!(bucket = debug(&bucket), "send");
                self.tx.send(bucket.into_dupe_message(total, id)).unwrap();
            }
        }
        Ok(())
    }

    fn persist(&self, path: &Path, size: u64) {
        let statement = "INSERT INTO files VALUES (:path, :size)";
        match  self.db_connection.prepare(statement) {
            Ok(mut statement) => {
        statement.bind_iter::<_, (_, sqlite::Value)>([
            (":path", path.to_str().unwrap().into()),
            (":size", (size as i64).into())
        ]).unwrap();
        for row in statement.into_iter() {
            error!(row = debug(row), "statement result");
        }
        }, Err(e) => error!(error = debug(e), "failed to prepare statement from '{statement}'")
        }
    }

    fn persist_batch(&self, batch: &Vec<(u64, PathBuf)>) {
        let mut statement = "INSERT INTO files VALUES ".to_string();
        statement += &"(?, ?),".repeat(batch.len());
        statement.pop();
        debug!(statement, "batch sql");
        match self.db_connection.prepare(&statement) {
            Ok(mut statement) => {
                for (idx, (size, path)) in batch.iter().enumerate() {
                    debug!(size, path = debug(path), "parameter set {idx}");
                    statement.bind((1 + 2 * idx, std::convert::Into::<sqlite::Value>::into(path.to_str().unwrap())));
                    statement.bind((2 + 2 * idx, std::convert::Into::<sqlite::Value>::into(*size as i64)));
                }
                for row in statement.into_iter() {
                    debug!(row = debug(row), "statement result");
                }
            },
            Err(e) => error!(error = debug(e), "failed to prepare statement from '{statement}'")
        }
    }

    fn find_files_root(
        &self,
        root: String,
        non_recursive: bool,
        min_size: u64,
    ) -> std::thread::JoinHandle<std::vec::Vec<(u64, std::path::PathBuf)>> {
        std::thread::spawn(move || {
            info!("scanning {:?}...", root);
            let r = WalkDir::new(&root)
                .max_depth(non_recursive.map(usize::MAX, 1))
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_file())
                .filter(|entry| entry.metadata().unwrap().len() >= min_size)
                .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_owned()))
                .collect();
            info!("scanning {:?} complete.", root);
            r
        })
    }

    fn find_files(&self) -> BTreeMap<(u64, String), Vec<PathBuf>> {
        info!(
            "find all files in {:?} (non-recursive: {}, min_size: {})",
            self.config.roots, self.config.non_recursive, self.config.min_size
        );

        let all_groups = self
            .config
            .roots
            .iter()
            .map(|r| {
                self.find_files_root(
                    r.to_owned(),
                    self.config.non_recursive,
                    self.config.min_size,
                )
            })
            .filter_map(|h| h.join().ok())
            .flatten()
            .chunks(50)
            //.inspect(|(size, path)| self.persist(path, *size))
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .inspect(|batch| self.persist_batch(batch))
            .flat_map(|batch| batch.into_iter())
            .fold(BTreeMap::new(), |mut acc, (raw_size, path)| {
                for (comparator_name, comparator) in &self.group_comparators {
                    if raw_size >= self.config.min_size && comparator.can_analyse(&path) {
                        //TODO Comparator generate size (e.g. post-json normalization)
                        acc.entry((raw_size, comparator_name.to_owned()))
                            .or_insert_with(Vec::new)
                            .push(path.clone());
                    }
                }
                acc
            });
        if tracing::enabled!(tracing::Level::DEBUG) {
            debug!(
                "{} non-unique groups (by size): {:#?}",
                all_groups.len(),
                &all_groups
            );
        } else {
            info!("{} non-unique groups (by size)", all_groups.len());
        }
        all_groups
            .into_iter()
            .filter(|(_, files)| files.len() > 1)
            .collect()
    }

    fn build_matches(
        &self,
        groups: BTreeMap<(u64, String), Vec<PathBuf>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let total = groups.len();
        // TODO: What to do when multiple comparators match same group (or partial groups)?
        for (id, ((size, comparator_name), filenames)) in groups.iter().rev().enumerate() {
            debug!(
                "build matches ({}, {}): {} files",
                size,
                comparator_name,
                filenames.len()
            );
            let mut result = Vec::new();
            for filename in filenames {
                self.update_matches(filename, *size, comparator_name, &mut result);
            }
            debug!(
                " => {:?}",
                result.iter().map(|r| r.filenames.len()).collect::<Vec<_>>()
            );
            self.send((id, total, result))?;
        }
        Ok(())
    }

    fn update_matches<'a>(
        &'a self,
        filename: &Path,
        size: u64,
        comparator_name: &str,
        result: &mut Vec<FdupesGroup<'a>>,
    ) {
        let comparator = self.group_comparators.get(comparator_name).unwrap();
        //panic!("Update matches for {comparator_name}: {comparator:?}");
        //TODO Restriction to comparator logics
        let mut file = FdupesGroup::new(filename, size, comparator.as_ref());
        for r in result
            .iter_mut()
            .filter(|g| g.comparator.name() == comparator_name)
        {
            match self.matches(&mut file, r) {
                Ok(true) => {
                    r.add(filename);
                    return;
                }
                Ok(false) => {
                    debug!(filename = debug(filename), file = debug(&file), "different");
                }
                Err(e) => {
                    debug!(filename = debug(filename), file = debug(&file), error = debug(e), "failure");
                }
            }
        }
        result.push(file);
    }

    fn matches<'a, 'b>(
        &self,
        file: &mut FdupesGroup<'a>,
        group: &mut FdupesGroup<'b>,
    ) -> io::Result<bool>
    where
        'a: 'b,
        'b: 'a,
    {
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
        if file == group {
            Ok(true)
        } else {
            Ok(false)
        }
        //Ok(file == group)
    }
}
