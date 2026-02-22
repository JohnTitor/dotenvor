use std::collections::BTreeMap;

/// Destination for loaded environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEnv {
    kind: TargetEnvKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        match &self.kind {
            TargetEnvKind::Process => std::env::var_os(key).is_some(),
            TargetEnvKind::Memory(map) => map.contains_key(key),
        }
    }

    pub(crate) fn get_var(&self, key: &str) -> Option<String> {
        match &self.kind {
            TargetEnvKind::Process => {
                std::env::var_os(key).map(|value| value.to_string_lossy().into_owned())
            }
            TargetEnvKind::Memory(map) => map.get(key).cloned(),
        }
    }

    pub(crate) fn set_var(&mut self, key: &str, value: &str) {
        match &mut self.kind {
            TargetEnvKind::Process => unsafe { std::env::set_var(key, value) },
            TargetEnvKind::Memory(map) => {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
    }
}
