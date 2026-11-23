use std::path::{Path, PathBuf};

use super::{Roots, remove_roots, scan_roots};

/// An isolated runtime-state root laid out like `SGT_RUNTIME_STATE_ROOT`.
fn isolated_state(name: &str) -> (PathBuf, Roots) {
    let state =
        std::env::temp_dir().join(format!("sgt-legacy-offline-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let roots = Roots {
        roaming: state.join("roaming-state").join("screen-goated-toolbox"),
        local: state.join("local-app-data").join("screen-goated-toolbox"),
    };
    (state, roots)
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![7_u8; bytes]).unwrap();
}

#[test]
fn sizes_and_removes_only_known_legacy_folders() {
    let (state, roots) = isolated_state("known");
    let (roaming, local) = (&roots.roaming, &roots.local);
    let legacy = [
        (roaming.join("models/kokoro_v1/model.onnx"), 10),
        (roaming.join("models/parakeet/encoder.onnx"), 20),
        (roaming.join("bin/qwen3_asr_reference/asr-server.exe"), 3),
        (local.join("bin/magpie_runtime/runtime/python.exe"), 5),
        (
            local.join("components/step-audio-runtime/1.0/receipt.json"),
            7,
        ),
        (
            local.join("worker-workspaces/local-asr-worker/state.json"),
            6,
        ),
        (roaming.join("models/supertonic_3/model.onnx"), 8),
        (
            local.join("components/supertonic-3-model/2026.05.11-int8/voice.bin"),
            9,
        ),
        (
            local.join("components/sherpa-onnx-runtime/1.13.2/bin/x64/sherpa-onnx-c-api.dll"),
            10,
        ),
        (local.join("bin/x64/sherpa-onnx/sherpa-onnx-c-api.dll"), 11),
        (
            local.join("component-staging/vieneu-runtime-1-42-3/file.bin"),
            1,
        ),
    ];
    let legacy_files = [
        (
            local.join("component-downloads/kokoro-82m-v1-model-42-1.download"),
            4,
        ),
        (
            local.join("component-partials/qwen3-cuda-runtime-ab12.part"),
            2,
        ),
        (
            local.join("component-downloads/supertonic-3-model-42-1.download"),
            12,
        ),
    ];
    let kept = [
        roaming.join("models/kokoro_v1_notes/readme.txt"),
        roaming.join("config.json"),
        roaming.join("tts_references/reference-mic_1.wav"),
        local.join("components/screen-recorder/1.0/receipt.json"),
        local.join("component-staging/screen-recorder-1-42-3/file.bin"),
        local.join("components/onnx-directml-runtime/1.0/receipt.json"),
        local.join("bin/x64/onnxruntime.dll"),
    ];
    for (path, bytes) in legacy.iter().chain(&legacy_files) {
        write(path, *bytes);
    }
    for path in &kept {
        write(path, 1);
    }

    let found = scan_roots(&roots).unwrap();
    assert_eq!(found.paths.len(), legacy.len() + legacy_files.len());
    assert_eq!(found.total_bytes, 108);
    assert!(
        found
            .paths
            .iter()
            .all(|path| path.starts_with(roaming) || path.starts_with(local))
    );

    remove_roots(&roots).unwrap();

    assert!(!roaming.join("models/kokoro_v1").exists());
    assert!(!roaming.join("models/parakeet").exists());
    assert!(!roaming.join("bin/qwen3_asr_reference").exists());
    assert!(!local.join("bin/magpie_runtime").exists());
    assert!(!local.join("components/step-audio-runtime").exists());
    assert!(!local.join("worker-workspaces/local-asr-worker").exists());
    assert!(!roaming.join("models/supertonic_3").exists());
    assert!(!local.join("components/supertonic-3-model").exists());
    assert!(!local.join("components/sherpa-onnx-runtime").exists());
    assert!(!local.join("bin/x64/sherpa-onnx").exists());
    assert!(
        !local
            .join("component-staging/vieneu-runtime-1-42-3")
            .exists()
    );
    for (path, _) in &legacy_files {
        assert!(!path.exists(), "{} should be removed", path.display());
    }
    for path in &kept {
        assert!(path.exists(), "{} must be kept", path.display());
    }
    assert!(local.join("components").is_dir());
    assert!(local.join("bin").is_dir());
    assert!(scan_roots(&roots).unwrap().paths.is_empty());
    std::fs::remove_dir_all(&state).unwrap();
}

#[test]
fn a_junctioned_models_folder_is_neither_listed_nor_followed() {
    let (state, roots) = isolated_state("junction");
    let outside = state.join("outside");
    let outside_model = outside.join("kokoro_v1/model.onnx");
    write(&outside_model, 9);
    std::fs::create_dir_all(&roots.roaming).unwrap();
    let link = roots.roaming.join("models");
    let executable = crate::system_command::system_executable("cmd.exe").unwrap();
    let status = crate::system_command::background_command(executable)
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "junction fixture failed: {status}");

    assert!(scan_roots(&roots).unwrap().paths.is_empty());
    remove_roots(&roots).unwrap();

    assert!(outside_model.exists());
    std::fs::remove_dir(&link).unwrap();
    std::fs::remove_dir_all(&state).unwrap();
}
