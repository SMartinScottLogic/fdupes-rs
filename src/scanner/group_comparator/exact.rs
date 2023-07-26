use std::{path::Path, io::{self, BufReader}, fs::File};

use tracing::trace;

use super::{GroupComparator,GroupReader};

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

    fn can_analyse(&self, path: &Path) -> bool {
        let can_analyse = true;
        trace!(path = debug(path), can_analyse, "can_analyse");
        can_analyse
    }

    fn open(&self, path: &dyn AsRef<Path>) -> io::Result<GroupReader> {
        File::open(path).map(|f| GroupReader {
            reader: Box::new(BufReader::new(f)),
        })
    }
}
impl ExactGroupComparator {
    pub fn new() -> Self {
        Self {}
    }
}
