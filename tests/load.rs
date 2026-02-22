use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dotenvor::{EnvLoader, Error, ParseErrorKind, SubstitutionMode, TargetEnv};

#[test]
fn override_existing_false_skips_existing_values() {
    let dir = make_temp_dir("override-false");
    let file = dir.join(".env");
    write_file(&file, "A=from_file\nB=2\n");

    let mut initial = BTreeMap::new();
    initial.insert("A".to_string(), "existing".to_string());

    let mut loader = EnvLoader::new()
        .path(&file)
        .target(TargetEnv::Memory(initial))
        .override_existing(false);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.files_read, 1);
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 1);

    let target = loader.target_env();
    let map = target.as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "existing");
    assert_eq!(map.get("B").expect("B should exist"), "2");
}

#[test]
fn override_existing_true_replaces_values() {
    let dir = make_temp_dir("override-true");
    let file = dir.join(".env");
    write_file(&file, "A=from_file\n");

    let mut initial = BTreeMap::new();
    initial.insert("A".to_string(), "existing".to_string());

    let mut loader = EnvLoader::new()
        .path(&file)
        .target(TargetEnv::Memory(initial))
        .override_existing(true);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 0);

    let target = loader.target_env();
    let map = target.as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "from_file");
}

#[test]
fn multi_file_load_uses_last_file_precedence() {
    let dir = make_temp_dir("precedence");
    let first = dir.join(".env.base");
    let second = dir.join(".env.local");
    write_file(&first, "A=base\nB=base\n");
    write_file(&second, "B=local\nC=local\n");

    let mut loader = EnvLoader::new()
        .paths([first, second])
        .target(TargetEnv::memory());

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.files_read, 2);
    assert_eq!(report.loaded, 3);
    assert_eq!(report.skipped_existing, 0);

    let target = loader.target_env();
    let map = target.as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "base");
    assert_eq!(map.get("B").expect("B should exist"), "local");
    assert_eq!(map.get("C").expect("C should exist"), "local");
}

#[test]
fn missing_file_returns_io_error() {
    let dir = make_temp_dir("missing");
    let missing = dir.join("missing.env");

    let mut loader = EnvLoader::new().path(missing);
    let err = loader.load().expect_err("expected I/O error");

    match err {
        Error::Io(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn malformed_file_returns_parse_error() {
    let dir = make_temp_dir("malformed");
    let file = dir.join(".env");
    write_file(&file, "A=ok\nBAD LINE\n");

    let mut loader = EnvLoader::new().path(file);
    let err = loader.load().expect_err("expected parse error");

    match err {
        Error::Parse(parse_err) => assert_eq!(parse_err.kind, ParseErrorKind::InvalidSyntax),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn substitution_expands_chained_and_forward_references() {
    let dir = make_temp_dir("substitution-forward");
    let file = dir.join(".env");
    write_file(&file, "A=$B\nB=${C}\nC=value\n");

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .substitution_mode(SubstitutionMode::Expand);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.loaded, 3);
    assert_eq!(report.skipped_existing, 0);

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "value");
    assert_eq!(map.get("B").expect("B should exist"), "value");
    assert_eq!(map.get("C").expect("C should exist"), "value");
}

#[test]
fn substitution_uses_target_environment_for_missing_values() {
    let dir = make_temp_dir("substitution-target-fallback");
    let file = dir.join(".env");
    write_file(&file, "OUT=${BASE}/bin\n");

    let mut initial = BTreeMap::new();
    initial.insert("BASE".to_string(), "/opt/app".to_string());

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::Memory(initial))
        .substitution_mode(SubstitutionMode::Expand);

    loader.load().expect("load should succeed");

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("OUT").expect("OUT should exist"), "/opt/app/bin");
}

#[test]
fn substitution_respects_override_existing_false() {
    let dir = make_temp_dir("substitution-override");
    let file = dir.join(".env");
    write_file(&file, "A=file\nB=${A}\n");

    let mut initial = BTreeMap::new();
    initial.insert("A".to_string(), "existing".to_string());

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::Memory(initial))
        .override_existing(false)
        .substitution_mode(SubstitutionMode::Expand);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 1);

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "existing");
    assert_eq!(map.get("B").expect("B should exist"), "existing");
}

#[test]
fn substitution_preserves_unknown_placeholders() {
    let dir = make_temp_dir("substitution-unknown");
    let file = dir.join(".env");
    write_file(&file, "A=prefix-${MISSING}-$OTHER-suffix\n");

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .substitution_mode(SubstitutionMode::Expand);

    loader.load().expect("load should succeed");

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(
        map.get("A").expect("A should exist"),
        "prefix-${MISSING}-$OTHER-suffix"
    );
}

#[test]
fn search_upward_true_finds_parent_file() {
    let dir = make_temp_dir("search-upward-true");
    let parent = dir.join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).expect("failed to create child dir");
    write_file(&parent.join(".env"), "A=upward\n");

    let (report, target) = with_current_dir(&child, || {
        let mut loader = EnvLoader::new()
            .search_upward(true)
            .target(TargetEnv::memory());
        let report = loader.load().expect("load should succeed");
        let target = loader.into_target();
        (report, target)
    });

    assert_eq!(report.files_read, 1);
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 0);
    let map = target.as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "upward");
}

#[test]
fn search_upward_false_does_not_walk_parents() {
    let dir = make_temp_dir("search-upward-false");
    let parent = dir.join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).expect("failed to create child dir");
    write_file(&parent.join(".env"), "A=upward\n");

    let err = with_current_dir(&child, || {
        let mut loader = EnvLoader::new().target(TargetEnv::memory());
        loader.load().expect_err("expected I/O error")
    });

    match err {
        Error::Io(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}

fn make_temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    path.push(format!("dotenvor-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&path).expect("failed to create temp dir");
    path
}

fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("failed to write test file");
}

fn with_current_dir<R>(dir: &Path, f: impl FnOnce() -> R) -> R {
    let _lock = cwd_lock().lock().expect("cwd lock should not be poisoned");
    let _guard = CurrentDirGuard::enter(dir);
    f()
}

fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().expect("failed to read current dir");
        std::env::set_current_dir(dir).expect("failed to set current dir");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original).expect("failed to restore current dir");
    }
}
