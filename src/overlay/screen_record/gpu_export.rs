use bytemuck::{Pod, Zeroable};
use resvg::usvg::{self, Options, Tree};
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use wgpu::util::DeviceExt;

// Vertex for fullscreen quad
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

const QUAD_VERTICES: &[Vertex] = &[
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

// Uniforms for the compositor shader
// Aligned to 16 bytes to match WGSL requirements
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
    pub _pad1: f32,                // 84-88 (Aligns cursor_pos to 8 bytes)
    pub cursor_pos: [f32; 2],      // 88-96
    pub cursor_scale: f32,         // 96-100
    pub cursor_clicked: f32,       // 100-104
    pub _pad2: [f32; 6],           // 104-128 (Fills remaining 24 bytes)
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
    cursor_texture: wgpu::Texture,
    cursor_bind_group: wgpu::BindGroup,
    output_texture: wgpu::Texture,
    output_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    video_width: u32,
    video_height: u32,
}

// Embed the pointer SVG
const CURSOR_BYTES: &[u8] = include_bytes!("dist/pointer.svg");

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

        // Video texture
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

        // Cursor texture
        let cursor_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cursor Texture"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let cursor_view = cursor_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Output texture
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

        // Output buffer
        let output_buffer_size = (output_width * output_height * 4) as u64;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Layouts
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // Visibility includes VERTEX because vs_main uses 'u' for scaling
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let video_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Video BG"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&video_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let cursor_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cursor BG"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cursor_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &texture_layout, &texture_layout],
            push_constant_ranges: &[],
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
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
            cursor_texture,
            cursor_bind_group,
            output_texture,
            output_buffer,
            width: output_width,
            height: output_height,
            video_width,
            video_height,
        })
    }

    pub fn init_cursor_texture(&self) {
        // Rasterize SVG cursor using resvg to match frontend look
        let width = 128;
        let height = 128;

        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Try to parse SVG
        let opt = Options::default();
        if let Ok(tree) = Tree::from_data(CURSOR_BYTES, &opt) {
            let mut pixmap = Pixmap::new(width, height).unwrap();

            // Calculate scale to fit 128x128
            let svg_size = tree.size();
            let scale_x = width as f32 / svg_size.width();
            let scale_y = height as f32 / svg_size.height();
            let scale = scale_x.min(scale_y);

            let transform = Transform::from_scale(scale, scale);

            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let data = pixmap.data();
            if data.len() == pixels.len() {
                pixels.copy_from_slice(data);
            }
        } else {
            // Fallback: Generate a simple white arrow if SVG fails
            println!("[GpuCompositor] Failed to load cursor SVG, using fallback.");
            for y in 0..height {
                for x in 0..width {
                    let fx = x as f32;
                    let fy = y as f32;
                    // Simple triangle
                    let in_arrow = (fx < fy * 0.7 && fx < 32.0 && fy < 48.0 && fx > 0.0);
                    if in_arrow {
                        let idx = ((y * width + x) * 4) as usize;
                        pixels[idx] = 255;
                        pixels[idx + 1] = 255;
                        pixels[idx + 2] = 255;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cursor_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
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
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let output_view = self
                .output_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
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

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.video_bind_group, &[]);
            pass.set_bind_group(2, &self.cursor_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..6, 0..1);
        }

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
    cursor_pos: (f32, f32),
    cursor_scale: f32,
    cursor_clicked: f32,
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
        cursor_pos: [cursor_pos.0, cursor_pos.1],
        cursor_scale,
        cursor_clicked,
        _pad2: [0.0; 6],
    }
}

// Updated Shader with compatible padding fields
const COMPOSITOR_SHADER: &str = r#"
struct Uniforms {
    video_offset: vec2<f32>,
    video_scale: vec2<f32>,
    output_size: vec2<f32>,
    video_size: vec2<f32>,
    border_radius: f32,
    shadow_offset: f32,
    shadow_blur: f32,
    shadow_opacity: f32,
    gradient_color1: vec4<f32>,
    gradient_color2: vec4<f32>,
    time: f32,
    _pad1: f32,
    cursor_pos: vec2<f32>,
    cursor_scale: f32,
    cursor_clicked: f32,
    _pad2_a: vec2<f32>, // WGSL doesn't like array strides in uniform blocks, so use vec2+vec4 for 24 bytes
    _pad2_b: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

@group(1) @binding(0) var video_tex: texture_2d<f32>;
@group(1) @binding(1) var video_samp: sampler;

@group(2) @binding(0) var cursor_tex: texture_2d<f32>;
@group(2) @binding(1) var cursor_samp: sampler;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = vec4<f32>(pos, 0.0, 1.0);
    out.tex_coord = uv;
    out.pixel_pos = uv * u.output_size;
    return out;
}

fn sd_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // 1. Background
    let t = in.tex_coord.x;
    var col = mix(u.gradient_color1, u.gradient_color2, t);
    
    // Video positioning
    let vid_center = u.video_offset * u.output_size + u.video_size * 0.5;
    let vid_half = u.video_size * 0.5;
    let dist = sd_box(in.pixel_pos - vid_center, vid_half, u.border_radius);
    
    // 2. Shadow
    if u.shadow_opacity > 0.0 {
        let sh_center = vid_center + vec2<f32>(u.shadow_offset);
        let sh_dist = sd_box(in.pixel_pos - sh_center, vid_half, u.border_radius);
        let sh_alpha = 1.0 - smoothstep(-u.shadow_blur, u.shadow_blur, sh_dist);
        col = mix(col, vec4<f32>(0.0,0.0,0.0, u.shadow_opacity), sh_alpha * u.shadow_opacity);
    }
    
    // 3. Video Content + Cursor
    if dist < 0.0 {
        let vid_uv = (in.pixel_pos - u.video_offset * u.output_size) / u.video_size;
        var vid_col = textureSample(video_tex, video_samp, vid_uv);
        
        // Render Cursor
        if u.cursor_pos.x >= 0.0 {
            // Screen space cursor rendering
            let cursor_pixel_size = 32.0 * u.cursor_scale * (1.0 - u.cursor_clicked * 0.2);
            let cur_center_screen_rel = u.cursor_pos * u.video_size; 
            
            // Pixel relative to video top-left
            let pixel_rel = in.pixel_pos - (u.video_offset * u.output_size);
            let d_cursor = pixel_rel - cur_center_screen_rel;
            
            // Offset logic for standard arrow cursor (top-left hotspot)
            let hotspot_offset = vec2<f32>(cursor_pixel_size * 0.0, cursor_pixel_size * 0.0);
            let sample_pos = d_cursor + hotspot_offset;

            if sample_pos.x >= 0.0 && sample_pos.x < cursor_pixel_size && 
               sample_pos.y >= 0.0 && sample_pos.y < cursor_pixel_size {
               
               let cursor_uv = sample_pos / cursor_pixel_size;
               let cur_col = textureSample(cursor_tex, cursor_samp, cursor_uv);
               
               // Blend
               vid_col = mix(vid_col, cur_col, cur_col.a);
            }
        }
        
        // Anti-aliased video edge
        let edge = 1.0 - smoothstep(-1.5, 0.0, dist);
        col = mix(col, vid_col, edge);
    }
    
    return col;
}
"#;
