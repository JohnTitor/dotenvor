use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::TargetEnv;
use crate::error::Error;
use crate::model::{Encoding, Entry, KeyParsingMode, LoadReport, SubstitutionMode};
use crate::parser::parse_str_with_source;

/// Load `.env` from the current working directory into the process environment.
///
/// # Safety
///
/// The caller must ensure no other threads concurrently read or write the
/// process environment while this function runs.
pub unsafe fn dotenv() -> Result<LoadReport, Error> {
    unsafe { from_filename(".env") }
}

/// Load a `.env` file from a specific path into the process environment.
///
/// # Safety
///
/// The caller must ensure no other threads concurrently read or write the
/// process environment while this function runs.
pub unsafe fn from_path(path: impl AsRef<Path>) -> Result<LoadReport, Error> {
    let mut loader = EnvLoader::new()
        .path(path)
        .target(unsafe { TargetEnv::process() });
    loader.load()
}

/// Load multiple `.env` files into the process environment.
///
/// # Safety
///
/// The caller must ensure no other threads concurrently read or write the
/// process environment while this function runs.
pub unsafe fn from_paths<I, P>(paths: I) -> Result<LoadReport, Error>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut loader = EnvLoader::new()
        .paths(paths)
        .target(unsafe { TargetEnv::process() });
    loader.load()
}

/// Load a dotenv file by filename into the process environment.
///
/// # Safety
///
/// The caller must ensure no other threads concurrently read or write the
/// process environment while this function runs.
pub unsafe fn from_filename(name: &str) -> Result<LoadReport, Error> {
    let mut loader = EnvLoader::new()
        .path(name)
        .search_upward(true)
        .target(unsafe { TargetEnv::process() });
    loader.load()
}

/// Builder-style dotenv loader.
///
/// `EnvLoader::new()` defaults to [`TargetEnv::memory`], which keeps values in
/// an in-memory map and avoids process-global mutation by default.
#[derive(Debug)]
pub struct EnvLoader {
    paths: Vec<PathBuf>,
    encoding: Encoding,
    required: bool,
    override_existing: bool,
    key_parsing_mode: KeyParsingMode,
    search_upward: bool,
    substitution_mode: SubstitutionMode,
    verbose: bool,
    quiet: bool,
    target: TargetEnv,
}

impl EnvLoader {
    /// Create a new loader with default settings.
    ///
    /// The default target is [`TargetEnv::memory`].
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

    /// Append paths using the common multi-environment dotenv convention.
    ///
    /// Precedence (highest to lowest):
    ///
    /// - `.env.{environment}.local`
    /// - `.env.local`
    /// - `.env.{environment}`
    /// - `.env`
    ///
    /// `dotenvor` merges files using "last file wins", so these paths are
    /// appended in reverse precedence order.
    pub fn convention(mut self, environment: impl AsRef<str>) -> Self {
        self.paths.extend(convention_paths(environment.as_ref()));
        self
    }

    /// Set input file decoding.
    ///
    /// Defaults to [`Encoding::Utf8`]. Use [`Encoding::Latin1`] for
    /// ISO-8859-1-compatible dotenv files.
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Set whether missing files should return an error.
    ///
    /// Defaults to `true`. When set to `false`, missing files are skipped.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
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
            self.target.set_var(&entry.key, &entry.value)?;
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
            if let Some(parsed) = self.read_entries(path, include_source)? {
                return Ok((parsed, 1));
            }
            return Ok((Vec::new(), 0));
        }

        let mut merged_entries = Vec::new();
        let mut by_key = HashMap::<String, usize>::new();
        let mut files_read = 0usize;

        for path in paths {
            let Some(parsed) = self.read_entries(&path, include_source)? else {
                continue;
            };
            files_read += 1;
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

    fn read_entries(&self, path: &Path, include_source: bool) -> Result<Option<Vec<Entry>>, Error> {
        self.log(&format!("reading {}", path.display()));
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && !self.required => {
                self.log(&format!("skipping missing {}", path.display()));
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };
        let content = decode(&bytes, self.encoding)?;
        let parsed = parse_str_with_source(
            content.as_ref(),
            include_source.then_some(path),
            self.key_parsing_mode,
            self.substitution_mode == SubstitutionMode::Expand,
        )
        .map_err(Error::from)?;
        Ok(Some(parsed))
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
            required: true,
            override_existing: false,
            key_parsing_mode: KeyParsingMode::Strict,
            search_upward: false,
            substitution_mode: SubstitutionMode::Disabled,
            verbose: false,
            quiet: false,
            target: TargetEnv::memory(),
        }
    }
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<Cow<'_, str>, Error> {
    match encoding {
        Encoding::Utf8 => Ok(Cow::Borrowed(std::str::from_utf8(bytes)?)),
        Encoding::Latin1 => Ok(Cow::Owned(decode_latin1(bytes))),
    }
}

fn decode_latin1(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len());
    for &byte in bytes {
        output.push(char::from(byte));
    }
    output
}

fn convention_paths(environment: &str) -> Vec<PathBuf> {
    let environment = environment.trim();
    let mut paths = Vec::with_capacity(4);

    push_unique_path(&mut paths, PathBuf::from(".env"));
    if !environment.is_empty() {
        push_unique_path(&mut paths, PathBuf::from(format!(".env.{environment}")));
    }
    push_unique_path(&mut paths, PathBuf::from(".env.local"));
    if !environment.is_empty() {
        push_unique_path(
            &mut paths,
            PathBuf::from(format!(".env.{environment}.local")),
        );
    }

    paths
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
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
        let expanded =
            expand_template(&raw_value, self.key_parsing_mode, |name, token, default| {
                self.resolve_placeholder(name, token, default, stack)
            });
        stack.pop();

        self.resolved_values
            .insert(key.to_owned(), expanded.clone());
        expanded
    }

    fn resolve_placeholder(
        &mut self,
        name: &str,
        token: &str,
        default: Option<&str>,
        stack: &mut Vec<String>,
    ) -> String {
        if stack.iter().any(|item| item == name) {
            return default.unwrap_or(token).to_owned();
        }

        let resolved = if self.raw_values.contains_key(name) {
            Some(self.resolve_key(name, stack))
        } else {
            self.target.get_var(name)
        };

        if let Some(value) = resolved {
            if default.is_some() && value.is_empty() {
                return default.unwrap_or_default().to_owned();
            }
            return value;
        }

        default.unwrap_or(token).to_owned()
    }
}

fn expand_template<F>(input: &str, key_parsing_mode: KeyParsingMode, mut resolve: F) -> String
where
    F: FnMut(&str, &str, Option<&str>) -> String,
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

        if idx > 0 && bytes[idx - 1] == b'\\' {
            out.push_str(&input[cursor..idx - 1]);
            out.push('$');
            cursor = idx + 1;
            idx += 1;
            continue;
        }

        let Some(placeholder) = parse_placeholder(input, idx, key_parsing_mode) else {
            idx += 1;
            continue;
        };

        let name = &input[placeholder.name_start..placeholder.name_end];
        let token = &input[idx..placeholder.token_end];
        let default = placeholder.default.map(|(start, end)| &input[start..end]);

        out.push_str(&input[cursor..idx]);
        out.push_str(&resolve(name, token, default));

        cursor = placeholder.token_end;
        idx = placeholder.token_end;
    }

    out.push_str(&input[cursor..]);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placeholder {
    name_start: usize,
    name_end: usize,
    default: Option<(usize, usize)>,
    token_end: usize,
}

fn parse_placeholder(
    input: &str,
    start: usize,
    key_parsing_mode: KeyParsingMode,
) -> Option<Placeholder> {
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
        let token_end = end + 1;

        if key_parsing_mode == KeyParsingMode::Strict {
            let inner = &input[name_start..end];
            if let Some(operator_idx) = inner.find(":-") {
                let name_end = name_start + operator_idx;
                let default_start = name_end + 2;
                let name = &input[name_start..name_end];
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| is_braced_var_char(byte, key_parsing_mode))
                {
                    return None;
                }

                return Some(Placeholder {
                    name_start,
                    name_end,
                    default: Some((default_start, end)),
                    token_end,
                });
            }
        }

        let name_end = end;
        let name = &input[name_start..name_end];
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| is_braced_var_char(byte, key_parsing_mode))
        {
            return None;
        }

        return Some(Placeholder {
            name_start,
            name_end,
            default: None,
            token_end,
        });
    }

    let name_start = start + 1;
    if !is_unbraced_var_start(bytes[name_start]) {
        return None;
    }

    let mut name_end = name_start + 1;
    while name_end < bytes.len() && is_unbraced_var_char(bytes[name_end]) {
        name_end += 1;
    }

    Some(Placeholder {
        name_start,
        name_end,
        default: None,
        token_end: name_end,
    })
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
    use super::{EnvLoader, convention_paths, resolve_upward_path};
    use crate::model::KeyParsingMode;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn logging_disabled_by_default() {
        let loader = EnvLoader::new();
        assert!(!loader.logging_enabled());
        assert!(loader.required);
        assert!(!loader.search_upward);
        assert!(loader.target_env().as_memory().is_some());
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
    fn required_builder_sets_flag() {
        let loader = EnvLoader::new().required(false);
        assert!(!loader.required);
    }

    #[test]
    fn key_parsing_mode_builder_sets_flag() {
        let loader = EnvLoader::new().key_parsing_mode(KeyParsingMode::Permissive);
        assert_eq!(loader.key_parsing_mode, KeyParsingMode::Permissive);
    }

    #[test]
    fn convention_builder_sets_common_stack_paths() {
        let loader = EnvLoader::new().convention("development");
        assert_eq!(
            loader.paths,
            vec![
                PathBuf::from(".env"),
                PathBuf::from(".env.development"),
                PathBuf::from(".env.local"),
                PathBuf::from(".env.development.local"),
            ]
        );
    }

    #[test]
    fn convention_builder_handles_blank_environment_name() {
        let loader = EnvLoader::new().convention("   ");
        assert_eq!(
            loader.paths,
            vec![PathBuf::from(".env"), PathBuf::from(".env.local")]
        );
    }

    #[test]
    fn convention_paths_avoid_duplicates() {
        assert_eq!(
            convention_paths("local"),
            vec![
                PathBuf::from(".env"),
                PathBuf::from(".env.local"),
                PathBuf::from(".env.local.local"),
            ]
        );
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
