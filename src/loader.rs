use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::TargetEnv;
use crate::error::Error;
use crate::model::{Encoding, Entry, LoadReport};
use crate::parser::parse_str_with_source;

/// Load `.env` from the current working directory.
pub fn dotenv() -> Result<LoadReport, Error> {
    from_filename(".env")
}

/// Load a `.env` file from a specific path into the process environment.
pub fn from_path(path: impl AsRef<Path>) -> Result<LoadReport, Error> {
    let mut loader = EnvLoader::new().path(path);
    loader.load()
}

/// Load multiple `.env` files into the process environment.
pub fn from_paths<I, P>(paths: I) -> Result<LoadReport, Error>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut loader = EnvLoader::new().paths(paths);
    loader.load()
}

/// Load a dotenv file by filename from the current working directory.
pub fn from_filename(name: &str) -> Result<LoadReport, Error> {
    from_path(PathBuf::from(name))
}

/// Builder-style dotenv loader.
#[derive(Debug, Clone)]
pub struct EnvLoader {
    paths: Vec<PathBuf>,
    encoding: Encoding,
    override_existing: bool,
    debug: bool,
    target: TargetEnv,
}

impl EnvLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.paths
            .extend(paths.into_iter().map(|path| path.as_ref().to_path_buf()));
        self
    }

    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn override_existing(mut self, override_existing: bool) -> Self {
        self.override_existing = override_existing;
        self
    }

    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn target(mut self, target: TargetEnv) -> Self {
        self.target = target;
        self
    }

    pub fn target_env(&self) -> &TargetEnv {
        &self.target
    }

    pub fn target_env_mut(&mut self) -> &mut TargetEnv {
        &mut self.target
    }

    pub fn into_target(self) -> TargetEnv {
        self.target
    }

    pub fn parse_only(&self) -> Result<Vec<Entry>, Error> {
        let (entries, _) = self.collect_entries()?;
        Ok(entries)
    }

    pub fn load(&mut self) -> Result<LoadReport, Error> {
        let (entries, files_read) = self.collect_entries()?;
        let mut report = LoadReport {
            files_read,
            ..LoadReport::default()
        };

        for entry in entries {
            if !self.override_existing && self.target.contains_key(&entry.key) {
                report.skipped_existing += 1;
                if self.debug {
                    eprintln!("dotenvor: skipping existing key {}", entry.key);
                }
                continue;
            }

            self.target.set_var(&entry.key, &entry.value);
            report.loaded += 1;
        }

        Ok(report)
    }

    fn collect_entries(&self) -> Result<(Vec<Entry>, usize), Error> {
        let mut merged_entries = Vec::new();
        let mut by_key = HashMap::<String, usize>::new();
        let mut files_read = 0usize;

        for path in self.effective_paths() {
            let bytes = std::fs::read(&path)?;
            files_read += 1;
            let content = decode(&bytes, self.encoding)?;
            let parsed = parse_str_with_source(content, Some(&path)).map_err(Error::from)?;

            for entry in parsed {
                if let Some(existing_idx) = by_key.get(&entry.key).copied() {
                    merged_entries[existing_idx] = entry;
                } else {
                    by_key.insert(entry.key.clone(), merged_entries.len());
                    merged_entries.push(entry);
                }
            }
        }

        Ok((merged_entries, files_read))
    }

    fn effective_paths(&self) -> Vec<PathBuf> {
        if self.paths.is_empty() {
            vec![PathBuf::from(".env")]
        } else {
            self.paths.clone()
        }
    }
}

impl Default for EnvLoader {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            encoding: Encoding::Utf8,
            override_existing: false,
            debug: false,
            target: TargetEnv::Process,
        }
    }
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<&str, Error> {
    match encoding {
        Encoding::Utf8 => Ok(std::str::from_utf8(bytes)?),
    }
}
