use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use resvg::{tiny_skia, usvg};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub const ATLAS_SIZES: &[u32] = &[
    11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 30, 33, 34, 35, 38, 40,
    41, 42, 44, 45, 47, 50, 53, 54, 59, 60, 67,
];
pub const ATLAS_COLUMNS: u32 = 9;
pub const ATLAS_GUTTER: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sprite {
    pub file: String,
    pub variants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub sprites: Vec<Sprite>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPage {
    pub pixels: u32,
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

pub fn generate(manifest_dir: &Path, out_dir: &Path) {
    let manifest = load_manifest(manifest_dir).unwrap_or_else(|error| panic!("{error}"));
    for &pixels in ATLAS_SIZES {
        let page =
            encode_page(manifest_dir, &manifest, pixels).unwrap_or_else(|error| panic!("{error}"));
        write_if_changed(&out_dir.join(format!("icon-atlas-{pixels}.png")), &page.png)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let generated = generate_rust(&manifest);
    write_if_changed(
        &out_dir.join("icon_atlas_generated.rs"),
        generated.as_bytes(),
    )
    .unwrap_or_else(|error| panic!("{error}"));

    println!("cargo:rerun-if-changed=src/gui/icons/manifest.json");
    println!("cargo:rerun-if-changed=src/gui/icons/svg");
    for sprite in &manifest.sprites {
        println!("cargo:rerun-if-changed=src/gui/icons/svg/{}", sprite.file);
    }
}

pub fn load_manifest(manifest_dir: &Path) -> Result<Manifest, String> {
    let manifest_path = manifest_dir.join("src/gui/icons/manifest.json");
    let text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let root = value
        .as_object()
        .ok_or_else(|| "icon manifest root must be an object".to_owned())?;
    if root.len() != 1 || !root.contains_key("sprites") {
        return Err("icon manifest root must contain only `sprites`".to_owned());
    }
    let entries = root["sprites"]
        .as_array()
        .ok_or_else(|| "icon manifest `sprites` must be an array".to_owned())?;
    let mut sprites = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let object = entry
            .as_object()
            .ok_or_else(|| format!("icon sprite {index} must be an object"))?;
        if object.len() != 2 || !object.contains_key("file") || !object.contains_key("variants") {
            return Err(format!(
                "icon sprite {index} must contain only `file` and `variants`"
            ));
        }
        let file = object["file"]
            .as_str()
            .ok_or_else(|| format!("icon sprite {index} has a non-string file"))?
            .to_owned();
        let variants = object["variants"]
            .as_array()
            .ok_or_else(|| format!("icon sprite {index} variants must be an array"))?
            .iter()
            .map(|variant| {
                variant
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("icon sprite {index} has a non-string variant"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        sprites.push(Sprite { file, variants });
    }
    validate_manifest(manifest_dir, &sprites)?;
    Ok(Manifest { sprites })
}

fn validate_manifest(manifest_dir: &Path, sprites: &[Sprite]) -> Result<(), String> {
    if sprites.is_empty() {
        return Err("icon manifest cannot be empty".to_owned());
    }
    let mut files = BTreeSet::new();
    let mut variants = BTreeSet::new();
    let mut previous = None;
    for sprite in sprites {
        if sprite.file.contains(['/', '\\']) || !sprite.file.ends_with(".svg") {
            return Err(format!(
                "icon source must be an SVG basename: {}",
                sprite.file
            ));
        }
        if previous.is_some_and(|name: &str| name >= sprite.file.as_str()) {
            return Err(
                "icon manifest sources must be unique and alphabetically sorted".to_owned(),
            );
        }
        previous = Some(&sprite.file);
        files.insert(sprite.file.clone());
        if sprite.variants.is_empty() {
            return Err(format!("icon source {} has no variants", sprite.file));
        }
        for variant in &sprite.variants {
            if !valid_rust_variant(variant) || !variants.insert(variant.clone()) {
                return Err(format!("invalid or duplicate icon variant: {variant}"));
            }
        }
    }

    let svg_dir = manifest_dir.join("src/gui/icons/svg");
    let disk_files = fs::read_dir(&svg_dir)
        .map_err(|error| format!("failed to read {}: {error}", svg_dir.display()))?
        .map(|entry| {
            entry
                .map_err(|error| error.to_string())?
                .file_name()
                .into_string()
                .map_err(|_| "icon source filename is not UTF-8".to_owned())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if files != disk_files {
        let missing = disk_files.difference(&files).cloned().collect::<Vec<_>>();
        let absent = files.difference(&disk_files).cloned().collect::<Vec<_>>();
        return Err(format!(
            "icon manifest/source mismatch; unmapped={missing:?}, absent={absent:?}"
        ));
    }
    Ok(())
}

fn valid_rust_variant(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

pub fn render_alpha(path: &Path, pixels: u32) -> Result<Vec<u8>, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default())
        .map_err(|error| format!("invalid icon SVG {}: {error}", path.display()))?;
    let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels)
        .ok_or_else(|| format!("could not allocate {pixels}px icon pixmap"))?;
    let size = tree.size();
    let scale = pixels as f32 / size.width().max(size.height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut alpha = Vec::with_capacity((pixels * pixels) as usize);
    for rgba in pixmap.data().chunks_exact(4) {
        if rgba[0] != rgba[3] || rgba[1] != rgba[3] || rgba[2] != rgba[3] {
            return Err(format!(
                "icon {} must render as a white alpha mask",
                path.display()
            ));
        }
        alpha.push(rgba[3]);
    }
    Ok(alpha)
}

pub fn encode_page(
    manifest_dir: &Path,
    manifest: &Manifest,
    pixels: u32,
) -> Result<EncodedPage, String> {
    let cell = pixels + ATLAS_GUTTER * 2;
    let rows = (manifest.sprites.len() as u32).div_ceil(ATLAS_COLUMNS);
    let width = ATLAS_COLUMNS * cell;
    let height = rows * cell;
    let mut atlas = vec![0_u8; (width * height) as usize];
    for (index, sprite) in manifest.sprites.iter().enumerate() {
        let path = manifest_dir.join("src/gui/icons/svg").join(&sprite.file);
        let alpha = render_alpha(&path, pixels)?;
        let x = (index as u32 % ATLAS_COLUMNS) * cell + ATLAS_GUTTER;
        let y = (index as u32 / ATLAS_COLUMNS) * cell + ATLAS_GUTTER;
        for row in 0..pixels {
            let source = (row * pixels) as usize;
            let destination = ((y + row) * width + x) as usize;
            atlas[destination..destination + pixels as usize]
                .copy_from_slice(&alpha[source..source + pixels as usize]);
        }
    }
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Best, FilterType::Adaptive)
        .write_image(&atlas, width, height, ExtendedColorType::L8)
        .map_err(|error| format!("failed to encode {pixels}px icon atlas: {error}"))?;
    Ok(EncodedPage {
        pixels,
        width,
        height,
        png,
    })
}

fn generate_rust(manifest: &Manifest) -> String {
    let mut output = String::from("// @generated by build_support/icon_atlas.rs\n");
    output.push_str(
        "impl Icon {\n    const fn sprite_index(self) -> usize {\n        match self {\n",
    );
    for (index, sprite) in manifest.sprites.iter().enumerate() {
        let pattern = sprite
            .variants
            .iter()
            .map(|variant| format!("Icon::{variant}"))
            .collect::<Vec<_>>()
            .join(" | ");
        writeln!(output, "            {pattern} => {index},").unwrap();
    }
    output.push_str("        }\n    }\n}\n");
    writeln!(
        output,
        "const ICON_SPRITE_COUNT: usize = {};\nconst ICON_ATLAS_COLUMNS: u32 = {ATLAS_COLUMNS};\nconst ICON_ATLAS_GUTTER: u32 = {ATLAS_GUTTER};",
        manifest.sprites.len()
    )
    .unwrap();
    output.push_str("const ICON_ATLAS_PAGES: &[AtlasPage] = &[\n");
    for pixels in ATLAS_SIZES {
        let cell = pixels + ATLAS_GUTTER * 2;
        let rows = (manifest.sprites.len() as u32).div_ceil(ATLAS_COLUMNS);
        writeln!(output, "    AtlasPage {{ pixels: {pixels}, width: {}, height: {}, png: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/icon-atlas-{pixels}.png\")) }},", ATLAS_COLUMNS * cell, rows * cell).unwrap();
    }
    output.push_str("];\n#[cfg(test)]\nconst ALL_ICONS: &[Icon] = &[\n");
    for sprite in &manifest.sprites {
        for variant in &sprite.variants {
            writeln!(output, "    Icon::{variant},").unwrap();
        }
    }
    output.push_str("];\n");
    output
}

fn write_if_changed(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    fs::write(path, bytes).map_err(|error| format!("failed to write {}: {error}", path.display()))
}
