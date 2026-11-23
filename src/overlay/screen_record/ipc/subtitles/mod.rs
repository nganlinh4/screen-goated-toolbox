pub(crate) mod audio;
mod job;
pub(crate) mod media;
mod postprocess;
mod providers;
mod translation;
#[cfg(test)]
mod translation_cancel_tests;
mod translation_openai;
mod translation_providers;
pub(crate) mod types;

pub use job::{
    handle_cancel_subtitle_generation, handle_get_subtitle_generation_capabilities,
    handle_get_subtitle_generation_status, handle_start_subtitle_generation,
};
pub use translation::{
    handle_cancel_subtitle_translation, handle_get_subtitle_translation_capabilities,
    handle_get_subtitle_translation_status, handle_start_subtitle_translation,
};

pub(super) fn cancel_all_jobs() {
    job::cancel_all_jobs();
    translation::cancel_all_jobs();
}
