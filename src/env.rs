use std::collections::BTreeMap;
use std::io::{Error as IoError, ErrorKind};

/// Destination for loaded environment variables.
///
/// This type intentionally does not implement [`Clone`]. Cloning a process
/// target would duplicate access to process-global mutation behind safe APIs,
/// making the `TargetEnv::process` safety contract much easier to violate.
#[derive(Debug, PartialEq, Eq)]
pub struct TargetEnv {
    kind: TargetEnvKind,
}

#[derive(Debug, PartialEq, Eq)]
enum TargetEnvKind {
    /// Apply entries to the current process environment.
    ///
    /// This writes through [`std::env::set_var`], which mutates global process
    /// state and is not thread-safe for concurrent environment access.
    Process,
    /// Apply entries to an in-memory map.
    Memory(BTreeMap<String, String>),
}

impl Default for TargetEnv {
    fn default() -> Self {
        Self::memory()
    }
}

impl TargetEnv {
    /// Create a process-environment target.
    ///
    /// # Safety
    ///
    /// The caller must ensure no other threads concurrently read or write the
    /// process environment for the duration of operations that may mutate this
    /// target.
    pub unsafe fn process() -> Self {
        Self {
            kind: TargetEnvKind::Process,
        }
    }

    /// Create an in-memory environment target.
    ///
    /// Use this to avoid mutating the process environment.
    pub fn memory() -> Self {
        Self::from_memory(BTreeMap::new())
    }

    /// Create an in-memory environment target from an existing map.
    pub fn from_memory(map: BTreeMap<String, String>) -> Self {
        Self {
            kind: TargetEnvKind::Memory(map),
        }
    }

    pub fn as_memory(&self) -> Option<&BTreeMap<String, String>> {
        match &self.kind {
            TargetEnvKind::Memory(map) => Some(map),
            TargetEnvKind::Process => None,
        }
    }

    pub fn as_memory_mut(&mut self) -> Option<&mut BTreeMap<String, String>> {
        match &mut self.kind {
            TargetEnvKind::Memory(map) => Some(map),
            TargetEnvKind::Process => None,
        }
    }

    pub fn into_memory(self) -> Option<BTreeMap<String, String>> {
        match self.kind {
            TargetEnvKind::Memory(map) => Some(map),
            TargetEnvKind::Process => None,
        }
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        match &self.kind {
            TargetEnvKind::Process => std::env::var_os(key).is_some(),
            TargetEnvKind::Memory(map) => memory_contains_key(map, key),
        }
    }

    pub(crate) fn get_var(&self, key: &str) -> std::io::Result<Option<String>> {
        match &self.kind {
            TargetEnvKind::Process => match std::env::var(key) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(std::env::VarError::NotUnicode(_)) => Err(IoError::new(
                    ErrorKind::InvalidData,
                    format!(
                        "process environment variable `{key}` value is not valid UTF-8; variable expansion requires UTF-8 environment data"
                    ),
                )),
            },
            TargetEnvKind::Memory(map) => Ok(memory_get(map, key).cloned()),
        }
    }

    pub(crate) fn set_var(&mut self, key: &str, value: &str) -> std::io::Result<()> {
        match &mut self.kind {
            TargetEnvKind::Process => {
                validate_process_env_pair(key, value)?;
                unsafe { std::env::set_var(key, value) };
                Ok(())
            }
            TargetEnvKind::Memory(map) => {
                memory_insert(map, key, value);
                Ok(())
            }
        }
    }
}

pub(crate) fn comparable_env_key(key: &str) -> String {
    #[cfg(windows)]
    {
        key.to_uppercase()
    }

    #[cfg(not(windows))]
    {
        key.to_owned()
    }
}

#[cfg(windows)]
fn memory_entry<'a>(
    map: &'a BTreeMap<String, String>,
    key: &str,
) -> Option<(&'a String, &'a String)> {
    let comparable_key = comparable_env_key(key);
    map.iter()
        .find(|(existing_key, _)| comparable_env_key(existing_key) == comparable_key)
}

fn memory_contains_key(map: &BTreeMap<String, String>, key: &str) -> bool {
    #[cfg(not(windows))]
    {
        map.contains_key(key)
    }

    #[cfg(windows)]
    {
        memory_entry(map, key).is_some()
    }
}

fn memory_get<'a>(map: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    #[cfg(not(windows))]
    {
        map.get(key)
    }

    #[cfg(windows)]
    {
        memory_entry(map, key).map(|(_, value)| value)
    }
}

fn memory_insert(map: &mut BTreeMap<String, String>, key: &str, value: &str) {
    #[cfg(not(windows))]
    {
        map.insert(key.to_owned(), value.to_owned());
    }

    #[cfg(windows)]
    {
        if let Some(existing_key) =
            memory_entry(map, key).map(|(existing_key, _)| existing_key.clone())
        {
            map.remove(&existing_key);
        }
        map.insert(key.to_owned(), value.to_owned());
    }
}

fn validate_process_env_pair(key: &str, value: &str) -> std::io::Result<()> {
    if key.contains('\0') || key.contains('=') {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!("invalid environment variable name `{key}`"),
        ));
    }
    if value.contains('\0') {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!("environment variable `{key}` value contains NUL byte"),
        ));
    }
    Ok(())
}
