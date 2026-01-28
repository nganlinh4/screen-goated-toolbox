use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;

// Vertex for fullscreen quad
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

const QUAD_VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [1.0, -1.0], tex_coords: [1.0, 1.0] },
    Vertex { position: [1.0, 1.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [1.0, 1.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [-1.0, 1.0], tex_coords: [0.0, 0.0] },
];

// Uniforms for the compositor shader
// Must match WGSL struct layout with proper alignment (vec4 = 16-byte aligned)
// Total size: 112 bytes
#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CompositorUniforms {
    pub video_offset: [f32; 2],    // 0-8
    pub video_scale: [f32; 2],     // 8-16
    pub output_size: [f32; 2],     // 16-24
    pub video_size: [f32; 2],      // 24-32
    pub border_radius: f32,        // 32-36
    pub shadow_offset: f32,        // 36-40
    pub shadow_blur: f32,          // 40-44
    pub shadow_opacity: f32,       // 44-48
    pub gradient_color1: [f32; 4], // 48-64
    pub gradient_color2: [f32; 4], // 64-80
    pub time: f32,                 // 80-84
    pub _pad1: f32,                // 84-88
    pub _pad2: f32,                // 88-92
    pub _pad3: f32,                // 92-96
    pub _pad4: [f32; 4],           // 96-112
}

// Cursor uniforms
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CursorUniforms {
    position: [f32; 2],
    scale: f32,
    is_clicked: f32,
}

pub struct GpuCompositor {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    video_texture: wgpu::Texture,
    video_bind_group: wgpu::BindGroup,
    output_texture: wgpu::Texture,
    output_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    video_width: u32,
    video_height: u32,
}

impl GpuCompositor {
    pub fn new(
        output_width: u32,
        output_height: u32,
        video_width: u32,
        video_height: u32,
    ) -> Result<Self, String> {
        // Initialize wgpu
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or("Failed to find GPU adapter")?;

        println!("[GPU] Using adapter: {:?}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SGT GPU Compositor"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("Failed to create device: {}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compositor Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITOR_SHADER.into()),
        });

        // Vertex buffer
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<CompositorUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Video texture (input)
        let video_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Video Texture"),
            size: wgpu::Extent3d {
                width: video_width,
                height: video_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let video_view = video_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let video_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Output texture (render target)
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Output Texture"),
            size: wgpu::Extent3d {
                width: output_width,
                height: output_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        // Output buffer for CPU readback
        let output_buffer_size = (output_width * output_height * 4) as u64;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Bind group layouts
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
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

        // Bind groups
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let video_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Video Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&video_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&video_sampler),
                },
            ],
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Compositor Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Compositor Pipeline"),
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            video_texture,
            video_bind_group,
            output_texture,
            output_buffer,
            width: output_width,
            height: output_height,
            video_width,
            video_height,
        })
    }

    pub fn upload_frame(&self, rgba_data: &[u8]) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.video_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.video_width * 4),
                rows_per_image: Some(self.video_height),
            },
            wgpu::Extent3d {
                width: self.video_width,
                height: self.video_height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn render_frame(&self, uniforms: &CompositorUniforms) -> Vec<u8> {
        // Update uniforms
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Render pass
        {
            let output_view = self
                .output_texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Compositor Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.video_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        // Copy output to buffer
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.width * 4),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // Read back
        let buffer_slice = self.output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();

        let data = buffer_slice.get_mapped_range();
        let result = data.to_vec();
        drop(data);
        self.output_buffer.unmap();

        result
    }
}

// Helper to create uniforms
pub fn create_uniforms(
    video_offset: (f32, f32),
    video_scale: (f32, f32),
    output_size: (f32, f32),
    video_size: (f32, f32),
    border_radius: f32,
    shadow_offset: f32,
    shadow_blur: f32,
    shadow_opacity: f32,
    gradient_color1: [f32; 4],
    gradient_color2: [f32; 4],
    time: f32,
) -> CompositorUniforms {
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
        time,
        _pad1: 0.0,
        _pad2: 0.0,
        _pad3: 0.0,
        _pad4: [0.0; 4],
    }
}

// WGSL Shader for compositing
const COMPOSITOR_SHADER: &str = r#"
struct Uniforms {
    video_offset: vec2<f32>,      // 0-8
    video_scale: vec2<f32>,       // 8-16
    output_size: vec2<f32>,       // 16-24
    video_size: vec2<f32>,        // 24-32
    border_radius: f32,           // 32-36
    shadow_offset: f32,           // 36-40
    shadow_blur: f32,             // 40-44
    shadow_opacity: f32,          // 44-48
    gradient_color1: vec4<f32>,   // 48-64
    gradient_color2: vec4<f32>,   // 64-80
    time: f32,                    // 80-84
    _pad1: f32,                   // 84-88
    _pad2: f32,                   // 88-92
    _pad3: f32,                   // 92-96
    _pad4: vec4<f32>,             // 96-112 (vec4 for alignment)
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var video_texture: texture_2d<f32>;
@group(1) @binding(1)
var video_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) frag_pos: vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.frag_pos = in.tex_coords * uniforms.output_size;
    return out;
}

// Signed distance function for rounded rectangle
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.frag_pos;
    
    // Draw gradient background
    let gradient_t = pixel.x / uniforms.output_size.x;
    var color = mix(uniforms.gradient_color1, uniforms.gradient_color2, gradient_t);
    
    // Video rect position and size
    let video_center = uniforms.video_offset * uniforms.output_size + uniforms.video_size * 0.5;
    let video_half_size = uniforms.video_size * 0.5;
    
    // Shadow (offset box with blur approximation)
    if uniforms.shadow_opacity > 0.0 {
        let shadow_center = video_center + vec2<f32>(uniforms.shadow_offset, uniforms.shadow_offset);
        let shadow_dist = sd_rounded_box(
            pixel - shadow_center,
            video_half_size,
            uniforms.border_radius
        );
        let shadow_alpha = 1.0 - smoothstep(-uniforms.shadow_blur, uniforms.shadow_blur * 2.0, shadow_dist);
        let shadow_color = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha * uniforms.shadow_opacity);
        color = mix(color, shadow_color, shadow_color.a);
    }
    
    // Video with rounded corners
    let video_dist = sd_rounded_box(
        pixel - video_center,
        video_half_size,
        uniforms.border_radius
    );
    
    if video_dist < 0.0 {
        // Inside video area - sample video texture
        let video_uv = (pixel - uniforms.video_offset * uniforms.output_size) / uniforms.video_size;
        let video_color = textureSample(video_texture, video_sampler, video_uv);
        
        // Anti-aliased edge
        let edge_alpha = 1.0 - smoothstep(-1.0, 1.0, video_dist);
        color = mix(color, video_color, edge_alpha);
    }
    
    return color;
}
"#;
