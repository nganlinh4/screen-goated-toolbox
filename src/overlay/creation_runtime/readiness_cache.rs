use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Entry {
    value: Option<String>,
    checked: Option<Instant>,
    active: bool,
}

static CACHE: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(Mutex::default);

pub(super) fn read(tool: &str) -> String {
    let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    let entry = cache.entry(tool.to_string()).or_default();
    if !entry.active
        && entry
            .checked
            .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(1))
    {
        entry.active = true;
        let tool = tool.to_string();
        std::thread::spawn(move || {
            let value = super::query_readiness(&tool);
            let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
            let entry = cache.entry(tool).or_default();
            entry.value = Some(value);
            entry.checked = Some(Instant::now());
            entry.active = false;
        });
    }
    entry
        .value
        .clone()
        .unwrap_or_else(|| "preparing".to_string())
}
