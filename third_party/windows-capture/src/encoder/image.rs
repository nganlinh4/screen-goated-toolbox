use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use windows::Storage::Streams::{
    Buffer, DataReader, InMemoryRandomAccessStream, InputStreamOptions,
};

use crate::frame::ImageFormat;
use crate::settings::ColorFormat;

#[derive(thiserror::Error, Eq, PartialEq, Clone, Debug)]
pub enum ImageEncoderError {
    #[error("This color format is not supported for saving as an image")]
    UnsupportedFormat,
    #[error("Windows API error: {0}")]
    WindowsError(#[from] windows::core::Error),
}

/// The `ImageEncoder` struct is used to encode image buffers into image bytes with a specified format and color format.
pub struct ImageEncoder {
    format: ImageFormat,
    color_format: ColorFormat,
}

impl ImageEncoder {
    /// Creates a new `ImageEncoder` with the specified format and color format.
    #[must_use]
    #[inline]
    pub const fn new(format: ImageFormat, color_format: ColorFormat) -> Self {
        Self {
            format,
            color_format,
        }
    }

    /// Encodes the image buffer into image bytes with the specified format.
    #[inline]
    pub fn encode(
        &self,
        image_buffer: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, ImageEncoderError> {
        let encoder = match self.format {
            ImageFormat::Jpeg => BitmapEncoder::JpegEncoderId()?,
            ImageFormat::Png => BitmapEncoder::PngEncoderId()?,
            ImageFormat::Gif => BitmapEncoder::GifEncoderId()?,
            ImageFormat::Tiff => BitmapEncoder::TiffEncoderId()?,
            ImageFormat::Bmp => BitmapEncoder::BmpEncoderId()?,
            ImageFormat::JpegXr => BitmapEncoder::JpegXREncoderId()?,
        };

        let stream = InMemoryRandomAccessStream::new()?;
        let encoder = BitmapEncoder::CreateAsync(encoder, &stream)?.join()?;

        let pixelformat = match self.color_format {
            ColorFormat::Bgra8 => BitmapPixelFormat::Bgra8,
            ColorFormat::Rgba8 => BitmapPixelFormat::Rgba8,
            ColorFormat::Rgba16F => return Err(ImageEncoderError::UnsupportedFormat),
        };

        encoder.SetPixelData(
            pixelformat,
            BitmapAlphaMode::Premultiplied,
            width,
            height,
            1.0,
            1.0,
            image_buffer,
        )?;

        encoder.FlushAsync()?.join()?;

        let buffer = Buffer::Create(u32::try_from(stream.Size()?).unwrap())?;
        stream
            .ReadAsync(&buffer, buffer.Capacity()?, InputStreamOptions::None)?
            .join()?;

        let data_reader = DataReader::FromBuffer(&buffer)?;
        let length = data_reader.UnconsumedBufferLength()?;
        let mut bytes = vec![0u8; length as usize];
        data_reader.ReadBytes(&mut bytes)?;

        Ok(bytes)
    }
}
