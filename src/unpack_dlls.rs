#[path = "unpack_dlls/ai_runtime/mod.rs"]
mod ai_runtime;

use std::path::PathBuf;

pub(crate) use self::ai_runtime::ensure_native_onnx_runtime;
#[cfg(not(feature = "recorder-worker"))]
pub use self::ai_runtime::remove_ai_runtime;
#[cfg(feature = "recorder-worker")]
pub use self::ai_runtime::{AiRuntimeStatus, current_ai_runtime_status, start_ai_runtime_install};
pub use self::ai_runtime::{AiRuntimeUi, ensure_ai_runtime_installed, is_ai_runtime_installed};

pub(crate) fn private_bin_dir() -> PathBuf {
    crate::paths::app_local_data_dir().join("bin").join("x64")
}
