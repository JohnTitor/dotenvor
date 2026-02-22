use std::collections::BTreeMap;

/// Destination for loaded environment variables.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TargetEnv {
    /// Apply entries to the current process environment.
    #[default]
    Process,
    /// Apply entries to an in-memory map.
    Memory(BTreeMap<String, String>),
}

impl TargetEnv {
    pub fn memory() -> Self {
        Self::Memory(BTreeMap::new())
    }

    pub fn as_memory(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            Self::Memory(map) => Some(map),
            Self::Process => None,
        }
    }

    pub fn as_memory_mut(&mut self) -> Option<&mut BTreeMap<String, String>> {
        match self {
            Self::Memory(map) => Some(map),
            Self::Process => None,
        }
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Process => std::env::var_os(key).is_some(),
            Self::Memory(map) => map.contains_key(key),
        }
    }

    pub(crate) fn get_var(&self, key: &str) -> Option<String> {
        match self {
            Self::Process => {
                std::env::var_os(key).map(|value| value.to_string_lossy().into_owned())
            }
            Self::Memory(map) => map.get(key).cloned(),
        }
    }

    pub(crate) fn set_var(&mut self, key: &str, value: &str) {
        match self {
            Self::Process => unsafe { std::env::set_var(key, value) },
            Self::Memory(map) => {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
    }
}
