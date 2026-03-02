use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn load_attribute_resolves_renamed_dependency_path() {
    let dir = make_temp_dir("macro-renamed-dep");
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("failed to create src dir");

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    write_file(
        &dir.join("Cargo.toml"),
        &format!(
            r#"[package]
name = "dotenvor-renamed-dep-smoke"
version = "0.0.0"
edition = "2024"

[dependencies]
denv = {{ package = "dotenvor", path = '{manifest_dir}' }}
"#
        ),
    );
    write_file(
        &src_dir.join("main.rs"),
        r#"use denv::load;

#[load(required = false)]
fn boot() -> Result<(), denv::Error> {
    Ok(())
}

fn main() {
    let _ = unsafe { boot() };
}
"#,
    );

    let output = Command::new("cargo")
        .args(["check", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .output()
        .expect("failed to run cargo check");

    assert!(
        output.status.success(),
        "expected renamed dependency macro usage to compile: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
