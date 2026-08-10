use super::arguments::StartupArgs;

pub(super) const EXPORT_REPLAY_FLAG: &str = "--sr-export-replay";
pub(super) const EXPORT_REPLAY_LAST_FLAG: &str = "--sr-export-replay-last";

pub(crate) fn is_requested(args: &StartupArgs) -> bool {
    args.has(EXPORT_REPLAY_FLAG) || args.has(EXPORT_REPLAY_LAST_FLAG)
}

pub(crate) fn run(args: &StartupArgs) -> Option<i32> {
    let replay_path = absolute_replay_path(&resolve_replay_path(args)?);

    crate::initialization::init_com_and_dpi();
    let bench_runs = args
        .value("--sr-export-replay-bench")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|runs| *runs > 0);
    let keep_outputs = args.has("--sr-export-replay-keep-output");

    if bench_runs.is_none() {
        println!("[Replay] Running native export replay from {replay_path}");
        return match crate::overlay::screen_record::run_export_replay(&replay_path, 1, true) {
            Ok(response) => match replay_runs(&response).first() {
                Some(run) if run.get("error").is_none() => {
                    let result = run.get("result").cloned().unwrap_or_default();
                    println!(
                        "[Replay] Export replay succeeded: {}",
                        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
                    );
                    Some(0)
                }
                Some(run) => {
                    eprintln!(
                        "[Replay] Export replay failed: {}",
                        run.get("error")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown worker error")
                    );
                    Some(1)
                }
                None => {
                    eprintln!("[Replay] Export replay failed: worker returned no run");
                    Some(1)
                }
            },
            Err(error) => {
                println!("[Replay] Export replay failed: {error:#}");
                Some(1)
            }
        };
    }

    let runs = bench_runs.unwrap_or(1);
    let runs = u16::try_from(runs).ok().filter(|runs| *runs <= 100);
    let Some(runs) = runs else {
        eprintln!("[ReplayBench] run count must be between 1 and 100");
        return Some(2);
    };
    println!("[ReplayBench] Running {runs} native export replay run(s) from {replay_path}");
    let response =
        match crate::overlay::screen_record::run_export_replay(&replay_path, runs, keep_outputs) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("[ReplayBench] worker failed: {error:#}");
                return Some(1);
            }
        };
    let returned_runs = replay_runs(&response);
    let mut successful_wall_secs: Vec<f64> = Vec::with_capacity(runs as usize);
    let mut failed_runs = 0usize;
    for (run_idx, run) in returned_runs.iter().enumerate() {
        match run.get("result") {
            Some(result) => {
                let wall_secs = run
                    .get("wallSeconds")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
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
            }
            None => {
                failed_runs += 1;
                eprintln!(
                    "[ReplayBench] run={}/{} failed: {}",
                    run_idx + 1,
                    runs,
                    run.get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown worker error")
                );
            }
        }
    }
    failed_runs += (runs as usize).saturating_sub(returned_runs.len());

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
            crate::overlay::screen_record::export_replay_args_path()
                .map(|path| path.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

fn absolute_replay_path(path: &str) -> String {
    let path = std::path::PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(path)
    }
    .to_string_lossy()
    .to_string()
}

fn replay_runs(response: &serde_json::Value) -> &[serde_json::Value] {
    response
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
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
