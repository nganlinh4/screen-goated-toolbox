#[path = "../build_support/icon_atlas.rs"]
mod icon_atlas;

use image::{GrayImage, ImageReader, imageops};
use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

const WINDOWS_DPI_TARGETS: &[u32] = &[
    11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 30, 33, 34, 35, 38, 40,
    41, 42, 44, 45, 47, 50, 53, 54, 59, 60, 67,
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn nearest_size(target: u32) -> u32 {
    *icon_atlas::ATLAS_SIZES
        .iter()
        .min_by(|left, right| {
            left.abs_diff(target)
                .cmp(&right.abs_diff(target))
                .then_with(|| right.cmp(left))
        })
        .unwrap()
}

fn source_path(root: &Path, file: &str) -> PathBuf {
    root.join("src/gui/icons/svg").join(file)
}

#[test]
fn manifest_is_exhaustive_and_atlases_are_deterministic() {
    let root = root();
    let manifest = icon_atlas::load_manifest(&root).unwrap();
    assert_eq!(manifest.sprites.len(), 67);
    assert_eq!(
        manifest
            .sprites
            .iter()
            .map(|sprite| sprite.variants.len())
            .sum::<usize>(),
        70
    );

    let output = root
        .join("target/icon-atlas-generation-test")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&output).unwrap();
    icon_atlas::generate(&root, &output);
    let generated_rust = std::fs::read(output.join("icon_atlas_generated.rs")).unwrap();
    let mut generated_pages = Vec::new();
    let mut total_png_bytes = 0;
    for &pixels in icon_atlas::ATLAS_SIZES {
        let bytes = std::fs::read(output.join(format!("icon-atlas-{pixels}.png"))).unwrap();
        total_png_bytes += bytes.len();
        generated_pages.push((pixels, bytes));
    }
    assert!(
        total_png_bytes < 400_000,
        "atlas delivery budget regressed to {total_png_bytes} bytes"
    );
    icon_atlas::generate(&root, &output);
    assert_eq!(
        generated_rust,
        std::fs::read(output.join("icon_atlas_generated.rs")).unwrap()
    );
    for (pixels, expected) in generated_pages {
        assert_eq!(
            expected,
            std::fs::read(output.join(format!("icon-atlas-{pixels}.png"))).unwrap()
        );
    }
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn encoded_cells_match_source_rasters_and_keep_clear_gutters() {
    let root = root();
    let manifest = icon_atlas::load_manifest(&root).unwrap();
    for pixels in [11, 33, 67] {
        let page = icon_atlas::encode_page(&root, &manifest, pixels).unwrap();
        let decoded = ImageReader::with_format(Cursor::new(page.png), image::ImageFormat::Png)
            .decode()
            .unwrap()
            .into_luma8();
        assert_eq!(decoded.dimensions(), (page.width, page.height));
        let cell = pixels + icon_atlas::ATLAS_GUTTER * 2;
        for (index, sprite) in manifest.sprites.iter().enumerate() {
            let x = (index as u32 % icon_atlas::ATLAS_COLUMNS) * cell + icon_atlas::ATLAS_GUTTER;
            let y = (index as u32 / icon_atlas::ATLAS_COLUMNS) * cell + icon_atlas::ATLAS_GUTTER;
            let expected =
                icon_atlas::render_alpha(&source_path(&root, &sprite.file), pixels).unwrap();
            let actual = imageops::crop_imm(&decoded, x, y, pixels, pixels)
                .to_image()
                .into_raw();
            assert_eq!(actual, expected, "{} at {pixels}px", sprite.file);
            assert!((x..x + pixels).all(|column| {
                decoded.get_pixel(column, y - 1)[0] == 0
                    && decoded.get_pixel(column, y + pixels)[0] == 0
            }));
            assert!((y..y + pixels).all(|row| {
                decoded.get_pixel(x - 1, row)[0] == 0 && decoded.get_pixel(x + pixels, row)[0] == 0
            }));
        }
    }
}

#[test]
fn dpi_ladder_preserves_alpha_shape_and_edge_contrast() {
    let root = root();
    let manifest = icon_atlas::load_manifest(&root).unwrap();
    let mut worst_mean_error = 0.0_f64;
    let mut worst_edge_ratio_error = 0.0_f64;
    let mut worst_mean_case = String::new();
    let mut worst_edge_case = String::new();
    for &target in WINDOWS_DPI_TARGETS {
        let source_size = nearest_size(target);
        assert_eq!(source_size, target);
        for sprite in &manifest.sprites {
            let path = source_path(&root, &sprite.file);
            let exact = icon_atlas::render_alpha(&path, target).unwrap();
            let selected = GrayImage::from_raw(
                source_size,
                source_size,
                icon_atlas::render_alpha(&path, source_size).unwrap(),
            )
            .unwrap();
            let scaled =
                imageops::resize(&selected, target, target, imageops::FilterType::Triangle)
                    .into_raw();
            let mean_error = exact
                .iter()
                .zip(&scaled)
                .map(|(left, right)| (f64::from(*left) - f64::from(*right)).abs())
                .sum::<f64>()
                / exact.len() as f64
                / 255.0;
            if mean_error > worst_mean_error {
                worst_mean_error = mean_error;
                worst_mean_case = format!("{} {source_size}px->{target}px", sprite.file);
            }
            let exact_edges = edge_energy(&exact, target);
            let scaled_edges = edge_energy(&scaled, target);
            if exact_edges > 0.0 {
                let edge_ratio_error = (scaled_edges / exact_edges - 1.0).abs();
                if edge_ratio_error > worst_edge_ratio_error {
                    worst_edge_ratio_error = edge_ratio_error;
                    worst_edge_case = format!("{} {source_size}px->{target}px", sprite.file);
                }
            }
        }
    }
    assert!(
        worst_mean_error == 0.0,
        "mean alpha error regressed to {worst_mean_error:.4} at {worst_mean_case}"
    );
    assert!(
        worst_edge_ratio_error == 0.0,
        "edge contrast regressed by {worst_edge_ratio_error:.4} at {worst_edge_case}"
    );
}

fn edge_energy(alpha: &[u8], side: u32) -> f64 {
    let mut energy = 0_u64;
    for y in 0..side {
        for x in 0..side {
            let index = (y * side + x) as usize;
            if x + 1 < side {
                energy += u64::from(alpha[index].abs_diff(alpha[index + 1]));
            }
            if y + 1 < side {
                energy += u64::from(alpha[index].abs_diff(alpha[index + side as usize]));
            }
        }
    }
    energy as f64
}

#[test]
fn manifest_variants_are_unique() {
    let manifest = icon_atlas::load_manifest(&root()).unwrap();
    let variants = manifest
        .sprites
        .iter()
        .flat_map(|sprite| sprite.variants.iter())
        .collect::<BTreeSet<_>>();
    assert_eq!(variants.len(), 70);
}
