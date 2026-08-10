#[path = "../../../../../src/api/realtime_audio/qwen3/assets.rs"]
pub(crate) mod assets;
#[path = "../../../../../src/api/realtime_audio/qwen3/runtime.rs"]
pub(crate) mod runtime;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Qwen3ModelVariant {
    Small,
    Large,
}
