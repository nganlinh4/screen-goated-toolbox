mod manifest;
mod report;
mod runner;
mod scoring;
mod setup;

#[test]
fn catalog_benchmark_fixtures_are_valid() {
    let manifest = manifest::Manifest::load().expect("load catalog benchmark manifest");
    manifest
        .validate()
        .expect("validate catalog benchmark fixtures");
}

#[test]
#[ignore = "requires CATALOG_BENCH_LIVE=1 and real provider credentials"]
fn catalog_benchmark_live() {
    assert_eq!(
        std::env::var("CATALOG_BENCH_LIVE").as_deref(),
        Ok("1"),
        "set CATALOG_BENCH_LIVE=1 after reviewing tests/catalog-benchmark/review.html"
    );
    runner::run().expect("run live catalog benchmark");
}

#[test]
#[ignore = "requires CATALOG_BENCH_MERGE_INPUTS and CATALOG_BENCH_OUTPUT"]
fn catalog_benchmark_merge_reports() {
    let inputs = std::env::var_os("CATALOG_BENCH_MERGE_INPUTS")
        .expect("set CATALOG_BENCH_MERGE_INPUTS to semicolon-separated attempts.jsonl paths")
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let output = std::env::var_os("CATALOG_BENCH_OUTPUT")
        .map(std::path::PathBuf::from)
        .expect("set CATALOG_BENCH_OUTPUT for the merged report");
    report::merge_reports(&inputs, &output).expect("merge catalog benchmark reports");
}
