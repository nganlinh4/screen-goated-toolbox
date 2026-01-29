use bytemuck::{Pod, Zeroable};
use resvg::usvg::{self, Options, Tree};
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use wgpu::util::DeviceExt;

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
    pub cursor_pos: [f32; 2],      // 88-96
    pub cursor_scale: f32,         // 96-100
    pub cursor_clicked: f32,       // 100-104 - DEPRECATED in shader but kept for struct alignment
    pub cursor_type_id: f32,       // 104-108
    pub _pad2: f32,                // 108-112
    pub _pad3: [f32; 4],           // 112-128 (Total 128 bytes)
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

// Embed the pointer SVG for the "Hand" cursor
const POINTER_SVG: &[u8] = include_bytes!("dist/pointer.svg");

impl GpuCompositor {
    pub fn new(
        output_width: u32,
        output_height: u32,
        video_width: u32,
        video_height: u32,
    ) -> Result<Self, String> {
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compositor Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITOR_SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<CompositorUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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

        // Cursor Texture Atlas: 512x1536 (3 vertical tiles of 512x512)
        // Large tiles for crisp cursors even at high zoom levels
        let cursor_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cursor Atlas Texture"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512 * 3, // 3 tiles vertical
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

        let video_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Cursor sampler: use Nearest to avoid atlas bleeding between tiles
        let cursor_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

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

        let output_buffer_size = (output_width * output_height * 4) as u64;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
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
                    resource: wgpu::BindingResource::Sampler(&video_sampler),
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
                    resource: wgpu::BindingResource::Sampler(&cursor_sampler),
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
        // Build atlas: 512x1536 (3 tiles of 512x512)
        // Slot 0: Arrow (Default)
        // Slot 1: Text (I-Beam)
        // Slot 2: Pointer (Hand)

        let tile_size = 512u32;
        let center = tile_size as f32 / 2.0; // 256.0
        let mut atlas = Pixmap::new(tile_size, tile_size * 3).unwrap();

        // Scale factor: 8x
        let cursor_scale = 8.0;

        // --- SLOT 0: DEFAULT ARROW ---
        {
            let mut pb = PathBuilder::new();
            pb.move_to(8.2, 4.9);
            pb.line_to(19.8, 16.5);
            pb.line_to(13.0, 16.5);
            pb.line_to(12.6, 16.6);
            pb.line_to(8.2, 20.9);
            pb.close();
            let path = pb.finish().unwrap();

            let mut pb2 = PathBuilder::new();
            pb2.move_to(17.3, 21.6);
            pb2.line_to(13.7, 23.1);
            pb2.line_to(9.0, 12.0);
            pb2.line_to(12.7, 10.5);
            pb2.close();
            let click_path = pb2.finish().unwrap();

            let paint_fill = Paint {
                shader: tiny_skia::Shader::SolidColor(Color::BLACK),
                ..Default::default()
            };
            let paint_stroke = Paint {
                shader: tiny_skia::Shader::SolidColor(Color::WHITE),
                ..Default::default()
            };
            let stroke = Stroke {
                width: 1.5,
                ..Default::default()
            };

            let ts = Transform::from_translate(190.4, 216.8).pre_scale(cursor_scale, cursor_scale);

            atlas.stroke_path(&path, &paint_stroke, &stroke, ts, None);
            atlas.stroke_path(&click_path, &paint_stroke, &stroke, ts, None);
            atlas.fill_path(&path, &paint_fill, FillRule::Winding, ts, None);
            atlas.fill_path(&click_path, &paint_fill, FillRule::Winding, ts, None);
        }

        // --- SLOT 1: TEXT I-BEAM ---
        {
            let mut pb = PathBuilder::new();
            pb.move_to(2.0, 0.0);
            pb.line_to(10.0, 0.0);
            pb.line_to(10.0, 2.0);
            pb.line_to(7.0, 2.0);
            pb.line_to(7.0, 14.0);
            pb.line_to(10.0, 14.0);
            pb.line_to(10.0, 16.0);
            pb.line_to(2.0, 16.0);
            pb.line_to(2.0, 14.0);
            pb.line_to(5.0, 14.0);
            pb.line_to(5.0, 2.0);
            pb.line_to(2.0, 2.0);
            pb.close();
            let path = pb.finish().unwrap();

            let paint_fill = Paint {
                shader: tiny_skia::Shader::SolidColor(Color::BLACK),
                ..Default::default()
            };
            let paint_stroke = Paint {
                shader: tiny_skia::Shader::SolidColor(Color::WHITE),
                ..Default::default()
            };
            let stroke = Stroke {
                width: 1.5,
                ..Default::default()
            };

            let ts = Transform::from_translate(208.0, 704.0).pre_scale(cursor_scale, cursor_scale);

            atlas.stroke_path(&path, &paint_stroke, &stroke, ts, None);
            atlas.fill_path(&path, &paint_fill, FillRule::Winding, ts, None);
        }

        // --- SLOT 2: HAND POINTER ---
        {
            let opt = Options::default();
            if let Ok(tree) = Tree::from_data(POINTER_SVG, &opt) {
                let svg_size = tree.size();
                let target_size = 480.0;
                let scale = target_size / svg_size.width().max(svg_size.height());

                let hotspot_in_img_x = (svg_size.width() / 2.0 - 8.0) * scale;
                let hotspot_in_img_y = (svg_size.height() / 2.0 - 16.0) * scale;
                let x = center - hotspot_in_img_x;
                let y = (center + tile_size as f32 * 2.0) - hotspot_in_img_y;

                let ts = Transform::from_translate(x, y).pre_scale(scale, scale);

                resvg::render(&tree, ts, &mut atlas.as_mut());
            }
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cursor_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.data(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512 * 4),
                rows_per_image: Some(512 * 3),
            },
            wgpu::Extent3d {
                width: 512,
                height: 512 * 3,
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
    cursor_type_id: f32,
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
        cursor_type_id,
        _pad2: 0.0,
        _pad3: [0.0; 4],
    }
}

// Updated Shader with atlas support
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
    cursor_type_id: f32,
    _pad2: f32,
    _pad3: vec4<f32>,
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

// Hotspot offset function
fn get_hotspot(type_id: f32, size: f32) -> vec2<f32> {
    // All cursor types have hotspot at center of the 512x512 tile
    // To align hotspot with cursor_pos, offset by half the rendered size
    return vec2<f32>(size * 0.5, size * 0.5);
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
        // Improved shadow softness matching canvas
        let sh_alpha = 1.0 - smoothstep(-u.shadow_blur, u.shadow_blur, sh_dist);
        col = mix(col, vec4<f32>(0.0,0.0,0.0, u.shadow_opacity), sh_alpha * u.shadow_opacity);
    }
    
    // 3. Video Content + Cursor
    if dist < 0.0 {
        let vid_uv = (in.pixel_pos - u.video_offset * u.output_size) / u.video_size;
        var vid_col = textureSample(video_tex, video_samp, vid_uv);
        
        // Render Cursor
        if u.cursor_pos.x >= 0.0 {
            let cursor_pixel_size = 48.0 * u.cursor_scale;
            let cursor_px = u.cursor_pos * u.video_size;
            let pixel_rel = in.pixel_pos - (u.video_offset * u.output_size);
            let hotspot = get_hotspot(u.cursor_type_id, cursor_pixel_size);
            let sample_pos = (pixel_rel - cursor_px) + hotspot;

            if sample_pos.x >= 0.0 && sample_pos.x < cursor_pixel_size && 
               sample_pos.y >= 0.0 && sample_pos.y < cursor_pixel_size {
               
               let uv_in_tile = sample_pos / cursor_pixel_size;
               
               // Atlas mapping: 512x1536 (3 vertical tiles of 512)
               let tile_idx = floor(u.cursor_type_id + 0.5);
               let atlas_uv = vec2<f32>(
                   uv_in_tile.x,
                   (uv_in_tile.y + tile_idx) / 3.0
               );
               
               let cur_col = textureSample(cursor_tex, cursor_samp, atlas_uv);
               
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
