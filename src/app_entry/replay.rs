use std::time::Instant;

use anyhow::{Context, Result};

use super::arguments::StartupArgs;

pub(super) const EXPORT_REPLAY_FLAG: &str = "--sr-export-replay";
pub(super) const EXPORT_REPLAY_LAST_FLAG: &str = "--sr-export-replay-last";

pub(crate) fn is_requested(args: &StartupArgs) -> bool {
    args.has(EXPORT_REPLAY_FLAG) || args.has(EXPORT_REPLAY_LAST_FLAG)
}

pub(crate) fn run(args: &StartupArgs) -> Option<i32> {
    let replay_path = resolve_replay_path(args)?;
    let payload = match load_replay_payload(&replay_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("[Replay] {error}");
            return Some(2);
        }
    };

    crate::initialization::init_com_and_dpi();
    let bench_runs = args
        .value("--sr-export-replay-bench")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|runs| *runs > 0);
    let keep_outputs = args.has("--sr-export-replay-keep-output");

    if bench_runs.is_none() {
        println!("[Replay] Running native export replay from {replay_path}");
        return match crate::overlay::screen_record::native_export::start_native_export(payload) {
            Ok(result) => {
                println!(
                    "[Replay] Export replay succeeded: {}",
                    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
                );
                Some(0)
            }
            Err(error) => {
                eprintln!("[Replay] Export replay failed: {error}");
                Some(1)
            }
        };
    }

    let runs = bench_runs.unwrap_or(1);
    println!("[ReplayBench] Running {runs} native export replay run(s) from {replay_path}");
    let mut successful_wall_secs: Vec<f64> = Vec::with_capacity(runs);
    let mut failed_runs = 0usize;
    for run_idx in 0..runs {
        let run_start = Instant::now();
        match crate::overlay::screen_record::native_export::start_native_export(payload.clone()) {
            Ok(result) => {
                let wall_secs = run_start.elapsed().as_secs_f64();
                successful_wall_secs.push(wall_secs);
                let status = result
                    .get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let bytes = result
                    .get("bytes")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let output_path = result
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                println!(
                    "[ReplayBench] run={}/{} status={} wall={:.3}s bytes={} path={}",
                    run_idx + 1,
                    runs,
                    status,
                    wall_secs,
                    bytes,
                    if output_path.is_empty() {
                        "-"
                    } else {
                        output_path
                    }
                );
                if !keep_outputs && !output_path.is_empty() {
                    let _ = std::fs::remove_file(output_path);
                }
            }
            Err(error) => {
                failed_runs += 1;
                eprintln!(
                    "[ReplayBench] run={}/{} failed: {}",
                    run_idx + 1,
                    runs,
                    error
                );
            }
        }
    }

    if successful_wall_secs.is_empty() {
        eprintln!("[ReplayBench] all runs failed");
        return Some(1);
    }

    let mut sorted = successful_wall_secs.clone();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = sorted.iter().copied().sum();
    let avg = sum / sorted.len() as f64;
    let min = *sorted.first().unwrap_or(&0.0);
    let max = *sorted.last().unwrap_or(&0.0);
    let p50 = percentile(&sorted, 0.50);
    let p90 = percentile(&sorted, 0.90);
    println!(
        "[ReplayBench] summary runs={} ok={} failed={} min={:.3}s p50={:.3}s p90={:.3}s avg={:.3}s max={:.3}s keep_outputs={}",
        runs,
        sorted.len(),
        failed_runs,
        min,
        p50,
        p90,
        avg,
        max,
        keep_outputs
    );
    Some(if failed_runs > 0 { 1 } else { 0 })
}

fn resolve_replay_path(args: &StartupArgs) -> Option<String> {
    args.value(EXPORT_REPLAY_FLAG).or_else(|| {
        if args.has(EXPORT_REPLAY_LAST_FLAG) {
            crate::overlay::screen_record::native_export::export_replay_args_path()
                .map(|path| path.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

fn load_replay_payload(replay_path: &str) -> Result<serde_json::Value> {
    let raw = std::fs::read_to_string(replay_path)
        .with_context(|| format!("Failed to read export replay payload '{replay_path}'"))?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .with_context(|| format!("Invalid JSON in export replay payload '{replay_path}'"))
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let index = ((sorted.len() - 1) as f64 * clamped).round() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_uses_nearest_rank_and_clamps_ratio() {
        let sorted = [1.0, 2.0, 3.0, 4.0];

        assert_eq!(percentile(&[], 0.5), 0.0);
        assert_eq!(percentile(&sorted, -1.0), 1.0);
        assert_eq!(percentile(&sorted, 0.5), 3.0);
        assert_eq!(percentile(&sorted, 0.9), 4.0);
        assert_eq!(percentile(&sorted, 2.0), 4.0);
    }
}
