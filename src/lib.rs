//! Parse and load `.env` files.
//!
//! Convenience loaders (`dotenv`, `from_path`, `from_paths`, `from_filename`)
//! and [`EnvLoader::new`] default to [`TargetEnv::Process`], which mutates the
//! process environment via [`std::env::set_var`]. This is not thread-safe for
//! concurrent environment access, so prefer [`TargetEnv::memory`].

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
