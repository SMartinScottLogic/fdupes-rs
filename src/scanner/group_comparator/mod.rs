use std::{fmt::Debug, path::Path, io::{self, BufRead}};

mod exact;
mod json;
pub use exact::ExactGroupComparator;
pub use json::JsonGroupComparator;

pub trait GroupComparator: Debug + Send + Sync {
    fn name(&self) -> &str;
    fn can_analyse(&self, path: &Path) -> bool;
    fn open(&self, path: &dyn AsRef<Path>) -> io::Result<GroupReader>;
}

pub struct GroupReader {
    pub reader: Box<dyn BufRead>,
}


