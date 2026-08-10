#[path = "../../build_support/creation_runtime_delivery.rs"]
mod creation_runtime_delivery;
#[path = "../../build_support/local_asr_delivery.rs"]
mod local_asr_delivery;
#[path = "../../build_support/model_catalog.rs"]
mod model_catalog;
#[path = "../../build_support/qwen_runtime_delivery.rs"]
mod qwen_runtime_delivery;
#[path = "../../build_support/vc_runtime_delivery.rs"]
mod vc_runtime_delivery;
#[path = "../../build_support/web_asset_delivery.rs"]
mod web_asset_delivery;

use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = crate_dir
        .ancestors()
        .nth(2)
        .expect("recorder worker must remain under native/recorder_worker");
    assert_package_version_matches_host(repo);
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo::rustc-check-cfg=cfg(nopack)");
    let model_catalog_output = out.join("model_catalog_generated.rs");
    model_catalog::generate(
        &repo.join("catalog/model_catalog.json"),
        &model_catalog_output,
    );
    mark_host_only_catalog_items(&model_catalog_output);
    creation_runtime_delivery::generate(repo, &out);
    local_asr_delivery::generate(repo, &out);
    qwen_runtime_delivery::generate(repo, &out);
    vc_runtime_delivery::generate(repo, &out);
    web_asset_delivery::generate(repo, &out);
    println!(
        "cargo:rerun-if-changed={}",
        repo.join("catalog/model_catalog.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo.join("component-catalog/catalog-v1.json").display()
    );
}

fn mark_host_only_catalog_items(output: &Path) {
    let mut generated = std::fs::read_to_string(output).expect("read generated model catalog");
    for item in [
        "GEMINI_EMBEDDING_API_MODEL",
        "GEMINI_LIVE_API_MODEL_2_5",
        "GEMINI_LIVE_AUDIO_MODEL_ID_2_5",
        "GEMINI_LIVE_AUDIO_MODEL_ID_3_1",
        "GEMINI_LIVE_TRANSLATE_MODEL_ID",
        "QWEN3_ASR_0_6B_MODEL_ID",
        "QWEN3_ASR_1_7B_MODEL_ID",
        "REALTIME_TRANSLATION_MODEL_GTX",
        "HELP_ASSISTANT_MODEL_CHAIN_IDS",
        "COMPUTER_CONTROL_GROUNDING_MODEL_CHAIN_IDS",
        "GENERATED_REALTIME_TRANSCRIPTION_OPTIONS",
    ] {
        generated = generated.replace(
            &format!("pub const {item}"),
            &format!("#[cfg(not(feature = \"recorder-worker\"))]\npub const {item}"),
        );
    }
    generated = generated.replace(
        "pub fn generated_normalize_realtime_transcription_model_id",
        "#[cfg(not(feature = \"recorder-worker\"))]\npub fn generated_normalize_realtime_transcription_model_id",
    );
    std::fs::write(output, generated).expect("write recorder worker model catalog");
}

fn assert_package_version_matches_host(repo: &Path) {
    let host_manifest = std::fs::read_to_string(repo.join("Cargo.toml"))
        .expect("read host Cargo.toml for recorder worker version check");
    let expected = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
    assert!(
        host_manifest.lines().any(|line| line.trim() == expected),
        "recorder worker package version must match the signed host"
    );
}
