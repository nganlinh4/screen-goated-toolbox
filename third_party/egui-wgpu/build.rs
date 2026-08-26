use naga::back::hlsl::{
    BindTarget, FragmentEntryPoint, Options, PipelineOptions, SamplerHeapBindTargets,
    SamplerIndexBufferKey, ShaderModel, Writer,
};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{ResourceBinding, ShaderStage};
use std::fs;
use std::path::{Path, PathBuf};

fn bind(space: u8, register: u32) -> BindTarget {
    BindTarget {
        space,
        register,
        ..Default::default()
    }
}

fn options(shader: ShaderKind) -> Options {
    let mut options = Options {
        shader_model: ShaderModel::V5_1,
        fake_missing_bindings: false,
        force_loop_bounding: true,
        restrict_indexing: true,
        sampler_heap_target: SamplerHeapBindTargets {
            standard_samplers: bind(0, 0),
            comparison_samplers: bind(0, 2048),
        },
        ..Default::default()
    };

    match shader {
        ShaderKind::Egui => {
            options
                .binding_map
                .insert(ResourceBinding { group: 0, binding: 0 }, bind(0, 0));
            options
                .binding_map
                .insert(ResourceBinding { group: 1, binding: 0 }, bind(0, 0));
            options.binding_map.insert(
                ResourceBinding { group: 1, binding: 1 },
                bind(255, 0),
            );
            options
                .sampler_buffer_binding_map
                .insert(SamplerIndexBufferKey { group: 1 }, bind(0, 1));
        }
        ShaderKind::Capture => {
            options
                .binding_map
                .insert(ResourceBinding { group: 0, binding: 0 }, bind(0, 0));
            // `vertex_index` is adjusted by WGPU's first-vertex root constants.
            options.special_constants_binding = Some(bind(0, 0));
        }
    }
    options
}

#[derive(Clone, Copy)]
enum ShaderKind {
    Egui,
    Capture,
}

fn generate(
    source_path: &Path,
    output_path: PathBuf,
    kind: ShaderKind,
    stage: ShaderStage,
    entry_point: &str,
    fragment_entry_point: Option<&str>,
) {
    let source = fs::read_to_string(source_path).expect("read WGSL source");
    let module = naga::front::wgsl::parse_str(&source).expect("parse WGSL source");
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .expect("validate WGSL source");
    let options = options(kind);
    let pipeline_options = PipelineOptions {
        entry_point: Some((stage, entry_point.to_owned())),
    };
    let fragment = fragment_entry_point.map(|name| {
        FragmentEntryPoint::new(&module, name).expect("find linked fragment entry point")
    });
    let mut hlsl = String::new();
    let reflection = Writer::new(&mut hlsl, &options, &pipeline_options)
        .write(&module, &info, fragment.as_ref())
        .expect("generate HLSL");
    let generated_name = reflection
        .entry_point_names
        .into_iter()
        .find_map(Result::ok)
        .expect("generated entry point");
    assert_eq!(generated_name, entry_point, "entry point was unexpectedly renamed");
    fs::write(output_path, hlsl).expect("write generated HLSL");
}

fn main() {
    println!("cargo:rerun-if-changed=src/egui.wgsl");
    println!("cargo:rerun-if-changed=src/texture_copy.wgsl");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));

    generate(
        Path::new("src/egui.wgsl"),
        out.join("egui_vs.hlsl"),
        ShaderKind::Egui,
        ShaderStage::Vertex,
        "vs_main",
        Some("fs_main_gamma_framebuffer"),
    );
    for entry in ["fs_main_gamma_framebuffer", "fs_main_linear_framebuffer"] {
        generate(
            Path::new("src/egui.wgsl"),
            out.join(format!("{entry}.hlsl")),
            ShaderKind::Egui,
            ShaderStage::Fragment,
            entry,
            None,
        );
    }
    generate(
        Path::new("src/texture_copy.wgsl"),
        out.join("capture_vs.hlsl"),
        ShaderKind::Capture,
        ShaderStage::Vertex,
        "vs_main",
        Some("fs_main"),
    );
    generate(
        Path::new("src/texture_copy.wgsl"),
        out.join("capture_fs.hlsl"),
        ShaderKind::Capture,
        ShaderStage::Fragment,
        "fs_main",
        None,
    );
}
