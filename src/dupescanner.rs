use std::{collections::BTreeMap, ffi::OsString, io, sync::mpsc::Sender};

use crate::{dupemessage::DupeMessage, fdupesgroup::FdupesGroup};
use log::{debug, info, trace};
use walkdir::WalkDir;

pub struct DupeScanner {
    tx: Sender<DupeMessage>,
    sourceroot: Vec<OsString>,
    recursive: bool,
    skip_empty: bool,
}

impl DupeScanner {
    pub fn new(
        tx: Sender<DupeMessage>,
        sourceroot: Vec<OsString>,
        recursive: bool,
        skip_empty: bool,
    ) -> Self {
        Self {
            tx,
            sourceroot,
            recursive,
            skip_empty,
        }
    }

    pub fn find_groups(&self) {
        let groups = self.find_files();
        info!("{} total groups (by size)", groups.len());
        self.build_matches(groups);
        self.tx.send(DupeMessage::End).unwrap();
    }
}

impl DupeScanner {
    fn send(&self, groups: Vec<FdupesGroup>) {
        for bucket in groups {
            self.tx.send(bucket.into()).unwrap();
        }
    }

    fn find_files(&self) -> BTreeMap<u64, Vec<String>> {
        info!(
            "find all files in {:?} (recursive: {}, skip_empty: {})",
            self.sourceroot, self.recursive, self.skip_empty
        );
        let all_groups = self
            .sourceroot
            .iter()
            .flat_map(|root| {
                info!("scanning {:?}...", root);
                let walk = WalkDir::new(root);
                if self.recursive {
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
                if size > 0 || !self.skip_empty {
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

    fn build_matches(&self, groups: BTreeMap<u64, Vec<String>>) {
        for (size, filenames) in groups.iter().rev() {
            debug!("build matches {}: {} files", size, filenames.len());
            let mut result = Vec::new();
            for filename in filenames {
                self.update_matches(filename, *size, &mut result);
            }
            debug!(
                " => {:?}",
                result.iter().map(|r| r.filenames.len()).collect::<Vec<_>>()
            );
            self.send(result);
        }
    }

    fn update_matches(&self, filename: &str, size: u64, result: &mut Vec<FdupesGroup>) {
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
