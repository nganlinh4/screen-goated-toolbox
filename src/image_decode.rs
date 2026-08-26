//! Bounded image decoding with native Windows coverage for uncommon formats.

use anyhow::{Context as _, Result, bail};
use image::{DynamicImage, ImageEncoder as _, RgbaImage};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};

const MAX_ENCODED_BYTES: usize = 256 * 1024 * 1024;
const MAX_AXIS: u32 = 32_768;
const MAX_PIXELS: u64 = 100_000_000;

struct ComScope(bool);

impl ComScope {
    fn enter() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status.is_ok() {
            Ok(Self(true))
        } else if status == RPC_E_CHANGED_MODE {
            // The thread already owns another COM apartment. WIC is apartment
            // agile, and this call did not acquire an uninitialization lease.
            Ok(Self(false))
        } else {
            Err(windows::core::Error::from_hresult(status).into())
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// Decode a still image. Rust decoders keep common PNG/JPEG/WebP input fast;
/// Windows Imaging Component supplies broad installed-format coverage without
/// carrying every uncommon codec in the executable.
pub fn load_from_memory(bytes: &[u8]) -> Result<DynamicImage> {
    if bytes.len() > MAX_ENCODED_BYTES {
        bail!("encoded image exceeds the {} byte limit", MAX_ENCODED_BYTES);
    }
    if let Ok(image) = image::load_from_memory(bytes) {
        validate_dimensions(image.width(), image.height())?;
        return Ok(image);
    }
    decode_with_wic(bytes).context("Windows image decoder rejected the image")
}

/// Decode for the image-preset pipeline while retaining common source bytes.
/// Native-only formats are normalized to PNG once so every downstream stage
/// sees a truthful, universally supported media payload.
pub fn load_for_pipeline(bytes: Vec<u8>) -> Result<(RgbaImage, Vec<u8>)> {
    if bytes.len() > MAX_ENCODED_BYTES {
        bail!("encoded image exceeds the {} byte limit", MAX_ENCODED_BYTES);
    }
    if let Ok(image) = image::load_from_memory(&bytes) {
        validate_dimensions(image.width(), image.height())?;
        return Ok((image.to_rgba8(), bytes));
    }

    let rgba = decode_with_wic(&bytes)?.to_rgba8();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok((rgba, png))
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 || width > MAX_AXIS || height > MAX_AXIS || pixels > MAX_PIXELS {
        bail!("image dimensions {width}x{height} exceed the decode limits");
    }
    Ok(())
}

fn decode_with_wic(bytes: &[u8]) -> Result<DynamicImage> {
    let _com = ComScope::enter()?;
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
    let stream = unsafe { factory.CreateStream() }?;
    unsafe { stream.InitializeFromMemory(bytes) }?;
    let decoder = unsafe {
        factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
    }?;
    let frame = unsafe { decoder.GetFrame(0) }?;

    let mut width = 0;
    let mut height = 0;
    unsafe { frame.GetSize(&mut width, &mut height) }?;
    validate_dimensions(width, height)?;

    let stride = width.checked_mul(4).context("image row is too wide")?;
    let byte_len = usize::try_from(stride)
        .ok()
        .and_then(|row| row.checked_mul(height as usize))
        .context("decoded image allocation overflow")?;
    let mut pixels = vec![0_u8; byte_len];

    let converter = unsafe { factory.CreateFormatConverter() }?;
    unsafe {
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        converter.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
    }

    let image = RgbaImage::from_raw(width, height, pixels)
        .context("Windows image decoder returned an invalid pixel buffer")?;
    Ok(DynamicImage::ImageRgba8(image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    #[test]
    fn common_decoder_stays_bounded() {
        let source = RgbaImage::from_pixel(2, 3, image::Rgba([1, 2, 3, 255]));
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(source.as_raw(), 2, 3, image::ExtendedColorType::Rgba8)
            .unwrap();

        let decoded = load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 3));
    }

    #[test]
    fn native_decoder_covers_uncommon_still_formats() {
        // A 1x1 GIF89a. The Rust GIF feature is intentionally absent, so this
        // exercises the WIC fallback rather than a second bundled codec.
        let gif = base64::engine::general_purpose::STANDARD
            .decode("R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==")
            .unwrap();
        let decoded = load_from_memory(&gif).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1, 1));
    }

    #[test]
    fn oversized_encoded_input_is_rejected_before_decode() {
        let bytes = vec![0_u8; MAX_ENCODED_BYTES + 1];
        assert!(load_from_memory(&bytes).is_err());
    }
}
