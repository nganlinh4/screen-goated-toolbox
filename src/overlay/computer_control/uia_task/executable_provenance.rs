//! Bounded identity of the executable that produced a Computer Control trace.

use serde_json::{Value, json};

pub(super) fn capture() -> Value {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let Ok(path) = std::env::current_exe() else {
        return json!({"error": "current executable path unavailable"});
    };
    let metadata = std::fs::metadata(&path).ok();
    let modified_ms = metadata
        .as_ref()
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis());
    let sha256 = std::fs::File::open(&path).ok().and_then(|mut file| {
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        Some(format!("{:x}", hasher.finalize()))
    });
    json!({
        "path": path,
        "byte_count": metadata.map(|value| value.len()),
        "modified_unix_ms": modified_ms,
        "sha256": sha256,
    })
}
