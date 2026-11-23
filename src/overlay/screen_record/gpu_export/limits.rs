//! Check resource sizes before submitting them to wgpu's validation layer.

const MAX_REQUESTED_READBACK_BYTES: u64 = 512 * 1024 * 1024;

/// Use adapter-supported texture resolution and enough buffer capacity for the
/// permitted padded readback, without requiring unusually large buffers.
pub(super) fn export_device_limits(adapter: &wgpu::Limits) -> wgpu::Limits {
    let mut required = wgpu::Limits::default().using_resolution(adapter.clone());
    required.max_buffer_size = required
        .max_buffer_size
        .max(adapter.max_buffer_size.min(MAX_REQUESTED_READBACK_BYTES));
    required
}

pub(super) struct ResourceErrorScopes {
    validation: wgpu::ErrorScopeGuard,
    memory: wgpu::ErrorScopeGuard,
    internal: wgpu::ErrorScopeGuard,
}

impl ResourceErrorScopes {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            validation: device.push_error_scope(wgpu::ErrorFilter::Validation),
            memory: device.push_error_scope(wgpu::ErrorFilter::OutOfMemory),
            internal: device.push_error_scope(wgpu::ErrorFilter::Internal),
        }
    }

    pub(super) fn finish(self, context: &str) -> Result<(), String> {
        let mut first_error = None;
        for scope in [self.internal, self.memory, self.validation] {
            if let Some(error) = pollster::block_on(scope.pop()) {
                first_error.get_or_insert_with(|| format!("{context}: {error}"));
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

pub(super) fn validate_texture(
    limits: &wgpu::Limits,
    width: u32,
    height: u32,
    label: &str,
) -> Result<(), String> {
    let maximum = limits.max_texture_dimension_2d;
    if width == 0 || height == 0 || width > maximum || height > maximum {
        return Err(format!(
            "{label} dimensions {width}x{height} exceed the GPU texture limit of {maximum}"
        ));
    }
    Ok(())
}

pub(super) fn validate_readback(
    limits: &wgpu::Limits,
    width: u32,
    height: u32,
) -> Result<(u32, u64), String> {
    let row = u64::from(width) * 4;
    let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let padded = row.div_ceil(alignment) * alignment;
    let bytes = padded
        .checked_mul(u64::from(height))
        .ok_or_else(|| "GPU readback dimensions are too large".to_string())?;
    if bytes == 0 || bytes > limits.max_buffer_size || padded > u64::from(u32::MAX) {
        return Err(format!(
            "GPU readback buffer requires {bytes} bytes, above the device limit of {}",
            limits.max_buffer_size
        ));
    }
    Ok((padded as u32, bytes))
}

pub(super) fn validate_rgba(
    limits: &wgpu::Limits,
    width: u32,
    height: u32,
    data: &[u8],
    label: &str,
) -> Result<(), String> {
    validate_texture(limits, width, height, label)?;
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("{label} pixel data size overflows"))?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) != expected {
        return Err(format!("{label} pixel data has an unexpected length"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_limits_and_readback_padding_are_checked() {
        let limits = wgpu::Limits {
            max_texture_dimension_2d: 8192,
            max_buffer_size: 1024,
            ..Default::default()
        };
        assert!(validate_texture(&limits, 8192, 1, "output").is_ok());
        assert!(validate_texture(&limits, 8193, 1, "output").is_err());
        assert!(validate_texture(&limits, 0, 1, "output").is_err());
        assert_eq!(validate_readback(&limits, 1, 4).unwrap(), (256, 1024));
        assert!(validate_readback(&limits, 1, 5).is_err());
        assert!(validate_rgba(&limits, 2, 2, &[0; 15], "atlas").is_err());
        assert!(validate_rgba(&limits, 2, 2, &[0; 16], "atlas").is_ok());
    }

    #[test]
    fn request_supported_readback_capacity_without_unbounded_requirements() {
        let adapter = wgpu::Limits {
            max_texture_dimension_2d: 16_384,
            max_buffer_size: 1024 * 1024 * 1024,
            ..Default::default()
        };
        let requested = export_device_limits(&adapter);
        assert_eq!(requested.max_texture_dimension_2d, 16_384);
        assert_eq!(requested.max_buffer_size, MAX_REQUESTED_READBACK_BYTES);
        let tight = wgpu::Limits {
            max_buffer_size: 300 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(
            export_device_limits(&tight).max_buffer_size,
            tight.max_buffer_size
        );
    }

    #[test]
    #[ignore = "requires a graphics adapter"]
    fn validation_errors_are_returned_by_resource_scope() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("a graphics adapter is required for this test");
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("device creation should succeed");
        let scopes = ResourceErrorScopes::new(&device);
        let _texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("invalid test texture"),
            size: wgpu::Extent3d {
                width: device.limits().max_texture_dimension_2d + 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        assert!(scopes.finish("test allocation").is_err());
    }
}
