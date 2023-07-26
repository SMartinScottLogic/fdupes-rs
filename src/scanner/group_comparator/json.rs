use std::{path::Path, io, fs::File};

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

    fn can_analyse(&self, _path: &Path) -> bool {
        false
        // let reader = match File::open(path) {
        //     Ok(f) => BufReader::new(f),
        //     Err(_) => return false,
        // };
        // let can_analyse =
        //     serde_json::from_reader::<BufReader<_>, serde_json::Value>(reader).is_ok();
        // trace!(path = debug(path), can_analyse, "can_analyse");
        // can_analyse
    }

    fn open(&self, path: &dyn AsRef<Path>) -> io::Result<GroupReader> {
        File::open(path).and_then(|_| Err(io::Error::new(io::ErrorKind::Unsupported, path.as_ref().to_string_lossy())))
    }
}

impl JsonGroupComparator {
    pub fn new() -> Self {
        Self {}
    }
}
