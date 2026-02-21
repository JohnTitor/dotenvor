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
    let line = "KEY=value\n";
    let repeat = bytes / line.len() + 1;
    line.repeat(repeat)
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
