use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dotenvor::{EnvLoader, Error, ParseErrorKind, TargetEnv};

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
