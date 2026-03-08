use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dotenvor::{Error, load};

#[load(path = "tests/fixtures/macro-basic.env", override_existing = false)]
fn load_without_override() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/macro-basic.env", override_existing = true)]
fn load_with_override() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/not-found.env", required = false)]
fn load_missing_optional() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/not-found.env", required = true)]
fn load_missing_required() -> Result<(), Error> {
    Ok(())
}

#[load(
    path = ".env.macro-upward",
    search_upward = true,
    override_existing = true
)]
fn load_with_search_upward() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/macro-async.env", override_existing = true)]
async fn load_plain_async() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/macro-async.env", override_existing = true)]
#[tokio::main(flavor = "current_thread")]
async fn load_before_tokio_runtime() -> Result<(), Error> {
    Ok(())
}

#[load(path = "tests/fixtures/macro-async.env", override_existing = true)]
#[tokio::main(flavor = "current_thread")]
async fn load_before_tokio_runtime_generic<T>() -> Result<(), Error>
where
    T: Default,
{
    let _ = T::default();
    Ok(())
}

#[load(path = "tests/fixtures/macro-async.env", override_existing = true)]
#[cfg_attr(all(), tokio::main(flavor = "current_thread"))]
async fn load_before_tokio_runtime_cfg_attr() -> Result<(), Error> {
    Ok(())
}

#[test]
fn load_attribute_respects_override_existing() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe {
        std::env::set_var("DOTENVOR_MACRO_OVERRIDE", "existing");
    }

    unsafe { load_without_override() }.expect("macro load should succeed");
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_OVERRIDE").expect("env var should be set"),
        "existing"
    );

    unsafe { load_with_override() }.expect("macro load should succeed");
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_OVERRIDE").expect("env var should be set"),
        "from_file"
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_OVERRIDE");
    }
}

#[test]
fn load_attribute_optional_missing_file_is_ok() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe { load_missing_optional() }.expect("optional missing file should be ignored");
}

#[test]
fn load_attribute_required_missing_file_returns_not_found() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    let err = unsafe { load_missing_required() }.expect_err("required missing file should fail");

    match err {
        Error::Io(io_err) => assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn load_attribute_search_upward_finds_parent_file() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    let dir = make_temp_dir("macro-search-upward");
    let parent = dir.join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).expect("failed to create nested directories");
    write_file(
        &parent.join(".env.macro-upward"),
        "DOTENVOR_MACRO_UPWARD=from_parent\n",
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_UPWARD");
    }

    with_current_dir(&child, || unsafe {
        load_with_search_upward().expect("macro load should succeed");
    });

    assert_eq!(
        std::env::var("DOTENVOR_MACRO_UPWARD").expect("env var should be set"),
        "from_parent"
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_UPWARD");
    }
}

#[test]
fn load_attribute_plain_async_runs_before_future_is_polled() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }

    let future = unsafe { load_plain_async() };
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_ASYNC").expect("env var should be set"),
        "from_async_file"
    );

    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime should build")
        .block_on(future)
        .expect("macro-wrapped async fn should succeed");

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }
}

#[test]
fn load_attribute_runs_before_tokio_runtime_entry() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }

    unsafe { load_before_tokio_runtime() }.expect("macro-wrapped tokio entry should succeed");
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_ASYNC").expect("env var should be set"),
        "from_async_file"
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }
}

#[test]
fn load_attribute_runs_before_cfg_attr_tokio_runtime_entry() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }

    unsafe { load_before_tokio_runtime_cfg_attr() }
        .expect("macro-wrapped cfg_attr tokio entry should succeed");
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_ASYNC").expect("env var should be set"),
        "from_async_file"
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }
}

#[test]
fn load_attribute_runtime_wrapper_forwards_generics() {
    let _guard = env_lock().lock().expect("env lock poisoned");

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }

    unsafe { load_before_tokio_runtime_generic::<()>() }
        .expect("macro-wrapped generic tokio entry should succeed");
    assert_eq!(
        std::env::var("DOTENVOR_MACRO_ASYNC").expect("env var should be set"),
        "from_async_file"
    );

    unsafe {
        std::env::remove_var("DOTENVOR_MACRO_ASYNC");
    }
}

fn env_lock() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

fn with_current_dir<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    let current = std::env::current_dir().expect("failed to read current dir");
    std::env::set_current_dir(path).expect("failed to set current dir");

    let result = f();

    std::env::set_current_dir(current).expect("failed to restore current dir");
    result
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
    std::fs::write(path, content).expect("failed to write file");
}
