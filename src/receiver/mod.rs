use std::path::PathBuf;

#[derive(PartialEq, Copy, Clone)]
enum Mark {
    Purge,
    Keep,
}

type DupeGroup<'a> = Vec<(&'a PathBuf, Mark)>;

mod basic_receiver;

pub use basic_receiver::BasicReceiver;

pub trait DupeGroupReceiver: Send {
    fn run(&mut self) -> Result<(), std::io::Error>;
}

fn mark_group(files: &mut DupeGroup, purge: Mark) {
    for file in files {
        file.1 = purge;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    lazy_static::lazy_static! {
    static ref FILE1: PathBuf = PathBuf::from("test1");
    static ref FILE2: PathBuf = PathBuf::from("test2");
    static ref FILE3: PathBuf = PathBuf::from("test3");
    }

    #[test]
    fn mark_group_false() {
        let mut files = vec![
            (&*FILE1, Mark::Keep),
            (&*FILE2, Mark::Keep),
            (&*FILE3, Mark::Keep),
        ];
        mark_group(&mut files, Mark::Purge);
        for (file, mark) in files {
            assert!(mark == Mark::Purge, "{file:?} should be purged");
        }
    }

    #[test]
    fn mark_group_true() {
        let mut files = vec![
            (&*FILE1, Mark::Purge),
            (&*FILE2, Mark::Purge),
            (&*FILE3, Mark::Purge),
        ];
        mark_group(&mut files, Mark::Keep);
        for (file, mark) in files {
            assert!(mark == Mark::Keep, "{file:?} should be retained");
        }
    }
}
