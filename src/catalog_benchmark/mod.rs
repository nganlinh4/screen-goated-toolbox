mod history;
mod localization_probe;
mod manifest;
mod reasoning;
mod report;
mod review;
mod runner;
mod scoring;
pub(crate) mod setup;
mod transport_probe;

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

#[test]
#[ignore = "reads completed local benchmark runs without calling providers"]
fn catalog_benchmark_refresh_latest_history() {
    history::refresh_current_history().expect("refresh latest benchmark history");
}

#[test]
#[ignore = "requires CATALOG_BENCH_REGISTER_OUTPUT for one complete logical live run"]
fn catalog_benchmark_register_history_run() {
    let output = std::env::var_os("CATALOG_BENCH_REGISTER_OUTPUT")
        .map(std::path::PathBuf::from)
        .expect("set CATALOG_BENCH_REGISTER_OUTPUT to a complete live run directory");
    history::register_existing_live_run(&output).expect("register existing live benchmark run");
}

#[test]
#[ignore = "requires CATALOG_BENCH_TRANSPORT_PROBE=1 and a real Gemini credential"]
fn catalog_benchmark_transport_probe() {
    assert_eq!(
        std::env::var("CATALOG_BENCH_TRANSPORT_PROBE").as_deref(),
        Ok("1"),
        "set CATALOG_BENCH_TRANSPORT_PROBE=1 for the non-history transport experiment"
    );
    transport_probe::run().expect("run catalog benchmark transport probe");
}

#[test]
#[ignore = "requires CATALOG_BENCH_LOCALIZATION_PROBE=1 and real vision credentials"]
fn catalog_benchmark_localization_probe() {
    assert_eq!(
        std::env::var("CATALOG_BENCH_LOCALIZATION_PROBE").as_deref(),
        Ok("1"),
        "set CATALOG_BENCH_LOCALIZATION_PROBE=1 for the non-history localization diagnostic"
    );
    localization_probe::run().expect("run screen-text localization probe");
}
