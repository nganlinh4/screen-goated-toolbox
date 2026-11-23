use serde_json::Value;
use std::sync::atomic::AtomicBool;

#[test]
fn background_dispatch_cannot_execute_native_window_commands() {
    for cmd in [
        "pick_image",
        "pick_images",
        "pick_output_dir",
        "start_drag",
        "close_window",
        "minimize_window",
    ] {
        assert!(super::super::ipc::dispatch(cmd, &Value::Null, &AtomicBool::new(true)).is_err());
    }
}
