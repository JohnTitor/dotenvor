#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn run_loads_default_dotenv_file() {
    let dir = make_temp_dir("cli-default");
    write_file(&dir.join(".env"), "DOTENVOR_CLI_DEFAULT=from_default\n");

    let output = run_dotenv(
        &dir,
        &["run", "--", "printenv", "DOTENVOR_CLI_DEFAULT"],
        None,
    );

    assert_success(&output);
    assert_eq!(stdout_trimmed(&output), "from_default");
}

#[test]
fn run_uses_last_file_precedence_for_selected_files() {
    let dir = make_temp_dir("cli-precedence");
    write_file(&dir.join(".env.base"), "DOTENVOR_CLI_PRECEDENCE=base\n");
    write_file(&dir.join(".env.local"), "DOTENVOR_CLI_PRECEDENCE=local\n");

    let output = run_dotenv(
        &dir,
        &[
            "run",
            "-f",
            ".env.base,.env.local",
            "--",
            "printenv",
            "DOTENVOR_CLI_PRECEDENCE",
        ],
        None,
    );

    assert_success(&output);
    assert_eq!(stdout_trimmed(&output), "local");
}

#[test]
fn run_override_flag_controls_existing_environment_precedence() {
    let dir = make_temp_dir("cli-override");
    write_file(&dir.join(".env"), "DOTENVOR_CLI_OVERRIDE=from_file\n");

    let without_override = run_dotenv(
        &dir,
        &["run", "--", "printenv", "DOTENVOR_CLI_OVERRIDE"],
        Some(("DOTENVOR_CLI_OVERRIDE", "from_env")),
    );
    assert_success(&without_override);
    assert_eq!(stdout_trimmed(&without_override), "from_env");

    let with_override = run_dotenv(
        &dir,
        &["run", "-o", "--", "printenv", "DOTENVOR_CLI_OVERRIDE"],
        Some(("DOTENVOR_CLI_OVERRIDE", "from_env")),
    );
    assert_success(&with_override);
    assert_eq!(stdout_trimmed(&with_override), "from_file");
}

#[test]
fn run_ignore_missing_skips_missing_selected_files() {
    let dir = make_temp_dir("cli-ignore-missing");
    write_file(&dir.join(".env.real"), "DOTENVOR_CLI_IGNORE=loaded\n");

    let output = run_dotenv(
        &dir,
        &[
            "run",
            "--ignore-missing",
            "-f",
            "missing.env,.env.real",
            "--",
            "printenv",
            "DOTENVOR_CLI_IGNORE",
        ],
        None,
    );

    assert_success(&output);
    assert_eq!(stdout_trimmed(&output), "loaded");
}

#[test]
fn run_without_ignore_missing_fails_when_selected_file_is_missing() {
    let dir = make_temp_dir("cli-required");

    let output = run_dotenv(
        &dir,
        &[
            "run",
            "-f",
            "missing.env,.env.real",
            "--",
            "printenv",
            "DOTENVOR_CLI_REQUIRED",
        ],
        None,
    );

    assert!(
        !output.status.success(),
        "expected missing file to fail: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_search_upward_finds_parent_file_when_requested() {
    let dir = make_temp_dir("cli-search-upward");
    let parent = dir.join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).expect("failed to create nested directories");
    write_file(&parent.join(".env"), "DOTENVOR_CLI_UPWARD=from_parent\n");

    let output = run_dotenv(
        &child,
        &[
            "run",
            "--search-upward",
            "--",
            "printenv",
            "DOTENVOR_CLI_UPWARD",
        ],
        None,
    );

    assert_success(&output);
    assert_eq!(stdout_trimmed(&output), "from_parent");
}

#[test]
fn run_expand_fails_when_inherited_env_value_is_not_utf8() {
    let dir = make_temp_dir("cli-expand-non-utf8");
    write_file(
        &dir.join(".env"),
        "DOTENVOR_CLI_EXPAND_RESULT=${DOTENVOR_CLI_PARENT_NON_UTF8}\n",
    );

    let mut command = Command::new(dotenv_bin());
    command.current_dir(&dir).args([
        "run",
        "--expand",
        "--",
        "printenv",
        "DOTENVOR_CLI_EXPAND_RESULT",
    ]);
    command.env(
        "DOTENVOR_CLI_PARENT_NON_UTF8",
        OsString::from_vec(vec![0x66, 0x80, 0x67]),
    );
    let output = command.output().expect("failed to run dotenv binary");

    assert!(
        !output.status.success(),
        "expected failure when expansion reads non-UTF-8 env value: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DOTENVOR_CLI_PARENT_NON_UTF8"),
        "expected offending key in stderr: {stderr:?}"
    );
    assert!(
        stderr.contains("not valid UTF-8"),
        "expected UTF-8 validation error in stderr: {stderr:?}"
    );
}

fn run_dotenv(dir: &Path, args: &[&str], env_pair: Option<(&str, &str)>) -> Output {
    let mut command = Command::new(dotenv_bin());
    command.current_dir(dir).args(args);
    if let Some((key, value)) = env_pair {
        command.env(key, value);
    }
    command.output().expect("failed to run dotenv binary")
}

fn stdout_trimmed(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn dotenv_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_dotenv").map(PathBuf::from) {
        return path;
    }

    let mut path = std::env::current_exe().expect("failed to resolve current test executable");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }

    let candidate = path.join("dotenv");
    if candidate.is_file() {
        return candidate;
    }

    let candidate = path.join("dotenv.exe");
    if candidate.is_file() {
        return candidate;
    }

    panic!("could not locate built dotenv binary");
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
    std::fs::write(path, content).expect("failed to write fixture file");
}
