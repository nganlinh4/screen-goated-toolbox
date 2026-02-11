use bytemuck::{Pod, Zeroable};
use resvg::usvg::{Options, Tree};
use std::sync::{Arc, OnceLock};
use std::sync::Mutex;
use tiny_skia::{Pixmap, Transform};
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
    pub cursor_opacity: f32,       // 100-104 - cursor visibility (0.0 = hidden, 1.0 = fully visible)
    pub cursor_type_id: f32,       // 104-108
    pub cursor_rotation: f32,      // 108-112 (radians, tip anchored)
    pub cursor_shadow: f32,        // 112-116 (0-1)
    pub _pad3: [f32; 3],           // 116-128 (Total 128 bytes)
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
    padded_bytes_per_row: u32,
    video_width: u32,
    video_height: u32,
}

const DEFAULT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-default-screenstudio.svg");
const TEXT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-text-screenstudio.svg");
const POINTER_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-pointer-screenstudio.svg");
const OPENHAND_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-openhand-screenstudio.svg");
const CLOSEHAND_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-closehand-screenstudio.svg");
const WAIT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-wait-screenstudio.svg");
const APPSTARTING_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-screenstudio.svg");
const CROSSHAIR_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-screenstudio.svg");
const RESIZE_NS_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-screenstudio.svg");
const RESIZE_WE_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-screenstudio.svg");
const RESIZE_NWSE_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-screenstudio.svg");
const RESIZE_NESW_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-screenstudio.svg");
const DEFAULT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-default-macos26.svg");
const TEXT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-text-macos26.svg");
const POINTER_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-pointer-macos26.svg");
const OPENHAND_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-openhand-macos26.svg");
const CLOSEHAND_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-closehand-macos26.svg");
const WAIT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-wait-macos26.svg");
const APPSTARTING_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-macos26.svg");
const CROSSHAIR_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-macos26.svg");
const RESIZE_NS_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-macos26.svg");
const RESIZE_WE_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-macos26.svg");
const RESIZE_NWSE_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-macos26.svg");
const RESIZE_NESW_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-macos26.svg");
const DEFAULT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtcute.svg");
const TEXT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtcute.svg");
const POINTER_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtcute.svg");
const OPENHAND_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtcute.svg");
const CLOSEHAND_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtcute.svg");
const WAIT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtcute.svg");
const APPSTARTING_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtcute.svg");
const CROSSHAIR_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtcute.svg");
const RESIZE_NS_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtcute.svg");
const RESIZE_WE_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtcute.svg");
const RESIZE_NWSE_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtcute.svg");
const RESIZE_NESW_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtcute.svg");
const DEFAULT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtcool.svg");
const TEXT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtcool.svg");
const POINTER_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtcool.svg");
const OPENHAND_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtcool.svg");
const CLOSEHAND_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtcool.svg");
const WAIT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtcool.svg");
const APPSTARTING_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtcool.svg");
const CROSSHAIR_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtcool.svg");
const RESIZE_NS_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtcool.svg");
const RESIZE_WE_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtcool.svg");
const RESIZE_NWSE_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtcool.svg");
const RESIZE_NESW_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtcool.svg");
const DEFAULT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtai.svg");
const TEXT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtai.svg");
const POINTER_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtai.svg");
const OPENHAND_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtai.svg");
const CLOSEHAND_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtai.svg");
const WAIT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtai.svg");
const APPSTARTING_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtai.svg");
const CROSSHAIR_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtai.svg");
const RESIZE_NS_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtai.svg");
const RESIZE_WE_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtai.svg");
const RESIZE_NWSE_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtai.svg");
const RESIZE_NESW_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtai.svg");
const DEFAULT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtpixel.svg");
const TEXT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtpixel.svg");
const POINTER_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtpixel.svg");
const OPENHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtpixel.svg");
const CLOSEHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtpixel.svg");
const WAIT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtpixel.svg");
const APPSTARTING_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtpixel.svg");
const CROSSHAIR_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtpixel.svg");
const RESIZE_NS_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtpixel.svg");
const RESIZE_WE_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtpixel.svg");
const RESIZE_NWSE_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtpixel.svg");
const RESIZE_NESW_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtpixel.svg");
const CURSOR_ATLAS_COLS: u32 = 9;
const CURSOR_ATLAS_SLOTS: u32 = CURSOR_SVG_DATA.len() as u32;
const CURSOR_ATLAS_ROWS: u32 = (CURSOR_ATLAS_SLOTS + CURSOR_ATLAS_COLS - 1) / CURSOR_ATLAS_COLS;
const CURSOR_TILE_SIZE: u32 = 512;
static SHARED_GPU_CONTEXT: OnceLock<Result<SharedGpuContext, String>> = OnceLock::new();
static CURSOR_TILE_CACHE: OnceLock<Mutex<Vec<Option<Arc<Vec<u8>>>>>> = OnceLock::new();

struct SharedGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
}

fn create_shared_gpu_context() -> Result<SharedGpuContext, String> {
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

    Ok(SharedGpuContext {
        device,
        queue,
        pipeline,
        vertex_buffer,
        uniform_layout,
        texture_layout,
    })
}

fn shared_gpu_context() -> Result<&'static SharedGpuContext, String> {
    match SHARED_GPU_CONTEXT.get_or_init(create_shared_gpu_context) {
        Ok(ctx) => Ok(ctx),
        Err(err) => Err(err.clone()),
    }
}

// Add new cursor pack SVGs here — SLOTS, ROWS, and shader constants auto-update.
const CURSOR_SVG_DATA: &[&[u8]] = &[
    // screenstudio
    DEFAULT_SCREENSTUDIO_SVG,
    TEXT_SCREENSTUDIO_SVG,
    POINTER_SCREENSTUDIO_SVG,
    OPENHAND_SCREENSTUDIO_SVG,
    CLOSEHAND_SCREENSTUDIO_SVG,
    WAIT_SCREENSTUDIO_SVG,
    APPSTARTING_SCREENSTUDIO_SVG,
    CROSSHAIR_SCREENSTUDIO_SVG,
    RESIZE_NS_SCREENSTUDIO_SVG,
    RESIZE_WE_SCREENSTUDIO_SVG,
    RESIZE_NWSE_SCREENSTUDIO_SVG,
    RESIZE_NESW_SCREENSTUDIO_SVG,
    // macos26
    DEFAULT_MACOS26_SVG,
    TEXT_MACOS26_SVG,
    POINTER_MACOS26_SVG,
    OPENHAND_MACOS26_SVG,
    CLOSEHAND_MACOS26_SVG,
    WAIT_MACOS26_SVG,
    APPSTARTING_MACOS26_SVG,
    CROSSHAIR_MACOS26_SVG,
    RESIZE_NS_MACOS26_SVG,
    RESIZE_WE_MACOS26_SVG,
    RESIZE_NWSE_MACOS26_SVG,
    RESIZE_NESW_MACOS26_SVG,
    // sgtcute
    DEFAULT_SGTCUTE_SVG,
    TEXT_SGTCUTE_SVG,
    POINTER_SGTCUTE_SVG,
    OPENHAND_SGTCUTE_SVG,
    CLOSEHAND_SGTCUTE_SVG,
    WAIT_SGTCUTE_SVG,
    APPSTARTING_SGTCUTE_SVG,
    CROSSHAIR_SGTCUTE_SVG,
    RESIZE_NS_SGTCUTE_SVG,
    RESIZE_WE_SGTCUTE_SVG,
    RESIZE_NWSE_SGTCUTE_SVG,
    RESIZE_NESW_SGTCUTE_SVG,
    // sgtcool
    DEFAULT_SGTCOOL_SVG,
    TEXT_SGTCOOL_SVG,
    POINTER_SGTCOOL_SVG,
    OPENHAND_SGTCOOL_SVG,
    CLOSEHAND_SGTCOOL_SVG,
    WAIT_SGTCOOL_SVG,
    APPSTARTING_SGTCOOL_SVG,
    CROSSHAIR_SGTCOOL_SVG,
    RESIZE_NS_SGTCOOL_SVG,
    RESIZE_WE_SGTCOOL_SVG,
    RESIZE_NWSE_SGTCOOL_SVG,
    RESIZE_NESW_SGTCOOL_SVG,
    // sgtai
    DEFAULT_SGTAI_SVG,
    TEXT_SGTAI_SVG,
    POINTER_SGTAI_SVG,
    OPENHAND_SGTAI_SVG,
    CLOSEHAND_SGTAI_SVG,
    WAIT_SGTAI_SVG,
    APPSTARTING_SGTAI_SVG,
    CROSSHAIR_SGTAI_SVG,
    RESIZE_NS_SGTAI_SVG,
    RESIZE_WE_SGTAI_SVG,
    RESIZE_NWSE_SGTAI_SVG,
    RESIZE_NESW_SGTAI_SVG,
    // sgtpixel
    DEFAULT_SGTPIXEL_SVG,
    TEXT_SGTPIXEL_SVG,
    POINTER_SGTPIXEL_SVG,
    OPENHAND_SGTPIXEL_SVG,
    CLOSEHAND_SGTPIXEL_SVG,
    WAIT_SGTPIXEL_SVG,
    APPSTARTING_SGTPIXEL_SVG,
    CROSSHAIR_SGTPIXEL_SVG,
    RESIZE_NS_SGTPIXEL_SVG,
    RESIZE_WE_SGTPIXEL_SVG,
    RESIZE_NWSE_SGTPIXEL_SVG,
    RESIZE_NESW_SGTPIXEL_SVG,
];

fn cursor_tile_cache() -> &'static Mutex<Vec<Option<Arc<Vec<u8>>>>> {
    CURSOR_TILE_CACHE.get_or_init(|| Mutex::new(vec![None; CURSOR_ATLAS_SLOTS as usize]))
}

fn render_cursor_tile_rgba(slot: u32) -> Option<Vec<u8>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    let tile_size = CURSOR_TILE_SIZE;
    let center = tile_size as f32 / 2.0;
    let mut tile = Pixmap::new(tile_size, tile_size).unwrap();
    let target = if slot == 1 || slot == 13 || slot == 25 || slot == 37 || slot == 49 || slot == 61 {
        tile_size as f32 * 0.90
    } else {
        tile_size as f32 * 0.94
    };

    let opt = Options::default();
    let tree = Tree::from_data(CURSOR_SVG_DATA[slot as usize], &opt).ok()?;
    let svg_size = tree.size();
    let svg_w = svg_size.width().max(1.0);
    let svg_h = svg_size.height().max(1.0);
    let base_scale = target / svg_w.max(svg_h);
    let hotspot_px_x = (svg_w * 0.5) * base_scale;
    let hotspot_px_y = (svg_h * 0.5) * base_scale;
    let x = center - hotspot_px_x;
    let y = center - hotspot_px_y;
    let ts = Transform::from_translate(x, y).pre_scale(base_scale, base_scale);
    resvg::render(&tree, ts, &mut tile.as_mut());

    Some(tile.data().to_vec())
}

fn get_or_render_cursor_tile(slot: u32) -> Option<Arc<Vec<u8>>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    {
        let cache = cursor_tile_cache().lock().unwrap();
        if let Some(bytes) = &cache[slot as usize] {
            return Some(Arc::clone(bytes));
        }
    }

    let rendered = Arc::new(render_cursor_tile_rgba(slot)?);
    let mut cache = cursor_tile_cache().lock().unwrap();
    if let Some(bytes) = &cache[slot as usize] {
        Some(Arc::clone(bytes))
    } else {
        cache[slot as usize] = Some(Arc::clone(&rendered));
        Some(rendered)
    }
}

fn dedupe_valid_slots(slots: &[u32]) -> Vec<u32> {
    let mut seen = [false; CURSOR_ATLAS_SLOTS as usize];
    let mut out = Vec::with_capacity(slots.len().max(1));
    for slot in slots {
        let idx = *slot as usize;
        if idx >= CURSOR_ATLAS_SLOTS as usize || seen[idx] {
            continue;
        }
        seen[idx] = true;
        out.push(*slot);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

impl GpuCompositor {
    fn upload_cursor_slot_rgba(&self, slot: u32, rgba: &[u8]) {
        let col = slot % CURSOR_ATLAS_COLS;
        let row = slot / CURSOR_ATLAS_COLS;

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cursor_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: col * CURSOR_TILE_SIZE,
                    y: row * CURSOR_TILE_SIZE,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(CURSOR_TILE_SIZE * 4),
                rows_per_image: Some(CURSOR_TILE_SIZE),
            },
            wgpu::Extent3d {
                width: CURSOR_TILE_SIZE,
                height: CURSOR_TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn new(
        output_width: u32,
        output_height: u32,
        video_width: u32,
        video_height: u32,
    ) -> Result<Self, String> {
        let shared = shared_gpu_context()?;
        let device = Arc::clone(&shared.device);
        let queue = Arc::clone(&shared.queue);
        let pipeline = shared.pipeline.clone();
        let vertex_buffer = shared.vertex_buffer.clone();

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

        // Cursor Texture Atlas: (CURSOR_TILE_SIZE*CURSOR_ATLAS_COLS) x (CURSOR_TILE_SIZE*CURSOR_ATLAS_ROWS)
        // 2D atlas keeps 512px cursor tiles while staying under GPU texture limits.
        let cursor_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cursor Atlas Texture"),
            size: wgpu::Extent3d {
                width: CURSOR_TILE_SIZE * CURSOR_ATLAS_COLS,
                height: CURSOR_TILE_SIZE * CURSOR_ATLAS_ROWS,
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

        // Cursor sampler: Linear for smooth antialiased edges. The 8x rasterized
        // cursors have AA from tiny_skia; Nearest would destroy sub-pixel smoothing.
        // Atlas tile bleeding is not an issue — cursors are centered in 512×512 tiles
        // with >60px transparent padding on each side.
        let cursor_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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

        let unpadded_bytes_per_row = output_width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) / align * align;
        let output_buffer_size = (padded_bytes_per_row * output_height) as u64;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let uniform_layout = shared.uniform_layout.clone();
        let texture_layout = shared.texture_layout.clone();

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
            padded_bytes_per_row,
            video_width,
            video_height,
        })
    }

    pub fn init_cursor_texture_fast(&self, slots: &[u32]) -> bool {
        for slot in dedupe_valid_slots(slots) {
            if let Some(tile) = get_or_render_cursor_tile(slot) {
                self.upload_cursor_slot_rgba(slot, tile.as_slice());
            }
        }
        false
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

    pub fn render_frame_into(&self, uniforms: &CompositorUniforms, out: &mut Vec<u8>) {
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
                    bytes_per_row: Some(self.padded_bytes_per_row),
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
        let unpadded = self.width * 4;
        if self.padded_bytes_per_row == unpadded {
            out.clear();
            out.extend_from_slice(&data);
        } else {
            out.clear();
            out.reserve((unpadded * self.height) as usize);
            for row in data.chunks(self.padded_bytes_per_row as usize) {
                out.extend_from_slice(&row[..unpadded as usize]);
            }
        }
        drop(data);
        self.output_buffer.unmap();
    }

    pub fn render_frame(&self, uniforms: &CompositorUniforms) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.width * self.height * 4) as usize);
        self.render_frame_into(uniforms, &mut out);
        out
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
    cursor_opacity: f32,
    cursor_type_id: f32,
    cursor_rotation: f32,
    cursor_shadow: f32,
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
        cursor_opacity,
        cursor_type_id,
        cursor_rotation,
        cursor_shadow,
        _pad3: [0.0; 3],
    }
}

// Updated Shader with atlas support
// NOTE: COMPOSITOR_SHADER_BODY uses WGSL constants ATLAS_COLS / ATLAS_ROWS
// which are injected by compositor_shader() from the Rust CURSOR_ATLAS_* values.
// This guarantees the shader always matches the atlas layout — no manual sync needed.
const COMPOSITOR_SHADER_BODY: &str = r#"
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
    cursor_opacity: f32,
    cursor_type_id: f32,
    cursor_rotation: f32,
    cursor_shadow: f32,
    _pad3: f32,
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

fn get_rotation_pivot(type_id: f32, size: f32) -> vec2<f32> {
    let unit = size / 48.0;
    if abs(type_id - 2.0) < 0.5 || abs(type_id - 3.0) < 0.5 || abs(type_id - 4.0) < 0.5
        || abs(type_id - 14.0) < 0.5 || abs(type_id - 15.0) < 0.5 || abs(type_id - 16.0) < 0.5
        || abs(type_id - 26.0) < 0.5 || abs(type_id - 27.0) < 0.5 || abs(type_id - 28.0) < 0.5
        || abs(type_id - 38.0) < 0.5 || abs(type_id - 39.0) < 0.5 || abs(type_id - 40.0) < 0.5
        || abs(type_id - 50.0) < 0.5 || abs(type_id - 51.0) < 0.5 || abs(type_id - 52.0) < 0.5
        || abs(type_id - 62.0) < 0.5 || abs(type_id - 63.0) < 0.5 || abs(type_id - 64.0) < 0.5 {
        // hand cursors
        return vec2<f32>(3.0 * unit, 8.5 * unit);
    }
    if abs(type_id - 1.0) < 0.5 || abs(type_id - 13.0) < 0.5 || abs(type_id - 25.0) < 0.5 || abs(type_id - 37.0) < 0.5 || abs(type_id - 49.0) < 0.5 || abs(type_id - 61.0) < 0.5 {
        // text ibeam should stay upright
        return vec2<f32>(0.0, 0.0);
    }
    // default arrow
    return vec2<f32>(3.6 * unit, 5.6 * unit);
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
    
    // 3. Video Content
    if dist < 0.0 {
        let vid_uv = (in.pixel_pos - u.video_offset * u.output_size) / u.video_size;
        var vid_col = textureSample(video_tex, video_samp, vid_uv);

        // Anti-aliased video edge
        let edge = 1.0 - smoothstep(-1.5, 0.0, dist);
        col = mix(col, vid_col, edge);
    }

    // 4. Cursor Overlay (drawn over both video and background)
    if u.cursor_pos.x > -99.0 {
        let cursor_pixel_size = 48.0 * u.cursor_scale;
        let cursor_px = (u.video_offset + (u.cursor_pos * u.video_scale)) * u.output_size;
        let hotspot = get_hotspot(u.cursor_type_id, cursor_pixel_size);
        let pivot = get_rotation_pivot(u.cursor_type_id, cursor_pixel_size);
        let rel = in.pixel_pos - cursor_px;
        let c = cos(-u.cursor_rotation);
        let s = sin(-u.cursor_rotation);
        let rel_pivot = rel - pivot;
        let rel_rot = vec2<f32>(
            rel_pivot.x * c - rel_pivot.y * s,
            rel_pivot.x * s + rel_pivot.y * c
        ) + pivot;
        let sample_pos = rel_rot + hotspot;

        let tile_idx = floor(u.cursor_type_id + 0.5);
        let in_bounds =
            sample_pos.x >= 0.0 && sample_pos.x < cursor_pixel_size &&
            sample_pos.y >= 0.0 && sample_pos.y < cursor_pixel_size;

        let shadow_strength = clamp(u.cursor_shadow, 0.0, 2.0);
        if shadow_strength > 0.001 {
            let base = pow(min(shadow_strength, 1.0), 0.8);
            let overdrive = max(0.0, shadow_strength - 1.0);
            let boosted = base + overdrive;
            let shadow_offset = vec2<f32>(
                (2.0 * (0.25 + 0.75 * base)) + (1.4 * overdrive),
                (2.8 * (0.25 + 0.75 * base)) + (2.2 * overdrive)
            );
            let shadow_pos = sample_pos - shadow_offset;
            let shadow_in_bounds =
                shadow_pos.x >= 0.0 && shadow_pos.x < cursor_pixel_size &&
                shadow_pos.y >= 0.0 && shadow_pos.y < cursor_pixel_size;

            if shadow_in_bounds {
                let blur = 1.0 + (3.5 * base) + (3.8 * overdrive);
                let diag = blur * 0.75;
                let offsets = array<vec2<f32>, 9>(
                    vec2<f32>(0.0, 0.0),
                    vec2<f32>(blur, 0.0),
                    vec2<f32>(-blur, 0.0),
                    vec2<f32>(0.0, blur),
                    vec2<f32>(0.0, -blur),
                    vec2<f32>(diag, diag),
                    vec2<f32>(-diag, diag),
                    vec2<f32>(diag, -diag),
                    vec2<f32>(-diag, -diag)
                );
                var shadow_alpha = 0.0;
                for (var i: i32 = 0; i < 9; i = i + 1) {
                    let p = shadow_pos + offsets[i];
                    if p.x >= 0.0 && p.x < cursor_pixel_size && p.y >= 0.0 && p.y < cursor_pixel_size {
                        let uv_in_tile = p / cursor_pixel_size;
                        let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
                        let atlas_row = floor(tile_idx / ATLAS_COLS);
                        let atlas_uv = vec2<f32>(
                            (uv_in_tile.x + atlas_col) / ATLAS_COLS,
                            (uv_in_tile.y + atlas_row) / ATLAS_ROWS
                        );
                        shadow_alpha = shadow_alpha + textureSample(cursor_tex, cursor_samp, atlas_uv).a;
                    }
                }
                shadow_alpha = (shadow_alpha / 9.0) * min(1.0, (0.95 * base) + (0.7 * overdrive)) * u.cursor_opacity;
                if shadow_alpha > 0.0001 {
                    let shadow_col = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
                    col = mix(col, shadow_col, shadow_col.a);
                }
            }
        }

        if in_bounds {
            let uv_in_tile = sample_pos / cursor_pixel_size;
            let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
            let atlas_row = floor(tile_idx / ATLAS_COLS);
            let atlas_uv = vec2<f32>(
                (uv_in_tile.x + atlas_col) / ATLAS_COLS,
                (uv_in_tile.y + atlas_row) / ATLAS_ROWS
            );
            let cur_col = textureSample(cursor_tex, cursor_samp, atlas_uv);
            let faded = vec4<f32>(cur_col.rgb, cur_col.a * u.cursor_opacity);
            col = mix(col, faded, faded.a);
        }
    }
    
    return col;
}
"#;

fn compositor_shader() -> String {
    format!(
        "const ATLAS_COLS: f32 = {}.0;\nconst ATLAS_ROWS: f32 = {}.0;\n{}",
        CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS, COMPOSITOR_SHADER_BODY
    )
}
