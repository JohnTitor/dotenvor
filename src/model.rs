use std::path::PathBuf;

/// A parsed `KEY=VALUE` entry from a `.env` file or input buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub source: Option<PathBuf>,
    pub line: u32,
}

/// Summary of the load operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadReport {
    pub loaded: usize,
    pub skipped_existing: usize,
    pub files_read: usize,
}

/// Encoding choice for input data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// UTF-8 text input.
    #[default]
    Utf8,
}
