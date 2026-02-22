//! Parse and load `.env` files.
//!
//! [`EnvLoader::new`] defaults to [`TargetEnv::memory`], so loading is process-
//! isolated by default.
//!
//! Convenience loaders (`dotenv`, `from_path`, `from_paths`, `from_filename`)
//! mutate the process environment and are `unsafe`, because callers must
//! guarantee no concurrent process-environment access.

mod env;
mod error;
mod loader;
mod model;
mod parser;

pub use env::TargetEnv;
pub use error::{Error, ParseError, ParseErrorKind};
pub use loader::{EnvLoader, dotenv, from_filename, from_path, from_paths};
pub use model::{Encoding, Entry, KeyParsingMode, LoadReport, SubstitutionMode};
pub use parser::{
    parse_bytes, parse_bytes_with_mode, parse_reader, parse_reader_with_mode, parse_str,
    parse_str_with_mode,
};
