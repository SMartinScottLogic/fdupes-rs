use crate::fdupesgroup::FdupesGroup;

pub enum DupeMessage {
    End,
    Group(u64, Vec<String>),
}

impl std::convert::From<FdupesGroup> for DupeMessage {
    fn from(group: FdupesGroup) -> Self {
        Self::Group(group.size, group.filenames)
    }
}
