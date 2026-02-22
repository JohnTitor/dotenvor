use std::collections::BTreeMap;

/// Destination for loaded environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetEnv {
    /// Apply entries to the current process environment.
    ///
    /// Construct this target with [`TargetEnv::process`].
    ///
    /// This writes through [`std::env::set_var`], which mutates global process
    /// state and is not thread-safe for concurrent environment access.
    Process(ProcessTarget),
    /// Apply entries to an in-memory map.
    Memory(BTreeMap<String, String>),
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTarget(());

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
        Self::Process(ProcessTarget(()))
    }

    /// Create an in-memory environment target.
    ///
    /// Use this to avoid mutating the process environment.
    pub fn memory() -> Self {
        Self::Memory(BTreeMap::new())
    }

    pub fn as_memory(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            Self::Memory(map) => Some(map),
            Self::Process(_) => None,
        }
    }

    pub fn as_memory_mut(&mut self) -> Option<&mut BTreeMap<String, String>> {
        match self {
            Self::Memory(map) => Some(map),
            Self::Process(_) => None,
        }
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Process(_) => std::env::var_os(key).is_some(),
            Self::Memory(map) => map.contains_key(key),
        }
    }

    pub(crate) fn get_var(&self, key: &str) -> Option<String> {
        match self {
            Self::Process(_) => {
                std::env::var_os(key).map(|value| value.to_string_lossy().into_owned())
            }
            Self::Memory(map) => map.get(key).cloned(),
        }
    }

    pub(crate) fn set_var(&mut self, key: &str, value: &str) {
        match self {
            Self::Process(_) => unsafe { std::env::set_var(key, value) },
            Self::Memory(map) => {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
    }
}
