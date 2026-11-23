use super::*;

fn header(width: i32, height: i32, bits: u16, compression: u32, size: u32) -> Vec<u8> {
    let mut result = vec![0; size as usize];
    result[0..4].copy_from_slice(&size.to_le_bytes());
    result[4..8].copy_from_slice(&width.to_le_bytes());
    result[8..12].copy_from_slice(&height.to_le_bytes());
    result[12..14].copy_from_slice(&1u16.to_le_bytes());
    result[14..16].copy_from_slice(&bits.to_le_bytes());
    result[16..20].copy_from_slice(&compression.to_le_bytes());
    result
}

#[test]
fn rgb_rows_are_padded_and_orientation_is_preserved() {
    // Two rows, 24-bit, width 1; each row has one padding byte.
    let mut dib = header(1, 2, 24, BI_RGB, 40);
    dib.extend_from_slice(&[30, 20, 10, 0, 60, 50, 40, 0]);
    let image = decode(&dib).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [40, 50, 60, 255]);
    assert_eq!(image.get_pixel(0, 1).0, [10, 20, 30, 255]);
    dib[8..12].copy_from_slice(&(-2i32).to_le_bytes());
    let image = decode(&dib).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [10, 20, 30, 255]);
    assert_eq!(image.get_pixel(0, 1).0, [40, 50, 60, 255]);
}

#[test]
fn external_bitfields_are_not_mistaken_for_pixels() {
    let mut dib = header(1, 1, 32, BI_BITFIELDS, 40);
    // Nonstandard component ordering demonstrates actual mask use.
    for mask in [0x000000ffu32, 0x0000ff00, 0x00ff0000] {
        dib.extend_from_slice(&mask.to_le_bytes());
    }
    dib.extend_from_slice(&[9, 20, 31, 0]);
    assert_eq!(decode(&dib).unwrap().get_pixel(0, 0).0, [9, 20, 31, 255]);
}

#[test]
fn bitfield_components_are_scaled_and_16_bit_stride_is_aligned() {
    let mut dib = header(1, 1, 16, BI_BITFIELDS, 40);
    for mask in [0xf800u32, 0x07e0, 0x001f] {
        dib.extend_from_slice(&mask.to_le_bytes());
    }
    dib.extend_from_slice(&[0x10, 0xfc, 0, 0]); // red=31, green=32, blue=16
    assert_eq!(
        decode(&dib).unwrap().get_pixel(0, 0).0,
        [255, 130, 132, 255]
    );
}

#[test]
fn reserved_alpha_and_explicit_transparent_alpha_are_distinct() {
    let mut rgb = header(2, 1, 32, BI_RGB, 40);
    rgb.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
    assert_eq!(decode(&rgb).unwrap().get_pixel(0, 0).0, [3, 2, 1, 255]);
    rgb[47] = 128;
    let image = decode(&rgb).unwrap();
    // Even a mixed or nonzero reserved byte does not make BI_RGB transparent.
    assert_eq!(image.get_pixel(0, 0).0, [3, 2, 1, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [6, 5, 4, 255]);

    let mut v5 = header(1, 1, 32, BI_BITFIELDS, 124);
    for (offset, mask) in [
        (40, 0x00ff0000u32),
        (44, 0x0000ff00),
        (48, 0x000000ff),
        (52, 0xff000000),
    ] {
        v5[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
    }
    v5.extend_from_slice(&[1, 2, 3, 0]);
    assert_eq!(decode(&v5).unwrap().get_pixel(0, 0).0, [3, 2, 1, 0]);

    let mut external = header(1, 1, 32, BI_ALPHABITFIELDS, 40);
    for mask in [0x00ff0000u32, 0x0000ff00, 0x000000ff, 0xff000000] {
        external.extend_from_slice(&mask.to_le_bytes());
    }
    external.extend_from_slice(&[1, 2, 3, 0]);
    assert_eq!(decode(&external).unwrap().get_pixel(0, 0).0, [3, 2, 1, 0]);
}

#[test]
fn declared_color_table_is_skipped_for_truecolor_images() {
    let mut dib = header(1, 1, 24, BI_RGB, 40);
    dib[32..36].copy_from_slice(&1u32.to_le_bytes());
    dib.extend_from_slice(&[100, 101, 102, 103]);
    dib.extend_from_slice(&[1, 2, 3, 0]);
    assert_eq!(decode(&dib).unwrap().get_pixel(0, 0).0, [3, 2, 1, 255]);
}

#[test]
fn incomplete_or_invalid_dibs_are_rejected_before_allocation() {
    let mut dib = header(1, 1, 32, BI_BITFIELDS, 40);
    for mask in [0xff0000u32, 0xff00, 0xff] {
        dib.extend_from_slice(&mask.to_le_bytes());
    }
    dib.extend_from_slice(&[1, 2, 3, 4]);
    for end in 0..dib.len() {
        assert!(
            decode(&dib[..end]).is_none(),
            "accepted truncation at {end}"
        );
    }
    let mut overlap = dib.clone();
    overlap[44..48].copy_from_slice(&0xff0000u32.to_le_bytes());
    assert!(decode(&overlap).is_none());
    let mut discontinuous = dib.clone();
    discontinuous[40..44].copy_from_slice(&0x00a50000u32.to_le_bytes());
    assert!(decode(&discontinuous).is_none());
    let mut extreme = header(i32::MAX, i32::MIN, 32, BI_RGB, 40);
    extreme.extend_from_slice(&[0; 4]);
    assert!(decode(&extreme).is_none());
    let mut unknown = header(1, 1, 32, 4, 40);
    unknown.extend_from_slice(&[0; 4]);
    assert!(decode(&unknown).is_none());
    let mut palette = header(1, 1, 24, BI_RGB, 40);
    palette[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    palette.extend_from_slice(&[0; 4]);
    assert!(decode(&palette).is_none());
}

#[test]
fn registered_png_is_decoded_with_transparency_and_bounded_dimensions() {
    let source = RgbaImage::from_pixel(1, 1, image::Rgba([9, 8, 7, 3]));
    let mut png = Cursor::new(Vec::new());
    source.write_to(&mut png, ImageFormat::Png).unwrap();
    assert_eq!(decode_png(png.get_ref()).unwrap(), source);
    assert!(decode_png(&png.get_ref()[..12]).is_none());
}

#[test]
fn invalid_preferred_formats_fall_back_without_losing_the_image() {
    let mut dib = header(1, 1, 24, BI_RGB, 40);
    dib.extend_from_slice(&[4, 5, 6, 0]);
    let mut seen = Vec::new();
    let image = decode_preferred(0xc123, |format, is_png| {
        seen.push(format);
        match (format, is_png) {
            (0xc123, true) => decode_png(b"invalid PNG"),
            (17, false) => decode(b"invalid DIBV5"),
            (8, false) => decode(&dib),
            _ => None,
        }
    })
    .unwrap();
    assert_eq!(seen, [0xc123, 17, 8]);
    assert_eq!(image.get_pixel(0, 0).0, [6, 5, 4, 255]);
}

#[test]
fn paletted_dibs_map_packed_indices_through_their_color_table() {
    // 8-bit, two colors used, width 2: the row holds two indices plus padding.
    let mut eight = header(2, 1, 8, BI_RGB, 40);
    eight[32..36].copy_from_slice(&2u32.to_le_bytes());
    eight.extend_from_slice(&[30, 20, 10, 0, 3, 2, 1, 0]);
    eight.extend_from_slice(&[1, 0, 0, 0]);
    let image = decode(&eight).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [10, 20, 30, 255]);

    // 1-bit with an implicit two-entry table, most significant bit first,
    // stored bottom-up.
    let mut one = header(3, 2, 1, BI_RGB, 40);
    one.extend_from_slice(&[0, 0, 0, 0, 255, 255, 255, 0]);
    one.extend_from_slice(&[0b1010_0000, 0, 0, 0, 0b0100_0000, 0, 0, 0]);
    let image = decode(&one).unwrap();
    let white = [255, 255, 255, 255];
    let black = [0, 0, 0, 255];
    assert_eq!(image.get_pixel(0, 0).0, black);
    assert_eq!(image.get_pixel(1, 0).0, white);
    assert_eq!(image.get_pixel(0, 1).0, white);
    assert_eq!(image.get_pixel(1, 1).0, black);
    assert_eq!(image.get_pixel(2, 1).0, white);

    // 4-bit nibbles, high nibble first.
    let mut four = header(2, 1, 4, BI_RGB, 40);
    four[32..36].copy_from_slice(&2u32.to_le_bytes());
    four.extend_from_slice(&[0, 0, 9, 0, 0, 7, 0, 0]);
    four.extend_from_slice(&[0x10, 0, 0, 0]);
    let image = decode(&four).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [0, 7, 0, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [9, 0, 0, 255]);
}

#[test]
fn paletted_dibs_reject_out_of_table_indices_and_oversized_tables() {
    let mut dib = header(1, 1, 8, BI_RGB, 40);
    dib[32..36].copy_from_slice(&1u32.to_le_bytes());
    dib.extend_from_slice(&[1, 2, 3, 0]);
    dib.extend_from_slice(&[1, 0, 0, 0]);
    assert!(decode(&dib).is_none());
    dib[44] = 0;
    assert_eq!(decode(&dib).unwrap().get_pixel(0, 0).0, [3, 2, 1, 255]);
    dib[32..36].copy_from_slice(&257u32.to_le_bytes());
    assert!(decode(&dib).is_none());
    // A row truncated by the clipboard block is rejected, not read past.
    let mut short = header(64, 1, 1, BI_RGB, 40);
    short.extend_from_slice(&[0; 8]);
    short.extend_from_slice(&[0; 4]);
    assert!(decode(&short).is_none());
}

fn rle(width: i32, height: i32, bits: u16, compression: u32, stream: &[u8]) -> Vec<u8> {
    let mut dib = header(width, height, bits, compression, 40);
    dib[32..36].copy_from_slice(&3u32.to_le_bytes());
    // Table: 0 red, 1 green, 2 blue (stored as BGRx).
    dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0]);
    dib.extend_from_slice(stream);
    dib
}

#[test]
fn run_length_dibs_decode_runs_absolute_blocks_and_deltas_bottom_up() {
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    let clear = [0, 0, 0, 0];
    // RLE8, 4x2: bottom row = one red run then a padded absolute block
    // (green, green, blue); next row = delta skips two pixels, one blue run.
    let eight = rle(
        4,
        2,
        8,
        BI_RLE8,
        &[1, 0, 0, 3, 1, 1, 2, 0, 0, 0, 0, 2, 2, 0, 1, 2, 0, 1],
    );
    let image = decode(&eight).unwrap();
    assert_eq!(image.get_pixel(0, 1).0, red);
    assert_eq!(image.get_pixel(1, 1).0, green);
    assert_eq!(image.get_pixel(2, 1).0, green);
    assert_eq!(image.get_pixel(3, 1).0, blue);
    assert_eq!(image.get_pixel(0, 0).0, clear);
    assert_eq!(image.get_pixel(1, 0).0, clear);
    assert_eq!(image.get_pixel(2, 0).0, blue);
    assert_eq!(image.get_pixel(3, 0).0, clear);

    // RLE4, 5x1: a run alternates its two nibbles, then a padded absolute
    // block of three nibbles.
    let four = rle(5, 1, 4, BI_RLE4, &[2, 0x01, 0, 3, 0x21, 0x00, 0, 1]);
    let image = decode(&four).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, red);
    assert_eq!(image.get_pixel(1, 0).0, green);
    assert_eq!(image.get_pixel(2, 0).0, blue);
    assert_eq!(image.get_pixel(3, 0).0, green);
    assert_eq!(image.get_pixel(4, 0).0, red);
}

#[test]
fn run_length_dibs_reject_overruns_unknown_colors_and_top_down_layout() {
    // A run longer than the row.
    assert!(decode(&rle(2, 1, 8, BI_RLE8, &[3, 0, 0, 1])).is_none());
    // A color index outside the three-entry table.
    assert!(decode(&rle(2, 1, 8, BI_RLE8, &[2, 7, 0, 1])).is_none());
    // A delta that leaves the bitmap.
    assert!(decode(&rle(2, 1, 8, BI_RLE8, &[0, 2, 0, 5, 1, 0, 0, 1])).is_none());
    // A truncated absolute block.
    assert!(decode(&rle(4, 1, 8, BI_RLE8, &[0, 4, 1, 1])).is_none());
    // Compressed DIBs cannot be top-down.
    assert!(decode(&rle(2, -1, 8, BI_RLE8, &[2, 0, 0, 1])).is_none());
    // RLE8 compression on a 4-bit header is not a valid combination.
    assert!(decode(&rle(2, 1, 4, BI_RLE8, &[2, 0, 0, 1])).is_none());
}

#[test]
fn v5_profiles_are_skipped_after_pixels_and_rejected_inside_them() {
    let mut dib = header(1, 1, 32, BI_RGB, 124);
    dib[56..60].copy_from_slice(&u32::from_be_bytes(*b"MBED").to_le_bytes());
    // Profile after the single pixel: the pixel decodes unchanged.
    dib[112..116].copy_from_slice(&128u32.to_le_bytes());
    dib[116..120].copy_from_slice(&4u32.to_le_bytes());
    dib.extend_from_slice(&[3, 2, 1, 0, 9, 9, 9, 9]);
    assert_eq!(decode(&dib).unwrap().get_pixel(0, 0).0, [1, 2, 3, 255]);
    // Profile claimed where the pixels are: never read as pixels.
    dib[112..116].copy_from_slice(&124u32.to_le_bytes());
    assert!(decode(&dib).is_none());
}
