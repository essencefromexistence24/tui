use criterion::{Criterion, black_box, criterion_group, criterion_main};

const SHORT_TEXT: &str = "Hello world, this is a short text.";
const MEDIUM_TEXT: &str = "The quick brown fox jumps over the lazy dog. \
  This pangram is used for testing text compression algorithms. \
  It contains every letter of the English alphabet at least once. \
  It has been used for over a century in typing and font testing. \
  The sentence is well known and widely recognized in the industry.";
const LONG_TEXT: &str = include_str!("../../caveman/src/lib.rs");
const JSON_TEXT: &str = r#"[
  {"name": "alice", "age": 30, "role": "admin"},
  {"name": "bob", "age": 25, "role": "user"},
  {"name": "carol", "age": 35, "role": "moderator"},
  {"name": "dave", "age": 28, "role": "user"},
  {"name": "eve", "age": 32, "role": "admin"}
]"#;
const RTK_TEXT: &str = "diff --git a/src/main.rs b/src/main.rs\n\
  index abc123..def456 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n\
  @@ -1,5 +1,6 @@\n-fn old() {}\n+fn new() {}\n";

fn bench_lite(c: &mut Criterion) {
    let mut group = c.benchmark_group("lite");
    group.bench_function("short", |b| {
        b.iter(|| dx_route_lite::compress(black_box(SHORT_TEXT), "full"))
    });
    group.bench_function("long", |b| {
        b.iter(|| dx_route_lite::compress(black_box(LONG_TEXT), "full"))
    });
    group.finish();
}

fn bench_caveman(c: &mut Criterion) {
    let mut group = c.benchmark_group("caveman");
    group.bench_function("short", |b| {
        b.iter(|| dx_route_caveman::compress(black_box(SHORT_TEXT), "full"))
    });
    group.bench_function("medium", |b| {
        b.iter(|| dx_route_caveman::compress(black_box(MEDIUM_TEXT), "full"))
    });
    group.bench_function("long", |b| {
        b.iter(|| dx_route_caveman::compress(black_box(LONG_TEXT), "ultra"))
    });
    group.finish();
}

fn bench_rtk(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtk");
    group.bench_function("git_diff", |b| {
        b.iter(|| dx_route_rtk::compress(black_box(RTK_TEXT), Some("git diff")))
    });
    group.bench_function("generic", |b| {
        b.iter(|| dx_route_rtk::compress(black_box(LONG_TEXT), None))
    });
    group.finish();
}

fn bench_ultra(c: &mut Criterion) {
    let mut group = c.benchmark_group("ultra");
    group.bench_function("short", |b| {
        b.iter(|| dx_route_ultra::compress(black_box(SHORT_TEXT), "full"))
    });
    group.bench_function("long", |b| {
        b.iter(|| dx_route_ultra::compress(black_box(LONG_TEXT), "aggressive"))
    });
    group.finish();
}

fn bench_aggressive(c: &mut Criterion) {
    let mut group = c.benchmark_group("aggressive");
    group.bench_function("medium", |b| {
        b.iter(|| dx_route_aggressive::compress(black_box(MEDIUM_TEXT), "full"))
    });
    group.finish();
}

fn bench_headroom(c: &mut Criterion) {
    let mut group = c.benchmark_group("headroom");
    group.bench_function("json", |b| {
        b.iter(|| dx_route_headroom::compress(black_box(JSON_TEXT), "full"))
    });
    group.bench_function("text", |b| {
        b.iter(|| dx_route_headroom::compress(black_box(SHORT_TEXT), "full"))
    });
    group.finish();
}

fn bench_dedup(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedup");
    let text = "line1\nline2\nline1\nline3\nline2\nline4\n".repeat(100);
    let mut state = dx_route_dedup::SessionState::default();
    group.bench_function("exact", |b| b.iter(|| state.compress(black_box(&text))));
    group.finish();
}

fn bench_ccr(c: &mut Criterion) {
    let mut group = c.benchmark_group("ccr");
    let text = "before\n```\n".to_string() + &"x".repeat(1000) + "\n```\nafter";
    let mut store = dx_route_ccr::BlobStore::new();
    group.bench_function("compress", |b| {
        b.iter(|| dx_route_ccr::compress(black_box(&text), &mut store))
    });
    let compressed = dx_route_ccr::compress(&text, &mut dx_route_ccr::BlobStore::new());
    group.bench_function("expand", |b| {
        b.iter(|| dx_route_ccr::expand(black_box(&compressed.text), &store))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_lite,
    bench_caveman,
    bench_rtk,
    bench_ultra,
    bench_aggressive,
    bench_headroom,
    bench_dedup,
    bench_ccr,
);
criterion_main!(benches);
