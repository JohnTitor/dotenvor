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

/// Variable expansion behavior for loader values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubstitutionMode {
    /// Keep values as parsed with no expansion.
    #[default]
    Disabled,
    /// Expand `$VAR` and `${VAR}` placeholders.
    Expand,
}

/// Key validation behavior for parser and loader entry parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyParsingMode {
    /// Existing key character rules (`[A-Za-z0-9_.-]+`).
    #[default]
    Strict,
    /// POSIX-portable ASCII keys (except `=`) for cross-platform compatibility.
    Permissive,
}
