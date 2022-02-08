use clap::Parser;

mod dupemessage;
mod scanner;

pub mod receiver;

#[derive(Debug, Clone)]
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Only find duplicates immediately within supplied directories.
    #[clap(short, long)]
    pub non_recursive: bool,
    /// Should empty files be considered duplicates.
    #[clap(short='0', long)]
    pub include_empty: bool,
    /// Show sizes of files within duplicate groups.
    #[clap(short='S', long)]
    pub show_sizes: bool,
    /// Path(s) to search for files within.
    pub root: Vec<String>,
    /// prompt user for files to preserve and delete all others.
    #[clap(short='p', long)]
    pub prompt: bool,
}

pub use crate::dupemessage::DupeMessage;
pub use crate::scanner::DupeScanner;
