use std::fmt::Write;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for size in [1_024usize, 10_240, 102_400] {
        let input = make_input(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| dotenvor::parse_str(black_box(input)).expect("parse should succeed"));
        });
    }
    group.finish();
}

fn make_input(bytes: usize) -> String {
    let mut input = String::with_capacity(bytes + 32);
    let mut idx = 0usize;
    while input.len() < bytes {
        writeln!(&mut input, "KEY_{idx}=value").expect("write to string");
        idx += 1;
    }
    input
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
