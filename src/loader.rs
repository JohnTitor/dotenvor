use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::TargetEnv;
use crate::error::Error;
use crate::model::{Encoding, Entry, LoadReport, SubstitutionMode};
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
    substitution_mode: SubstitutionMode,
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

    pub fn substitution_mode(mut self, substitution_mode: SubstitutionMode) -> Self {
        self.substitution_mode = substitution_mode;
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
        let (mut entries, _) = self.collect_entries()?;
        self.apply_substitution(&mut entries);
        Ok(entries)
    }

    pub fn load(&mut self) -> Result<LoadReport, Error> {
        let (mut entries, files_read) = self.collect_entries()?;
        self.apply_substitution(&mut entries);
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

    fn apply_substitution(&self, entries: &mut [Entry]) {
        if self.substitution_mode == SubstitutionMode::Disabled {
            return;
        }

        let mut resolver = SubstitutionResolver::new(entries, &self.target, self.override_existing);
        for entry in entries.iter_mut() {
            entry.value = resolver.resolve_entry(&entry.key);
        }
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
            substitution_mode: SubstitutionMode::Disabled,
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

struct SubstitutionResolver<'a> {
    raw_values: HashMap<String, String>,
    resolved_values: HashMap<String, String>,
    target: &'a TargetEnv,
    override_existing: bool,
}

impl<'a> SubstitutionResolver<'a> {
    fn new(entries: &[Entry], target: &'a TargetEnv, override_existing: bool) -> Self {
        let raw_values = entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.value.clone()))
            .collect();

        Self {
            raw_values,
            resolved_values: HashMap::new(),
            target,
            override_existing,
        }
    }

    fn resolve_entry(&mut self, key: &str) -> String {
        self.resolve_key(key, &mut Vec::new())
    }

    fn resolve_key(&mut self, key: &str, stack: &mut Vec<String>) -> String {
        if let Some(existing) = self.resolved_values.get(key) {
            return existing.clone();
        }

        if !self.override_existing && self.target.contains_key(key) {
            let existing = self.target.get_var(key).unwrap_or_default();
            self.resolved_values
                .insert(key.to_owned(), existing.clone());
            return existing;
        }

        let Some(raw_value) = self.raw_values.get(key).cloned() else {
            return self.target.get_var(key).unwrap_or_default();
        };

        stack.push(key.to_owned());
        let expanded = expand_template(&raw_value, |name, token| {
            self.resolve_placeholder(name, token, stack)
        });
        stack.pop();

        self.resolved_values
            .insert(key.to_owned(), expanded.clone());
        expanded
    }

    fn resolve_placeholder(&mut self, name: &str, token: &str, stack: &mut Vec<String>) -> String {
        if stack.iter().any(|item| item == name) {
            return token.to_owned();
        }

        if self.raw_values.contains_key(name) {
            return self.resolve_key(name, stack);
        }

        self.target
            .get_var(name)
            .unwrap_or_else(|| token.to_owned())
    }
}

fn expand_template<F>(input: &str, mut resolve: F) -> String
where
    F: FnMut(&str, &str) -> String,
{
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    let mut idx = 0usize;
    let bytes = input.as_bytes();

    while idx < bytes.len() {
        if bytes[idx] != b'$' {
            idx += 1;
            continue;
        }

        let Some((name_start, name_end, token_end)) = parse_placeholder(input, idx) else {
            idx += 1;
            continue;
        };

        let name = &input[name_start..name_end];
        let token = &input[idx..token_end];

        out.push_str(&input[cursor..idx]);
        out.push_str(&resolve(name, token));

        cursor = token_end;
        idx = token_end;
    }

    out.push_str(&input[cursor..]);
    out
}

fn parse_placeholder(input: &str, start: usize) -> Option<(usize, usize, usize)> {
    let bytes = input.as_bytes();
    if start + 1 >= bytes.len() {
        return None;
    }

    if bytes[start + 1] == b'{' {
        let mut end = start + 2;
        while end < bytes.len() && bytes[end] != b'}' {
            end += 1;
        }

        if end >= bytes.len() {
            return None;
        }

        let name_start = start + 2;
        let name_end = end;
        let name = &input[name_start..name_end];
        if name.is_empty() || !name.bytes().all(is_braced_var_char) {
            return None;
        }

        return Some((name_start, name_end, end + 1));
    }

    let name_start = start + 1;
    if !is_unbraced_var_start(bytes[name_start]) {
        return None;
    }

    let mut name_end = name_start + 1;
    while name_end < bytes.len() && is_unbraced_var_char(bytes[name_end]) {
        name_end += 1;
    }

    Some((name_start, name_end, name_end))
}

fn is_braced_var_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || byte == b'-'
}

fn is_unbraced_var_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_unbraced_var_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
