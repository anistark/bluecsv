//! Benchmarks for the hot paths the LSP invokes on every edit / hover.
//!
//! Synthetic data: 10 columns (id, name, email, price, signup_date, status,
//! count, flag, notes, region) with a fixed header. Run:
//!
//!   cargo bench -p bluecsv
//!
//! Sizes are 1k / 10k / 100k rows. 1M-row runs are opt-in via
//! `BLUECSV_BENCH_HUGE=1` to keep default CI time bounded.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn gen_csv(rows: usize) -> String {
    let header = "id,name,email,price,signup_date,status,count,flag,notes,region\n";
    let mut out = String::with_capacity(rows * 80 + header.len());
    out.push_str(header);
    for i in 0..rows {
        let name = ["Alice", "Bob", "Carol", "Dave", "Eve"][i % 5];
        let status = ["active", "inactive", "pending"][i % 3];
        let region = ["us-east", "us-west", "eu", "ap"][i % 4];
        let flag = if i % 2 == 0 { "true" } else { "false" };
        out.push_str(&format!(
            "{i},{name},{name}{i}@example.com,{price:.2},2024-{month:02}-{day:02},{status},{count},{flag},note-{i},{region}\n",
            price = (i as f64) * 1.25,
            month = ((i % 12) + 1),
            day = ((i % 28) + 1),
            count = i * 3,
        ));
    }
    out
}

fn sizes() -> Vec<usize> {
    let mut v = vec![1_000, 10_000, 100_000];
    if std::env::var("BLUECSV_BENCH_HUGE").is_ok() {
        v.push(1_000_000);
    }
    v
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for &rows in &sizes() {
        let input = gen_csv(rows);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(rows), &input, |b, s| {
            b.iter(|| bluecsv::parse(black_box(s)));
        });
    }
    group.finish();
}

fn bench_align(c: &mut Criterion) {
    let mut group = c.benchmark_group("align");
    for &rows in &sizes() {
        let input = gen_csv(rows);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(rows), &input, |b, s| {
            b.iter(|| bluecsv::align(black_box(s)));
        });
    }
    group.finish();
}

fn bench_infer_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("infer_table");
    for &rows in &sizes() {
        let input = gen_csv(rows);
        let parsed = bluecsv::parse(&input);
        group.throughput(Throughput::Elements(rows as u64));
        group.bench_with_input(BenchmarkId::from_parameter(rows), &parsed, |b, p| {
            b.iter(|| bluecsv::infer_table(black_box(p), true));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(4)).sample_size(40);
    targets = bench_parse, bench_align, bench_infer_table
}
criterion_main!(benches);
