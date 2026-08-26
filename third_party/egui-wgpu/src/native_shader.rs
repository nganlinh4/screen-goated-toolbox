#![expect(unsafe_code)]

use std::borrow::Cow;

/// Create a backend-native HLSL module generated from validated, pinned WGSL at
/// build time. The binding maps in `build.rs` mirror the two fixed pipeline
/// layouts in this crate. Device-loss recovery calls this same constructor for
/// every replacement device, so no device-owned shader state survives a loss.
pub(crate) fn hlsl(
    device: &wgpu::Device,
    label: &'static str,
    entry_point: &'static str,
    source: &'static str,
) -> wgpu::ShaderModule {
    let descriptor = wgpu::ShaderModuleDescriptorPassthrough {
        label: Some(label),
        entry_points: Cow::Owned(vec![wgpu::PassthroughShaderEntryPoint {
            name: Cow::Borrowed(entry_point),
            workgroup_size: (0, 0, 0),
        }]),
        hlsl: Some(Cow::Borrowed(source)),
        ..Default::default()
    };

    // SAFETY: build.rs parses and validates the tracked WGSL and generates this
    // HLSL with binding maps matching the fixed layouts used by Renderer and
    // CaptureState. Each descriptor declares its exact single entry point.
    unsafe { device.create_shader_module_passthrough(descriptor) }
}
