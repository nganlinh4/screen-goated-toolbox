//! Run-length encoded (BI_RLE8 / BI_RLE4) clipboard DIBs.
//!
//! Every run, delta and absolute block is checked against the declared
//! bitmap and color table; a write outside either rejects the image instead of
//! guessing. Pixels the stream never paints (delta jumps, early end of line or
//! bitmap) stay transparent, as browsers render them.

use image::RgbaImage;

const END_OF_LINE: u8 = 0;
const END_OF_BITMAP: u8 = 1;
const DELTA: u8 = 2;

struct Canvas<'a> {
    palette: &'a [[u8; 4]],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    pixels: Vec<u8>,
}

impl Canvas<'_> {
    fn paint(&mut self, index: u8) -> Option<()> {
        if self.x >= self.width || self.y >= self.height {
            return None;
        }
        let color = self.palette.get(usize::from(index))?;
        // RLE bitmaps are bottom-up: stream row 0 is the last output row.
        let start = ((self.height - 1 - self.y) * self.width + self.x) * 4;
        self.pixels[start..start + 4].copy_from_slice(color);
        self.x += 1;
        Some(())
    }
}

/// Nibble `position` of a 4-bit run or absolute block, high nibble first.
fn nibble(byte: u8, position: usize) -> u8 {
    if position.is_multiple_of(2) {
        byte >> 4
    } else {
        byte & 0x0f
    }
}

pub(super) fn decode(
    bytes: &[u8],
    header_size: usize,
    width: u32,
    signed_height: i32,
    bits: u16,
) -> Option<RgbaImage> {
    // Compressed DIBs are bottom-up by definition.
    if signed_height <= 0 {
        return None;
    }
    let (palette, offset) = super::color_table(bytes, header_size, bits)?;
    let declared = usize::try_from(super::u32_at(bytes, 20)?).ok()?;
    let available = bytes.len().checked_sub(offset)?;
    let length = if declared == 0 { available } else { declared };
    let end = offset.checked_add(length)?;
    if super::profile_overlaps(bytes, header_size, offset..end) {
        return None;
    }
    let stream = bytes.get(offset..end)?;
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(signed_height).ok()?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(width.checked_mul(height)?.checked_mul(4)?)
        .ok()?;
    // Zeroed RGBA is transparent, the color of pixels the stream skips.
    pixels.resize(width * height * 4, 0);
    let mut canvas = Canvas {
        palette: &palette,
        width,
        height,
        x: 0,
        y: 0,
        pixels,
    };
    let mut cursor = 0;
    while let Some(&[count, value]) = stream.get(cursor..cursor + 2) {
        cursor += 2;
        if count > 0 {
            for position in 0..usize::from(count) {
                let index = if bits == 4 {
                    nibble(value, position)
                } else {
                    value
                };
                canvas.paint(index)?;
            }
            continue;
        }
        match value {
            END_OF_LINE => {
                canvas.x = 0;
                canvas.y += 1;
            }
            END_OF_BITMAP => break,
            DELTA => {
                let &[dx, dy] = stream.get(cursor..cursor + 2)? else {
                    return None;
                };
                cursor += 2;
                canvas.x = canvas.x.checked_add(usize::from(dx))?;
                canvas.y = canvas.y.checked_add(usize::from(dy))?;
                if canvas.x > width || canvas.y > height {
                    return None;
                }
            }
            literal => {
                let literal = usize::from(literal);
                let used = if bits == 4 {
                    literal.div_ceil(2)
                } else {
                    literal
                };
                // Absolute blocks are padded to a 16-bit boundary.
                let block = stream.get(cursor..cursor + used)?;
                cursor += used.next_multiple_of(2);
                for position in 0..literal {
                    let index = if bits == 4 {
                        nibble(block[position / 2], position)
                    } else {
                        block[position]
                    };
                    canvas.paint(index)?;
                }
            }
        }
    }
    RgbaImage::from_raw(width as u32, height as u32, canvas.pixels)
}
