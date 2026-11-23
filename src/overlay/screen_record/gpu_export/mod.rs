#[cfg(test)]
mod background_parity_tests;
mod cache;
mod compositor;
#[cfg(test)]
mod cursor_shadow_parity_tests;
mod cursors;
mod limits;
#[cfg(test)]
mod overlay_batch_tests;
mod setup;
mod shader;
pub(crate) mod wait;
mod webcam;

pub use compositor::{CompositorUniforms, GpuCompositor};
pub(crate) use setup::prewarm_shared_gpu_context;
pub use setup::{CompositorUniformParams, create_uniforms};
pub(crate) use setup::{diagnostic_context, diagnostic_device_loss};
