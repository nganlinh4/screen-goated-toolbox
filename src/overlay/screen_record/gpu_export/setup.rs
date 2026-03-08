use bytemuck::{Pod, Zeroable};
use std::sync::{Arc, OnceLock};
use wgpu::util::DeviceExt;

use super::compositor::CompositorUniforms;
use super::shader::{compositor_shader, overlay_shader};

pub(super) const OUTPUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

/// Per-vertex data for the overlay atlas pipeline.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct OverlayVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub alpha: f32,
    pub _pad: f32, // align to 4 floats
}

pub(super) const QUAD_VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0],
        tex_coords: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        tex_coords: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        tex_coords: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, 1.0],
        tex_coords: [0.0, 0.0],
    },
];

pub(super) struct SharedGpuContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub pipeline: wgpu::RenderPipeline,
    pub accumulate_pipeline: wgpu::RenderPipeline,
    pub overlay_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub uniform_layout: wgpu::BindGroupLayout,
    pub texture_layout: wgpu::BindGroupLayout,
    pub background_overlay_layout: wgpu::BindGroupLayout,
    pub atlas_texture_layout: wgpu::BindGroupLayout,
}

static SHARED_GPU_CONTEXT: OnceLock<Result<SharedGpuContext, String>> = OnceLock::new();

fn create_shared_gpu_context() -> Result<SharedGpuContext, String> {
    let request_adapter = |instance: &wgpu::Instance| {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()
    };

    #[cfg(target_os = "windows")]
    let preferred_backends = wgpu::Backends::DX12;
    #[cfg(not(target_os = "windows"))]
    let preferred_backends = wgpu::Backends::all();

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: preferred_backends,
        ..Default::default()
    });
    #[cfg(target_os = "windows")]
    let mut adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::DX12))
        .into_iter()
        .find(|candidate| candidate.get_info().vendor == 0x10DE);
    #[cfg(not(target_os = "windows"))]
    let mut adapter = None;

    if adapter.is_none() {
        adapter = request_adapter(&instance);
    }
    if adapter.is_none() {
        let fallback_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        adapter = request_adapter(&fallback_instance);
        // Last resort: software (WARP) adapter — always available on Windows.
        if adapter.is_none() {
            adapter = pollster::block_on(fallback_instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::None,
                    compatible_surface: None,
                    force_fallback_adapter: true,
                },
            ))
            .ok();
        }
    }
    let adapter = adapter.ok_or("Failed to find GPU adapter")?;
    let adapter_info = adapter.get_info();
    println!(
        "[Export][GPU] Selected backend={:?} vendor=0x{:04x} device=0x{:04x} name={} driver={}",
        adapter_info.backend,
        adapter_info.vendor,
        adapter_info.device,
        adapter_info.name,
        adapter_info.driver
    );

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("SGT GPU Compositor"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| format!("Failed to create device: {}", e))?;

    let device = Arc::new(device);
    let queue = Arc::new(queue);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Compositor Shader"),
        source: wgpu::ShaderSource::Wgsl(compositor_shader().into()),
    });

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(QUAD_VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(
                    std::mem::size_of::<CompositorUniforms>() as u64
                ),
            },
            count: None,
        }],
    });

    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Texture Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let background_overlay_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Background Overlay Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[
            &uniform_layout,
            &texture_layout,
            &texture_layout,
            &background_overlay_layout,
        ],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: OUTPUT_TEXTURE_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let accumulate_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Accumulate Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: OUTPUT_TEXTURE_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Constant,
                        dst_factor: wgpu::BlendFactor::OneMinusConstant,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Constant,
                        dst_factor: wgpu::BlendFactor::OneMinusConstant,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // Atlas texture bind group layout (tex + sampler) for the overlay pipeline.
    let atlas_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Atlas Texture Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let overlay_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Overlay Shader"),
        source: wgpu::ShaderSource::Wgsl(overlay_shader().into()),
    });

    let overlay_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Overlay Pipeline Layout"),
        bind_group_layouts: &[&atlas_texture_layout],
        immediate_size: 0,
    });

    let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Overlay Pipeline"),
        layout: Some(&overlay_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &overlay_shader_module,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 16,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &overlay_shader_module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: OUTPUT_TEXTURE_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    Ok(SharedGpuContext {
        device,
        queue,
        pipeline,
        accumulate_pipeline,
        overlay_pipeline,
        vertex_buffer,
        uniform_layout,
        texture_layout,
        background_overlay_layout,
        atlas_texture_layout,
    })
}

pub(super) fn shared_gpu_context() -> Result<&'static SharedGpuContext, String> {
    match SHARED_GPU_CONTEXT.get_or_init(create_shared_gpu_context) {
        Ok(ctx) => Ok(ctx),
        Err(err) => Err(err.clone()),
    }
}

pub struct CompositorUniformParams {
    pub video_offset: (f32, f32),
    pub video_scale: (f32, f32),
    pub output_size: (f32, f32),
    pub video_size: (f32, f32),
    pub border_radius: f32,
    pub shadow_offset: f32,
    pub shadow_blur: f32,
    pub shadow_opacity: f32,
    pub gradient_color1: [f32; 4],
    pub gradient_color2: [f32; 4],
    pub gradient_color3: [f32; 4],
    pub gradient_color4: [f32; 4],
    pub gradient_color5: [f32; 4],
    pub bg_params1: [f32; 4],
    pub bg_params2: [f32; 4],
    pub bg_params3: [f32; 4],
    pub bg_params4: [f32; 4],
    pub bg_params5: [f32; 4],
    pub bg_params6: [f32; 4],
    pub time: f32,
    pub render_mode: f32,
    pub cursor_pos: (f32, f32),
    pub cursor_scale: f32,
    pub cursor_opacity: f32,
    pub cursor_type_id: f32,
    pub cursor_rotation: f32,
    pub cursor_shadow: f32,
    pub use_background_texture: bool,
    pub bg_zoom: f32,
    pub bg_anchor: (f32, f32),
    pub background_style: f32,
    pub bg_tex_w: f32,
    pub bg_tex_h: f32,
}

pub fn create_uniforms(params: CompositorUniformParams) -> CompositorUniforms {
    let CompositorUniformParams {
        video_offset,
        video_scale,
        output_size,
        video_size,
        border_radius,
        shadow_offset,
        shadow_blur,
        shadow_opacity,
        gradient_color1,
        gradient_color2,
        gradient_color3,
        gradient_color4,
        gradient_color5,
        bg_params1,
        bg_params2,
        bg_params3,
        bg_params4,
        bg_params5,
        bg_params6,
        time,
        render_mode,
        cursor_pos,
        cursor_scale,
        cursor_opacity,
        cursor_type_id,
        cursor_rotation,
        cursor_shadow,
        use_background_texture,
        bg_zoom,
        bg_anchor,
        background_style,
        bg_tex_w,
        bg_tex_h,
    } = params;
    CompositorUniforms {
        video_offset: [video_offset.0, video_offset.1],
        video_scale: [video_scale.0, video_scale.1],
        output_size: [output_size.0, output_size.1],
        video_size: [video_size.0, video_size.1],
        border_radius,
        shadow_offset,
        shadow_blur,
        shadow_opacity,
        gradient_color1,
        gradient_color2,
        gradient_color3,
        gradient_color4,
        gradient_color5,
        time,
        render_mode,
        cursor_pos: [cursor_pos.0, cursor_pos.1],
        cursor_scale,
        cursor_opacity,
        cursor_type_id,
        cursor_rotation,
        cursor_shadow,
        use_background_texture: if use_background_texture { 1.0 } else { 0.0 },
        bg_zoom,
        bg_anchor_x: bg_anchor.0,
        bg_anchor_y: bg_anchor.1,
        bg_style: background_style,
        bg_tex_w,
        bg_tex_h,
        bg_params1,
        bg_params2,
        bg_params3,
        bg_params4,
        bg_params5,
        bg_params6,
    }
}
