//! Bounded decoding of packed clipboard DIBs and registered PNG data.

use image::{ImageFormat, ImageReader, RgbaImage};
use std::io::Cursor;

mod rle;

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

pub(super) fn decode_preferred(
    png_format: u32,
    mut read: impl FnMut(u32, bool) -> Option<RgbaImage>,
) -> Option<RgbaImage> {
    [(png_format, true), (17, false), (8, false)]
        .into_iter()
        .filter(|(format, _)| *format != 0)
        .find_map(|(format, is_png)| read(format, is_png))
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(super) fn decode_png(bytes: &[u8]) -> Option<RgbaImage> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::image_decode::MAX_AXIS);
    limits.max_image_height = Some(crate::image_decode::MAX_AXIS);
    limits.max_alloc = Some(crate::image_decode::MAX_PIXELS * 4);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    crate::image_decode::validate_dimensions(image.width(), image.height()).ok()?;
    Some(image.to_rgba8())
}

#[derive(Clone, Copy)]
struct Channel {
    mask: u32,
    shift: u32,
    maximum: u32,
}

impl Channel {
    fn new(mask: u32, bit_count: u16) -> Option<Self> {
        if mask == 0 || (bit_count < 32 && (mask >> bit_count) != 0) {
            return None;
        }
        let shift = mask.trailing_zeros();
        let maximum = mask >> shift;
        // A channel mask must describe one contiguous run of bits.
        if maximum & maximum.wrapping_add(1) != 0 {
            return None;
        }
        Some(Self {
            mask,
            shift,
            maximum,
        })
    }

    fn extract(self, pixel: u32) -> u8 {
        let value = (pixel & self.mask) >> self.shift;
        ((u64::from(value) * 255 + u64::from(self.maximum) / 2) / u64::from(self.maximum)) as u8
    }
}

fn channels(
    bytes: &[u8],
    header_size: usize,
    bit_count: u16,
    compression: u32,
) -> Option<([Channel; 3], Option<Channel>, usize)> {
    let (masks, alpha, extra) = match compression {
        BI_RGB => match bit_count {
            16 => ([0x7c00, 0x03e0, 0x001f], 0, 0),
            24 | 32 => ([0x00ff0000, 0x0000ff00, 0x000000ff], 0, 0),
            _ => return None,
        },
        BI_BITFIELDS | BI_ALPHABITFIELDS if matches!(bit_count, 16 | 32) => {
            let count = if compression == BI_ALPHABITFIELDS {
                4
            } else {
                3
            };
            let extra = if header_size == 40 { count * 4 } else { 0 };
            if header_size != 40 && header_size < 52 {
                return None;
            }
            if compression == BI_ALPHABITFIELDS && header_size != 40 && header_size < 56 {
                return None;
            }
            let red = u32_at(bytes, 40)?;
            let green = u32_at(bytes, 44)?;
            let blue = u32_at(bytes, 48)?;
            let alpha = if header_size >= 56 || compression == BI_ALPHABITFIELDS {
                u32_at(bytes, 52)?
            } else {
                0
            };
            ([red, green, blue], alpha, extra)
        }
        _ => return None,
    };
    if masks[0] & masks[1] != 0
        || masks[0] & masks[2] != 0
        || masks[1] & masks[2] != 0
        || alpha & (masks[0] | masks[1] | masks[2]) != 0
    {
        return None;
    }
    Some((
        [
            Channel::new(masks[0], bit_count)?,
            Channel::new(masks[1], bit_count)?,
            Channel::new(masks[2], bit_count)?,
        ],
        if alpha == 0 {
            None
        } else {
            Some(Channel::new(alpha, bit_count)?)
        },
        extra,
    ))
}

/// Decode a packed DIB only after validating every header, mask, and row.
pub(super) fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.len() > crate::image_decode::MAX_ENCODED_BYTES {
        return None;
    }
    let header_size = usize::try_from(u32_at(bytes, 0)?).ok()?;
    if !matches!(header_size, 40 | 52 | 56 | 108 | 124) || bytes.len() < header_size {
        return None;
    }
    let width = i32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
    let signed_height = i32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?);
    let width = u32::try_from(width).ok()?;
    let height = signed_height.checked_abs()? as u32;
    crate::image_decode::validate_dimensions(width, height).ok()?;
    if u16_at(bytes, 12)? != 1 {
        return None;
    }
    let bits = u16_at(bytes, 14)?;
    let compression = u32_at(bytes, 16)?;
    match (compression, bits) {
        (BI_RGB, 1 | 4 | 8) => {
            return decode_indexed(bytes, header_size, width, signed_height, bits);
        }
        (BI_RLE8, 8) | (BI_RLE4, 4) => {
            return rle::decode(bytes, header_size, width, signed_height, bits);
        }
        _ => {}
    }
    let (rgb, alpha, external_masks) = channels(bytes, header_size, bits, compression)?;
    let palette_size = usize::try_from(u32_at(bytes, 32)?).ok()?.checked_mul(4)?;
    let offset = header_size
        .checked_add(external_masks)?
        .checked_add(palette_size)?;
    let pixel_bytes = usize::from(bits).checked_div(8)?;
    let row_size = (usize::try_from(width)
        .ok()?
        .checked_mul(pixel_bytes)?
        .checked_add(3)?)
        & !3;
    let required = row_size.checked_mul(usize::try_from(height).ok()?)?;
    let end = offset.checked_add(required)?;
    if profile_overlaps(bytes, header_size, offset..end) {
        return None;
    }
    let pixels = bytes.get(offset..end)?;
    let output_size = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    let mut output = Vec::new();
    output.try_reserve_exact(output_size).ok()?;
    for y in 0..height as usize {
        let source_y = if signed_height < 0 {
            y
        } else {
            height as usize - 1 - y
        };
        let row = &pixels[source_y * row_size..(source_y + 1) * row_size];
        for pixel in row.chunks_exact(pixel_bytes).take(width as usize) {
            let value = match pixel_bytes {
                2 => u32::from(u16::from_le_bytes([pixel[0], pixel[1]])),
                3 => u32::from_le_bytes([pixel[0], pixel[1], pixel[2], 0]),
                4 => u32::from_le_bytes(pixel.try_into().ok()?),
                _ => return None,
            };
            // BI_RGB's high byte is reserved, not an alpha channel. Explicit
            // bitfield masks and registered PNG data carry transparency.
            let a = alpha.map_or(255, |channel| channel.extract(value));
            output.extend_from_slice(&[
                rgb[0].extract(value),
                rgb[1].extract(value),
                rgb[2].extract(value),
                a,
            ]);
        }
    }
    RgbaImage::from_raw(width, height, output)
}

/// Pixels are taken as sRGB, as every mainstream paste target does; colour
/// profiles are not converted. A V5 profile placed inside the pixel range would
/// be read as pixels, so that format is rejected and the synthesized CF_DIB,
/// which carries no profile, is used instead.
fn profile_overlaps(bytes: &[u8], header_size: usize, pixels: std::ops::Range<usize>) -> bool {
    const PROFILE_LINKED: u32 = u32::from_be_bytes(*b"LINK");
    const PROFILE_EMBEDDED: u32 = u32::from_be_bytes(*b"MBED");
    if header_size != 124 || !matches!(u32_at(bytes, 56), Some(PROFILE_LINKED | PROFILE_EMBEDDED)) {
        return false;
    }
    let (Some(start), Some(size)) = (u32_at(bytes, 112), u32_at(bytes, 116)) else {
        return true;
    };
    let start = start as usize;
    let end = start.saturating_add(size as usize);
    start < pixels.end && pixels.start < end
}

/// Read the RGBQUAD color table of a 1, 4 or 8 bit DIB, bounded by its bit
/// depth, and return it with the offset of the pixel data that follows it.
fn color_table(bytes: &[u8], header_size: usize, bits: u16) -> Option<(Vec<[u8; 4]>, usize)> {
    let capacity = 1usize << bits;
    let colors = match usize::try_from(u32_at(bytes, 32)?).ok()? {
        0 => capacity,
        used if used <= capacity => used,
        _ => return None,
    };
    let table = bytes.get(header_size..header_size.checked_add(colors.checked_mul(4)?)?)?;
    let palette = table
        .as_chunks::<4>()
        .0
        .iter()
        .map(|[blue, green, red, _]| [*red, *green, *blue, 255])
        .collect();
    Some((palette, header_size + table.len()))
}

/// Decode an uncompressed 1, 4 or 8 bit DIB through its color table. An index
/// outside the table rejects the whole image instead of guessing a color.
fn decode_indexed(
    bytes: &[u8],
    header_size: usize,
    width: u32,
    signed_height: i32,
    bits: u16,
) -> Option<RgbaImage> {
    let height = signed_height.checked_abs()? as u32;
    let (palette, offset) = color_table(bytes, header_size, bits)?;
    let width = usize::try_from(width).ok()?;
    let rows = usize::try_from(height).ok()?;
    let row_size = width
        .checked_mul(usize::from(bits))?
        .checked_add(31)?
        .checked_div(32)?
        .checked_mul(4)?;
    let end = offset.checked_add(row_size.checked_mul(rows)?)?;
    if profile_overlaps(bytes, header_size, offset..end) {
        return None;
    }
    let pixels = bytes.get(offset..end)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(width.checked_mul(rows)?.checked_mul(4)?)
        .ok()?;
    let per_byte = 8 / usize::from(bits);
    let mask = ((1u16 << bits) - 1) as u8;
    for y in 0..rows {
        let source_y = if signed_height < 0 { y } else { rows - 1 - y };
        let row = &pixels[source_y * row_size..(source_y + 1) * row_size];
        for x in 0..width {
            let byte = row[x / per_byte];
            let shift = 8 - usize::from(bits) * (x % per_byte + 1);
            let index = usize::from((byte >> shift) & mask);
            output.extend_from_slice(palette.get(index)?);
        }
    }
    RgbaImage::from_raw(width as u32, height, output)
}

#[cfg(test)]
mod tests;
