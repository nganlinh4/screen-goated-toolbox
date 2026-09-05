use super::{hide_command_window, process_query, shared_runtime_path};
use serde_json::{Value, json};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

static PENDING: AtomicUsize = AtomicUsize::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static REFRESHING: AtomicBool = AtomicBool::new(false);
static FAILED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn cancel() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(crate) fn status() -> Value {
    json!({
        "pendingCount": PENDING.load(Ordering::Acquire),
        "running": RUNNING.load(Ordering::Acquire),
        "failed": FAILED.load(Ordering::Acquire),
    })
}

fn query(command_name: &str, timeout: Duration, generation: usize) -> Option<Value> {
    let mut command = Command::new(shared_runtime_path()?);
    command.args(["--parent-pid", &std::process::id().to_string()]);
    hide_command_window(&mut command);
    let input = format!(
        "{}\n",
        json!({"id":"verification", "cmd":command_name,"args":{}})
    );
    let output = process_query::run_cancellable(
        &mut command,
        Some(input.as_bytes()),
        timeout,
        64 * 1024,
        || {
            super::runtime_shutting_down()
                || GENERATION.load(Ordering::Acquire) != generation
                || crate::overlay::creation_close::is_closing("3d")
        },
    )?;
    if !output.status.success() || output.truncated {
        return None;
    }
    String::from_utf8_lossy(&output.bytes)
        .lines()
        .rev()
        .find_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            (value["id"] == "verification" && value["ok"] == true).then(|| value["result"].clone())
        })
}

fn pending_count(value: &Value) -> Option<usize> {
    value["pendingCount"]
        .as_u64()
        .filter(|count| *count <= 1)
        .map(|count| count as usize)
}

fn read_pending(generation: usize) {
    if let Some(value) = query(
        "capacity_verification_status",
        Duration::from_secs(5),
        generation,
    ) && let Some(count) = pending_count(&value)
        && GENERATION.load(Ordering::Acquire) == generation
    {
        PENDING.store(count, Ordering::Release);
    }
}

pub(crate) fn refresh() {
    if REFRESHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let generation = GENERATION.load(Ordering::Acquire);
    std::thread::spawn(move || {
        read_pending(generation);
        REFRESHING.store(false, Ordering::Release);
        crate::overlay::three_d_generator::update_settings();
    });
}

pub(crate) fn start() -> Value {
    if PENDING.load(Ordering::Acquire) == 0 || RUNNING.swap(true, Ordering::AcqRel) {
        return status();
    }
    FAILED.store(false, Ordering::Release);
    let generation = GENERATION.load(Ordering::Acquire);
    std::thread::spawn(move || {
        let succeeded = query("verify_capacity", Duration::from_secs(10 * 60), generation)
            .is_some_and(|value| pending_count(&value) == Some(0));
        if succeeded && GENERATION.load(Ordering::Acquire) == generation {
            PENDING.store(0, Ordering::Release);
        }
        FAILED.store(!succeeded, Ordering::Release);
        read_pending(generation);
        RUNNING.store(false, Ordering::Release);
        crate::overlay::three_d_generator::update_settings();
    });
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_integer_counts_are_actionable() {
        assert_eq!(pending_count(&json!({"pendingCount": 0})), Some(0));
        assert_eq!(pending_count(&json!({"pendingCount": 1})), Some(1));
        for value in [
            json!({}),
            json!(null),
            json!({"pendingCount": 2}),
            json!({"pendingCount": -1}),
            json!({"pendingCount": "1"}),
            json!({"pendingCount": 1.5}),
            json!({"pendingCount": true}),
        ] {
            assert_eq!(pending_count(&value), None);
        }
    }
}
