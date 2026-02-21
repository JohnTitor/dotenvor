use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{Criterion, criterion_group, criterion_main};
use dotenvor::{EnvLoader, TargetEnv};

fn bench_load(c: &mut Criterion) {
    let dir = make_temp_dir("bench-load");
    let env_path = dir.join(".env");
    write_file(&env_path, &make_env_content(2_000));

    c.bench_function("load_in_memory", |b| {
        b.iter(|| {
            let mut loader = EnvLoader::new()
                .path(&env_path)
                .target(TargetEnv::memory())
                .override_existing(true);
            loader.load().expect("load should succeed")
        });
    });
}

fn make_env_content(entries: usize) -> String {
    let mut content = String::with_capacity(entries * 16);
    for idx in 0..entries {
        content.push_str("KEY_");
        content.push_str(&idx.to_string());
        content.push('=');
        content.push_str("value");
        content.push('\n');
    }
    content
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

criterion_group!(benches, bench_load);
criterion_main!(benches);
