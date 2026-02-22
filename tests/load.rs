use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dotenvor::{EnvLoader, Error, KeyParsingMode, ParseErrorKind, SubstitutionMode, TargetEnv};

#[test]
fn override_existing_false_skips_existing_values() {
    let dir = make_temp_dir("override-false");
    let file = dir.join(".env");
    write_file(&file, "A=from_file\nB=2\n");

    let mut initial = BTreeMap::new();
    initial.insert("A".to_string(), "existing".to_string());

    let mut loader = EnvLoader::new()
        .path(&file)
        .target(TargetEnv::from_memory(initial))
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
        .target(TargetEnv::from_memory(initial))
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
fn convention_stack_uses_expected_precedence() {
    let dir = make_temp_dir("convention-precedence");
    write_file(&dir.join(".env"), "ORDER=env\nBASE_ONLY=1\n");
    write_file(
        &dir.join(".env.development"),
        "ORDER=development\nDEVELOPMENT_ONLY=1\n",
    );
    write_file(&dir.join(".env.local"), "ORDER=local\nLOCAL_ONLY=1\n");
    write_file(
        &dir.join(".env.development.local"),
        "ORDER=development_local\nDEVELOPMENT_LOCAL_ONLY=1\n",
    );

    let (report, target) = with_current_dir(&dir, || {
        let mut loader = EnvLoader::new()
            .convention("development")
            .target(TargetEnv::memory());
        let report = loader.load().expect("load should succeed");
        let target = loader.into_target();
        (report, target)
    });

    assert_eq!(report.files_read, 4);
    assert_eq!(report.loaded, 5);
    assert_eq!(report.skipped_existing, 0);

    let map = target.as_memory().expect("memory target");
    assert_eq!(
        map.get("ORDER").expect("ORDER should exist"),
        "development_local"
    );
    assert_eq!(map.get("BASE_ONLY").expect("BASE_ONLY should exist"), "1");
    assert_eq!(
        map.get("DEVELOPMENT_ONLY")
            .expect("DEVELOPMENT_ONLY should exist"),
        "1"
    );
    assert_eq!(map.get("LOCAL_ONLY").expect("LOCAL_ONLY should exist"), "1");
    assert_eq!(
        map.get("DEVELOPMENT_LOCAL_ONLY")
            .expect("DEVELOPMENT_LOCAL_ONLY should exist"),
        "1"
    );
}

#[test]
fn convention_stack_can_skip_missing_files() {
    let dir = make_temp_dir("convention-missing");
    write_file(&dir.join(".env"), "ORDER=env\n");
    write_file(&dir.join(".env.local"), "ORDER=local\n");

    let (report, target) = with_current_dir(&dir, || {
        let mut loader = EnvLoader::new()
            .convention("development")
            .required(false)
            .target(TargetEnv::memory());
        let report = loader
            .load()
            .expect("missing convention files should be skipped");
        let target = loader.into_target();
        (report, target)
    });

    assert_eq!(report.files_read, 2);
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 0);

    let map = target.as_memory().expect("memory target");
    assert_eq!(map.get("ORDER").expect("ORDER should exist"), "local");
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
fn missing_file_is_skipped_when_not_required() {
    let dir = make_temp_dir("missing-optional");
    let missing = dir.join("missing.env");

    let mut loader = EnvLoader::new()
        .path(missing)
        .required(false)
        .target(TargetEnv::memory());
    let report = loader.load().expect("missing file should be skipped");

    assert_eq!(report.files_read, 0);
    assert_eq!(report.loaded, 0);
    assert_eq!(report.skipped_existing, 0);

    let map = loader.target_env().as_memory().expect("memory target");
    assert!(map.is_empty(), "target should remain empty");
}

#[test]
fn optional_mode_skips_missing_files_in_multi_file_load() {
    let dir = make_temp_dir("missing-optional-multi");
    let missing = dir.join("missing.env");
    let existing = dir.join("existing.env");
    write_file(&existing, "A=from_file\n");

    let mut loader = EnvLoader::new()
        .paths([missing, existing])
        .required(false)
        .target(TargetEnv::memory());
    let report = loader.load().expect("load should succeed");

    assert_eq!(report.files_read, 1);
    assert_eq!(report.loaded, 1);
    assert_eq!(report.skipped_existing, 0);

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("A").expect("A should exist"), "from_file");
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
fn strict_key_mode_rejects_extended_key_names() {
    let dir = make_temp_dir("strict-keys");
    let file = dir.join(".env");
    write_file(&file, "KEY:ONE=1\n");

    let mut loader = EnvLoader::new().path(file).target(TargetEnv::memory());
    let err = loader.load().expect_err("expected parse error");

    match err {
        Error::Parse(parse_err) => assert_eq!(parse_err.kind, ParseErrorKind::InvalidKey),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn permissive_key_mode_loads_extended_keys_and_substitutions() {
    let dir = make_temp_dir("permissive-keys");
    let file = dir.join(".env");
    write_file(&file, "KEY:ONE=one\nKEY:TWO=${KEY:ONE}11\n%TEMP%=/tmp\n");

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .key_parsing_mode(KeyParsingMode::Permissive)
        .substitution_mode(SubstitutionMode::Expand);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.loaded, 3);
    assert_eq!(report.skipped_existing, 0);

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("KEY:ONE").expect("KEY:ONE should exist"), "one");
    assert_eq!(map.get("KEY:TWO").expect("KEY:TWO should exist"), "one11");
    assert_eq!(map.get("%TEMP%").expect("%TEMP% should exist"), "/tmp");
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
        .target(TargetEnv::from_memory(initial))
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
        .target(TargetEnv::from_memory(initial))
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
fn substitution_expands_permissive_placeholders_with_punctuation_keys() {
    let dir = make_temp_dir("substitution-permissive-punctuation");
    let file = dir.join(".env");
    write_file(
        &file,
        "KEY:ONE=one\n%TEMP%=/tmp\nOUT_COLON=${KEY:ONE}11\nOUT_PERCENT=${%TEMP%}/cache\n",
    );

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .key_parsing_mode(KeyParsingMode::Permissive)
        .substitution_mode(SubstitutionMode::Expand);

    let report = loader.load().expect("load should succeed");
    assert_eq!(report.loaded, 4);
    assert_eq!(report.skipped_existing, 0);

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(
        map.get("OUT_COLON").expect("OUT_COLON should exist"),
        "one11"
    );
    assert_eq!(
        map.get("OUT_PERCENT").expect("OUT_PERCENT should exist"),
        "/tmp/cache"
    );
}

#[test]
fn substitution_expands_colon_minus_defaults_in_strict_mode() {
    let dir = make_temp_dir("substitution-colon-minus-defaults");
    let file = dir.join(".env");
    write_file(
        &file,
        "SET=from_file\n\
         EMPTY=\n\
         COLON_MINUS=${SET:-fallback}\n\
         EMPTY_COLON_MINUS=${EMPTY:-fallback}\n\
         MINUS=${SET-fallback}\n\
         COLON_PLUS=${SET:+alt}\n\
         PLUS=${SET+alt}\n\
         COLON_Q=${SET:?err}\n\
         Q=${SET?err}\n\
         MISSING_COLON_MINUS=${MISSING:-fallback}\n\
         MISSING_MINUS=${MISSING-fallback}\n\
         COMPOSITE=pre-${MISSING:-fallback}-post\n",
    );

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .substitution_mode(SubstitutionMode::Expand);

    loader.load().expect("load should succeed");

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("SET").expect("SET should exist"), "from_file");
    assert_eq!(map.get("EMPTY").expect("EMPTY should exist"), "");
    assert_eq!(
        map.get("COLON_MINUS").expect("COLON_MINUS should exist"),
        "from_file"
    );
    assert_eq!(
        map.get("EMPTY_COLON_MINUS")
            .expect("EMPTY_COLON_MINUS should exist"),
        "fallback"
    );
    assert_eq!(
        map.get("MINUS").expect("MINUS should exist"),
        "${SET-fallback}"
    );
    assert_eq!(
        map.get("COLON_PLUS").expect("COLON_PLUS should exist"),
        "${SET:+alt}"
    );
    assert_eq!(map.get("PLUS").expect("PLUS should exist"), "${SET+alt}");
    assert_eq!(
        map.get("COLON_Q").expect("COLON_Q should exist"),
        "${SET:?err}"
    );
    assert_eq!(map.get("Q").expect("Q should exist"), "${SET?err}");
    assert_eq!(
        map.get("MISSING_COLON_MINUS")
            .expect("MISSING_COLON_MINUS should exist"),
        "fallback"
    );
    assert_eq!(
        map.get("MISSING_MINUS")
            .expect("MISSING_MINUS should exist"),
        "${MISSING-fallback}"
    );
    assert_eq!(
        map.get("COMPOSITE").expect("COMPOSITE should exist"),
        "pre-fallback-post"
    );
}

#[test]
fn substitution_resolves_modifier_shaped_keys_in_permissive_mode() {
    let dir = make_temp_dir("substitution-modifier-shaped-keys");
    let file = dir.join(".env");
    write_file(
        &file,
        "VAR:-default=colon_minus\n\
         VAR-default=minus\n\
         VAR:+alt=colon_plus\n\
         VAR+alt=plus\n\
         VAR:?err=colon_question\n\
         VAR?err=question\n\
         OUT1=${VAR:-default}\n\
         OUT2=${VAR-default}\n\
         OUT3=${VAR:+alt}\n\
         OUT4=${VAR+alt}\n\
         OUT5=${VAR:?err}\n\
         OUT6=${VAR?err}\n",
    );

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .key_parsing_mode(KeyParsingMode::Permissive)
        .substitution_mode(SubstitutionMode::Expand);

    loader.load().expect("load should succeed");

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("OUT1").expect("OUT1 should exist"), "colon_minus");
    assert_eq!(map.get("OUT2").expect("OUT2 should exist"), "minus");
    assert_eq!(map.get("OUT3").expect("OUT3 should exist"), "colon_plus");
    assert_eq!(map.get("OUT4").expect("OUT4 should exist"), "plus");
    assert_eq!(
        map.get("OUT5").expect("OUT5 should exist"),
        "colon_question"
    );
    assert_eq!(map.get("OUT6").expect("OUT6 should exist"), "question");
}

#[test]
fn substitution_respects_literal_dollar_in_single_quotes_and_backslash_escapes() {
    let dir = make_temp_dir("substitution-quotes");
    let file = dir.join(".env");
    write_file(
        &file,
        "BASE=from_file\n\
         SINGLE='${BASE}'\n\
         DOUBLE=\"${BASE}\"\n\
         UNQUOTED=${BASE}\n\
         ESCAPED_UNQUOTED=\\${BASE}\n\
         ESCAPED_DOUBLE=\"\\${BASE}\"\n\
         ESCAPED_SIMPLE=\\$BASE\n",
    );

    let mut loader = EnvLoader::new()
        .path(file)
        .target(TargetEnv::memory())
        .substitution_mode(SubstitutionMode::Expand);

    loader.load().expect("load should succeed");

    let map = loader.target_env().as_memory().expect("memory target");
    assert_eq!(map.get("SINGLE").expect("SINGLE should exist"), "${BASE}");
    assert_eq!(map.get("DOUBLE").expect("DOUBLE should exist"), "from_file");
    assert_eq!(
        map.get("UNQUOTED").expect("UNQUOTED should exist"),
        "from_file"
    );
    assert_eq!(
        map.get("ESCAPED_UNQUOTED")
            .expect("ESCAPED_UNQUOTED should exist"),
        "${BASE}"
    );
    assert_eq!(
        map.get("ESCAPED_DOUBLE")
            .expect("ESCAPED_DOUBLE should exist"),
        "${BASE}"
    );
    assert_eq!(
        map.get("ESCAPED_SIMPLE")
            .expect("ESCAPED_SIMPLE should exist"),
        "$BASE"
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

#[test]
fn search_upward_false_can_skip_missing_default_file_when_not_required() {
    let dir = make_temp_dir("search-upward-false-optional");
    let parent = dir.join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).expect("failed to create child dir");
    write_file(&parent.join(".env"), "A=upward\n");

    let (report, target) = with_current_dir(&child, || {
        let mut loader = EnvLoader::new().required(false).target(TargetEnv::memory());
        let report = loader.load().expect("missing file should be skipped");
        let target = loader.into_target();
        (report, target)
    });

    assert_eq!(report.files_read, 0);
    assert_eq!(report.loaded, 0);
    assert_eq!(report.skipped_existing, 0);
    let map = target.as_memory().expect("memory target");
    assert!(map.is_empty(), "target should remain empty");
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
