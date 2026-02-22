use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::TargetEnv;
use crate::error::Error;
use crate::model::{Encoding, Entry, KeyParsingMode, LoadReport, SubstitutionMode};
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
    let mut loader = EnvLoader::new().path(name).search_upward(true);
    loader.load()
}

/// Builder-style dotenv loader.
#[derive(Debug, Clone)]
pub struct EnvLoader {
    paths: Vec<PathBuf>,
    encoding: Encoding,
    override_existing: bool,
    key_parsing_mode: KeyParsingMode,
    search_upward: bool,
    substitution_mode: SubstitutionMode,
    verbose: bool,
    quiet: bool,
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

    pub fn key_parsing_mode(mut self, key_parsing_mode: KeyParsingMode) -> Self {
        self.key_parsing_mode = key_parsing_mode;
        self
    }

    pub fn search_upward(mut self, search_upward: bool) -> Self {
        self.search_upward = search_upward;
        self
    }

    pub fn substitution_mode(mut self, substitution_mode: SubstitutionMode) -> Self {
        self.substitution_mode = substitution_mode;
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
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
        let (mut entries, _) = self.collect_entries(true)?;
        self.apply_substitution(&mut entries);
        self.log(&format!(
            "parsed {} entr{}",
            entries.len(),
            plural(entries.len(), "y", "ies")
        ));
        Ok(entries)
    }

    pub fn load(&mut self) -> Result<LoadReport, Error> {
        let (mut entries, files_read) = self.collect_entries(false)?;
        self.apply_substitution(&mut entries);
        let mut report = LoadReport {
            files_read,
            ..LoadReport::default()
        };

        for entry in entries {
            if !self.override_existing && self.target.contains_key(&entry.key) {
                report.skipped_existing += 1;
                self.log(&format!("skipping existing key {}", entry.key));
                continue;
            }

            self.log(&format!("setting key {}", entry.key));
            self.target.set_var(&entry.key, &entry.value);
            report.loaded += 1;
        }

        self.log(&format!(
            "load complete: files_read={}, loaded={}, skipped_existing={}",
            report.files_read, report.loaded, report.skipped_existing
        ));
        Ok(report)
    }

    fn collect_entries(&self, include_source: bool) -> Result<(Vec<Entry>, usize), Error> {
        let paths = self.effective_paths()?;
        if paths.len() == 1 {
            let path = &paths[0];
            self.log(&format!("reading {}", path.display()));
            let bytes = std::fs::read(path)?;
            let content = decode(&bytes, self.encoding)?;
            let parsed = parse_str_with_source(
                content,
                include_source.then_some(path.as_path()),
                self.key_parsing_mode,
            )
            .map_err(Error::from)?;
            return Ok((parsed, 1));
        }

        let mut merged_entries = Vec::new();
        let mut by_key = HashMap::<String, usize>::new();
        let mut files_read = 0usize;

        for path in paths {
            self.log(&format!("reading {}", path.display()));
            let bytes = std::fs::read(&path)?;
            files_read += 1;
            let content = decode(&bytes, self.encoding)?;
            let parsed = parse_str_with_source(
                content,
                include_source.then_some(path.as_path()),
                self.key_parsing_mode,
            )
            .map_err(Error::from)?;
            merged_entries.reserve(parsed.len());
            by_key.reserve(parsed.len());

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

        let mut resolver = SubstitutionResolver::new(
            entries,
            &self.target,
            self.override_existing,
            self.key_parsing_mode,
        );
        for entry in entries.iter_mut() {
            entry.value = resolver.resolve_entry(&entry.key);
        }
    }

    fn effective_paths(&self) -> Result<Vec<PathBuf>, Error> {
        let requested_paths = if self.paths.is_empty() {
            vec![PathBuf::from(".env")]
        } else {
            self.paths.clone()
        };

        if !self.search_upward {
            return Ok(requested_paths);
        }

        let start_dir = std::env::current_dir()?;
        Ok(resolve_paths_upward_from(&start_dir, &requested_paths))
    }

    fn logging_enabled(&self) -> bool {
        self.verbose && !self.quiet
    }

    fn log(&self, message: &str) {
        if self.logging_enabled() {
            eprintln!("dotenvor: {message}");
        }
    }
}

impl Default for EnvLoader {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            encoding: Encoding::Utf8,
            override_existing: false,
            key_parsing_mode: KeyParsingMode::Strict,
            search_upward: false,
            substitution_mode: SubstitutionMode::Disabled,
            verbose: false,
            quiet: false,
            target: TargetEnv::Process,
        }
    }
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<&str, Error> {
    match encoding {
        Encoding::Utf8 => Ok(std::str::from_utf8(bytes)?),
    }
}

fn resolve_paths_upward_from(start_dir: &Path, requested_paths: &[PathBuf]) -> Vec<PathBuf> {
    requested_paths
        .iter()
        .map(|requested| resolve_upward_path(start_dir, requested))
        .collect()
}

fn resolve_upward_path(start_dir: &Path, requested: &Path) -> PathBuf {
    if requested.is_absolute() {
        return requested.to_path_buf();
    }

    let fallback = start_dir.join(requested);
    let mut current = Some(start_dir);
    while let Some(dir) = current {
        let candidate = dir.join(requested);
        if candidate.is_file() {
            return candidate;
        }
        current = dir.parent();
    }

    fallback
}

struct SubstitutionResolver<'a> {
    raw_values: HashMap<String, String>,
    resolved_values: HashMap<String, String>,
    target: &'a TargetEnv,
    override_existing: bool,
    key_parsing_mode: KeyParsingMode,
}

impl<'a> SubstitutionResolver<'a> {
    fn new(
        entries: &[Entry],
        target: &'a TargetEnv,
        override_existing: bool,
        key_parsing_mode: KeyParsingMode,
    ) -> Self {
        let raw_values = entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.value.clone()))
            .collect();

        Self {
            raw_values,
            resolved_values: HashMap::new(),
            target,
            override_existing,
            key_parsing_mode,
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
        let expanded = expand_template(&raw_value, self.key_parsing_mode, |name, token| {
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

fn expand_template<F>(input: &str, key_parsing_mode: KeyParsingMode, mut resolve: F) -> String
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

        let Some((name_start, name_end, token_end)) =
            parse_placeholder(input, idx, key_parsing_mode)
        else {
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

fn parse_placeholder(
    input: &str,
    start: usize,
    key_parsing_mode: KeyParsingMode,
) -> Option<(usize, usize, usize)> {
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
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| is_braced_var_char(byte, key_parsing_mode))
        {
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

fn is_braced_var_char(byte: u8, key_parsing_mode: KeyParsingMode) -> bool {
    match key_parsing_mode {
        KeyParsingMode::Strict => {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || byte == b'-'
        }
        KeyParsingMode::Permissive => is_valid_permissive_key_byte(byte),
    }
}

fn is_valid_permissive_key_byte(byte: u8) -> bool {
    byte.is_ascii() && (b'!'..=b'~').contains(&byte) && byte != b'='
}

fn is_unbraced_var_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_unbraced_var_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use super::{EnvLoader, resolve_upward_path};
    use crate::model::KeyParsingMode;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn logging_disabled_by_default() {
        let loader = EnvLoader::new();
        assert!(!loader.logging_enabled());
        assert!(!loader.search_upward);
    }

    #[test]
    fn verbose_enables_logging() {
        let loader = EnvLoader::new().verbose(true);
        assert!(loader.logging_enabled());
    }

    #[test]
    fn quiet_overrides_verbose() {
        let loader = EnvLoader::new().verbose(true).quiet(true);
        assert!(!loader.logging_enabled());
    }

    #[test]
    fn search_upward_builder_sets_flag() {
        let loader = EnvLoader::new().search_upward(true);
        assert!(loader.search_upward);
    }

    #[test]
    fn key_parsing_mode_builder_sets_flag() {
        let loader = EnvLoader::new().key_parsing_mode(KeyParsingMode::Permissive);
        assert_eq!(loader.key_parsing_mode, KeyParsingMode::Permissive);
    }

    #[test]
    fn resolve_upward_path_uses_nearest_ancestor() {
        let root = make_temp_dir("resolve-upward-nearest");
        let parent = root.join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("failed to create child dir");

        let root_file = root.join(".env");
        let parent_file = parent.join(".env");
        std::fs::write(&root_file, "ROOT=1\n").expect("failed to write root file");
        std::fs::write(&parent_file, "PARENT=1\n").expect("failed to write parent file");

        let resolved = resolve_upward_path(&child, Path::new(".env"));
        assert_eq!(resolved, parent_file);
    }

    #[test]
    fn resolve_upward_path_returns_local_candidate_when_missing() {
        let root = make_temp_dir("resolve-upward-missing");
        let child = root.join("child");
        std::fs::create_dir_all(&child).expect("failed to create child dir");

        let resolved = resolve_upward_path(&child, Path::new(".env"));
        assert_eq!(resolved, child.join(".env"));
    }

    #[test]
    fn resolve_upward_path_keeps_absolute_paths() {
        let root = make_temp_dir("resolve-upward-absolute");
        let absolute = root.join(".env");
        std::fs::write(&absolute, "ABS=1\n").expect("failed to write absolute file");
        let unrelated = root.join("unrelated");
        std::fs::create_dir_all(&unrelated).expect("failed to create unrelated dir");

        let resolved = resolve_upward_path(&unrelated, &absolute);
        assert_eq!(resolved, absolute);
    }

    fn make_temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        path.push(format!(
            "dotenvor-loader-tests-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }
}
