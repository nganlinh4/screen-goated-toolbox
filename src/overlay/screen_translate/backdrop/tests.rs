use super::*;

#[test]
fn histogram_preserves_the_sorted_color_distance_threshold() {
    for width in [1, 2, 3, 17, 256, 1025] {
        let source = image::RgbaImage::from_fn(width, 3, |x, y| {
            image::Rgba([(x * 37 + y * 19) as u8, (x * 13) as u8, y as u8, 255])
        });
        let background = image::RgbaImage::from_pixel(width, 3, image::Rgba([80, 120, 50, 255]));
        let mut distances = source
            .pixels()
            .zip(background.pixels())
            .map(|(a, b)| color_distance(*a, *b))
            .collect::<Vec<_>>();
        distances.sort_unstable();
        assert_eq!(
            difference_threshold(&source, &background),
            distances[distances.len() * 2 / 3].max(16)
        );
    }
}

#[test]
fn shaped_background_leaves_foreign_pixel_ownership_transparent() {
    let source = image::RgbaImage::from_pixel(32, 24, image::Rgba([80, 90, 100, 255]));
    let target = PixelRegion {
        x: 8,
        y: 4,
        width: 16,
        height: 12,
    };
    let shape = [
        PixelRegion {
            height: 4,
            ..target
        },
        PixelRegion {
            y: 12,
            height: 4,
            ..target
        },
        PixelRegion {
            y: 8,
            width: 4,
            height: 4,
            ..target
        },
        PixelRegion {
            x: 20,
            y: 8,
            width: 4,
            height: 4,
        },
    ];
    let painted = reconstruct_shaped_blob(
        &source,
        target,
        &[target],
        &shape,
        Some(([80, 90, 100], 100)),
    );
    for (x, y, pixel) in painted.enumerate_pixels() {
        let protected = (4..12).contains(&x) && (4..8).contains(&y);
        assert_eq!(pixel[3], if protected { 0 } else { 255 });
    }
}

#[test]
fn reconstructed_blob_uses_surrounding_pixels_and_matches_the_detector_region() {
    let background = image::Rgba([72, 96, 120, 255]);
    let glyph = image::Rgba([244, 232, 210, 255]);
    let mut source = image::RgbaImage::from_pixel(80, 40, background);
    for y in 12..28 {
        for x in [28, 29, 30, 43, 44, 45] {
            source.put_pixel(x, y, glyph);
        }
    }
    let target = PixelRegion {
        x: 20,
        y: 10,
        width: 40,
        height: 20,
    };
    let painted = reconstruct_blob_image_with_background(&source, target, &[target], None);
    let original =
        image::imageops::crop_imm(&source, target.x, target.y, target.width, target.height)
            .to_image();
    let color = foreground_color(&original, &painted);
    let url = encode_data_url(&painted).unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
    assert_eq!(color, "#F4E8D2");
    let png = base64::engine::general_purpose::STANDARD
        .decode(url.trim_start_matches("data:image/png;base64,"))
        .unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (40, 20));
    assert!(decoded.pixels().all(|pixel| pixel[3] == 255));
    assert!(decoded.pixels().all(|pixel| {
        pixel[0].abs_diff(72) <= 2 && pixel[1].abs_diff(96) <= 2 && pixel[2].abs_diff(120) <= 2
    }));
}

#[test]
fn trusted_uniform_surface_does_not_create_directional_bands() {
    let mut source = image::RgbaImage::from_pixel(100, 40, image::Rgba([250, 250, 250, 255]));
    for y in 8..32 {
        for x in 10..90 {
            source.put_pixel(x, y, image::Rgba([5, 5, 5, 255]));
        }
    }
    let target = PixelRegion {
        x: 10,
        y: 8,
        width: 80,
        height: 24,
    };
    let painted = reconstruct_blob_image_with_background(
        &source,
        target,
        &[target],
        Some(([250, 250, 250], 90)),
    );
    assert!(
        painted
            .pixels()
            .all(|pixel| pixel.0 == [250, 250, 250, 255])
    );
}

#[test]
fn foreground_sampling_preserves_each_regions_glyph_color() {
    let cases = [
        ([221, 181, 38, 255], [22, 45, 72, 255]),
        ([31, 34, 36, 255], [173, 178, 184, 255]),
        ([240, 240, 240, 255], [194, 48, 116, 255]),
    ];
    for (background, glyph) in cases {
        let backdrop = image::RgbaImage::from_pixel(30, 20, image::Rgba(background));
        let mut source = backdrop.clone();
        for y in 3..17 {
            for x in 8..13 {
                source.put_pixel(x, y, image::Rgba(glyph));
            }
        }
        assert_eq!(
            foreground_color(&source, &backdrop),
            format!("#{:02X}{:02X}{:02X}", glyph[0], glyph[1], glyph[2])
        );
    }
}

#[test]
fn low_contrast_sample_falls_back_to_a_readable_neutral() {
    let backdrop = image::RgbaImage::from_pixel(40, 24, image::Rgba([112, 6, 20, 255]));
    let mut source = backdrop.clone();
    for y in 4..20 {
        for x in 16..24 {
            source.put_pixel(x, y, image::Rgba([145, 17, 43, 255]));
        }
    }
    assert_eq!(foreground_color(&source, &backdrop), "#FFFFFF");
}

#[test]
fn background_inpainting_has_no_horizontal_or_vertical_preference() {
    let image = image::RgbaImage::from_fn(44, 30, |x, y| {
        image::Rgba([
            (20 + x * 3) as u8,
            (30 + y * 5) as u8,
            (40 + x + y * 2) as u8,
            255,
        ])
    });
    let region = PixelRegion {
        x: 9,
        y: 7,
        width: 24,
        height: 14,
    };
    let sample = PixelRegion {
        x: 0,
        y: 0,
        width: image.width(),
        height: image.height(),
    };
    let filled = inpaint_regions(&image, sample, &[region], None);
    let transposed =
        image::RgbaImage::from_fn(image.height(), image.width(), |x, y| *image.get_pixel(y, x));
    let transposed_region = PixelRegion {
        x: region.y,
        y: region.x,
        width: region.height,
        height: region.width,
    };
    let transposed_sample = PixelRegion {
        x: 0,
        y: 0,
        width: transposed.width(),
        height: transposed.height(),
    };
    let transposed_filled =
        inpaint_regions(&transposed, transposed_sample, &[transposed_region], None);
    for y in 0..image.height() {
        for x in 0..image.width() {
            assert_eq!(filled.get_pixel(x, y), transposed_filled.get_pixel(y, x));
        }
    }
}

#[test]
fn background_inpainting_reconstructs_a_smooth_plane_without_diagonal_seams() {
    let image = image::RgbaImage::from_fn(64, 40, |x, y| {
        image::Rgba([
            (20 + x * 2 + y) as u8,
            (30 + x + y * 2) as u8,
            (40 + x + y) as u8,
            255,
        ])
    });
    let region = PixelRegion {
        x: 12,
        y: 9,
        width: 38,
        height: 22,
    };
    let sample = PixelRegion {
        x: 0,
        y: 0,
        width: image.width(),
        height: image.height(),
    };
    let filled = inpaint_regions(&image, sample, &[region], None);
    for y in region.y..region.y + region.height {
        for x in region.x..region.x + region.width {
            let expected = image.get_pixel(x, y);
            let actual = filled.get_pixel(x, y);
            assert!(
                expected
                    .0
                    .iter()
                    .zip(actual.0)
                    .all(|(expected, actual)| expected.abs_diff(actual) <= 1),
                "pixel ({x},{y}) expected={expected:?} actual={actual:?}"
            );
        }
    }
}

#[test]
fn background_inpainting_preserves_a_boundary_between_backgrounds() {
    let mut image = image::RgbaImage::from_fn(60, 30, |x, _| {
        if x < 30 {
            image::Rgba([32, 64, 96, 255])
        } else {
            image::Rgba([224, 192, 160, 255])
        }
    });
    let region = PixelRegion {
        x: 12,
        y: 8,
        width: 36,
        height: 14,
    };
    for y in region.y + 2..region.y + region.height - 2 {
        for x in [18, 19, 20] {
            image.put_pixel(x, y, image::Rgba([238, 238, 238, 255]));
        }
        for x in [39, 40, 41] {
            image.put_pixel(x, y, image::Rgba([12, 12, 12, 255]));
        }
    }
    let sample = PixelRegion {
        x: 0,
        y: 0,
        width: image.width(),
        height: image.height(),
    };
    let filled = inpaint_regions(&image, sample, &[region], None);
    let left = filled.get_pixel(19, 15);
    let right = filled.get_pixel(40, 15);
    assert!(left[0] < 100, "left={left:?}");
    assert!(right[0] > 156, "right={right:?}");
}
