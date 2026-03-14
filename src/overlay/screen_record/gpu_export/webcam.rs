use bytemuck::{Pod, Zeroable};

use super::setup::{OUTPUT_TEXTURE_FORMAT, SharedGpuContext};
use crate::overlay::screen_record::native_export::config::BakedWebcamFrame;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WebcamOverlayUniforms {
    output_size: [f32; 2],
    origin: [f32; 2],
    size: [f32; 2],
    roundness_px: f32,
    shadow_px: f32,
    opacity: f32,
    mirror: f32,
    _pad: [f32; 2],
}

pub(super) struct WebcamOverlayState {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    frame_width: u32,
    frame_height: u32,
    has_frame: bool,
}

impl WebcamOverlayState {
    pub fn new(device: &wgpu::Device, shared: &SharedGpuContext) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let texture = create_texture(device, 1, 1);
        let bind_group = create_bind_group(device, shared, &texture, &sampler);
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Webcam Overlay Uniform Buffer"),
            size: std::mem::size_of::<WebcamOverlayUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Webcam Overlay Uniform BG"),
            layout: &shared.webcam_uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            texture,
            bind_group,
            sampler,
            uniform_buffer,
            uniform_bind_group,
            frame_width: 1,
            frame_height: 1,
            has_frame: false,
        }
    }

    pub fn ensure_size(
        &mut self,
        device: &wgpu::Device,
        shared: &SharedGpuContext,
        width: u32,
        height: u32,
    ) {
        if width == 0 || height == 0 {
            return;
        }
        if self.frame_width == width && self.frame_height == height {
            return;
        }
        self.texture = create_texture(device, width, height);
        self.bind_group = create_bind_group(device, shared, &self.texture, &self.sampler);
        self.frame_width = width;
        self.frame_height = height;
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn mark_has_frame(&mut self) {
        self.has_frame = true;
    }

    pub fn prepare(
        &self,
        queue: &wgpu::Queue,
        output_width: u32,
        output_height: u32,
        frame: &BakedWebcamFrame,
    ) -> bool {
        if !self.has_frame
            || !frame.visible
            || frame.opacity <= 0.001
            || frame.width <= 0.0
            || frame.height <= 0.0
            || output_width == 0
            || output_height == 0
        {
            return false;
        }

        let uniforms = WebcamOverlayUniforms {
            output_size: [output_width as f32, output_height as f32],
            origin: [frame.x as f32, frame.y as f32],
            size: [frame.width as f32, frame.height as f32],
            roundness_px: frame.roundness_px.max(0.0) as f32,
            shadow_px: frame.shadow_px.max(0.0) as f32,
            opacity: frame.opacity.clamp(0.0, 1.0) as f32,
            mirror: if frame.mirror { 1.0 } else { 0.0 },
            _pad: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        true
    }

    pub fn render_pass<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        shared: &'a SharedGpuContext,
        vertex_buffer: &'a wgpu::Buffer,
    ) {
        pass.set_pipeline(&shared.webcam_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Webcam Overlay Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Bgra8UnormSrgb],
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    shared: &SharedGpuContext,
    texture: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(OUTPUT_TEXTURE_FORMAT),
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Webcam Overlay Texture BG"),
        layout: &shared.texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
