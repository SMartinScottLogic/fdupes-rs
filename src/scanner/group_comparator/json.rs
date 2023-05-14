use std::{path::Path, io::{self, BufReader}, fs::File};

use tracing::debug;

use super::{GroupComparator,GroupReader};

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
        false
        // let reader = match File::open(path) {
        //     Ok(f) => BufReader::new(f),
        //     Err(_) => return false,
        // };
        // let can_analyse =
        //     serde_json::from_reader::<BufReader<_>, serde_json::Value>(reader).is_ok();
        // debug!(path = debug(path), can_analyse, "can_analyse");
        // can_analyse
    }

    fn open(&self, path: &str) -> io::Result<GroupReader> {
        File::open(path).and_then(|_| Err(io::Error::new(io::ErrorKind::Unsupported, path)))
    }
}

impl JsonGroupComparator {
    pub fn new() -> Self {
        Self {}
    }
}
