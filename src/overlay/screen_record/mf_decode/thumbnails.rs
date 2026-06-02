use base64::Engine as _;
use windows::Win32::Media::MediaFoundation::*;

use super::{make_i64_propvariant, mf_shutdown, mf_startup};

/// Generate lightweight JPEG thumbnails via MF SourceReader in software RGB32 mode.
pub fn generate_thumbnails(
    file_path: &str,
    count: u32,
    start_sec: f64,
    end_sec: f64,
) -> Result<Vec<String>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }

    mf_startup()?;

    let result = (|| -> Result<Vec<String>, String> {
        let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut attrs: Option<IMFAttributes> = None;
        unsafe {
            MFCreateAttributes(&mut attrs, 2).map_err(|e| format!("MFCreateAttributes: {e}"))?;
        }
        let attrs = attrs.ok_or("MFCreateAttributes returned null")?;
        unsafe {
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(|e| format!("SetUINT32 ADVANCED_VIDEO_PROCESSING: {e}"))?;
        }

        let reader = unsafe {
            MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };
        let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        let media_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| format!("SetGUID major type: {e}"))?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|e| format!("SetGUID subtype RGB32: {e}"))?;
            reader
                .SetCurrentMediaType(video_idx, None, &media_type)
                .map_err(|e| format!("SetCurrentMediaType RGB32: {e}"))?;
        }

        let current_type = unsafe {
            reader
                .GetCurrentMediaType(video_idx)
                .map_err(|e| format!("GetCurrentMediaType: {e}"))?
        };
        let frame_size = unsafe {
            current_type
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
        };
        let width = (frame_size >> 32) as u32;
        let height = (frame_size & 0xFFFF_FFFF) as u32;

        let safe_end = if end_sec > start_sec {
            end_sec
        } else {
            start_sec
        };
        let duration = (safe_end - start_sec).max(0.0);
        let step = if count > 1 {
            duration / (count - 1) as f64
        } else {
            0.0
        };

        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let t = start_sec + (i as f64 * step);
            let propvar = make_i64_propvariant((t * 10_000_000.0) as i64);
            unsafe {
                let _ = reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar);
            }

            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                let _ = reader.ReadSample(
                    video_idx,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                );
            }

            if (flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                out.push(String::new());
                continue;
            }

            let Some(sample) = sample else {
                out.push(String::new());
                continue;
            };

            let buffer = unsafe {
                sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| format!("ConvertToContiguousBuffer: {e}"))?
            };
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut length = 0u32;
            unsafe {
                buffer
                    .Lock(&mut data_ptr, None, Some(&mut length))
                    .map_err(|e| format!("IMFMediaBuffer::Lock: {e}"))?;
            }

            let thumb_result = (|| -> Result<String, String> {
                let raw = unsafe { std::slice::from_raw_parts(data_ptr, length as usize) };
                let mut rgba = raw.to_vec();
                for px in rgba.chunks_exact_mut(4) {
                    px.swap(0, 2); // BGRA -> RGBA
                }

                let Some(img) = image::RgbaImage::from_raw(width, height, rgba) else {
                    return Ok(String::new());
                };
                let resized =
                    image::imageops::resize(&img, 160, 90, image::imageops::FilterType::Triangle);
                let mut jpg = Vec::new();
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 60);
                enc.encode_image(&image::DynamicImage::ImageRgba8(resized))
                    .map_err(|e| format!("JpegEncoder: {e}"))?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
                Ok(format!("data:image/jpeg;base64,{b64}"))
            })();

            unsafe {
                let _ = buffer.Unlock();
            }
            out.push(thumb_result.unwrap_or_default());
        }

        Ok(out)
    })();

    let _ = mf_shutdown();
    result
}

/// Generate lightweight JPEG thumbnails at exact source times.
///
/// Timeline trim thumbnails are sampled in compact timeline time, then mapped
/// back to source time by the WebView. Accepting explicit times avoids forcing
/// contiguous start/end sampling for multi-cut compositions.
pub fn generate_thumbnails_at_times(
    file_path: &str,
    times_sec: &[f64],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<String>, String> {
    if times_sec.is_empty() {
        return Ok(Vec::new());
    }

    mf_startup()?;

    let result = (|| -> Result<Vec<String>, String> {
        let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut attrs: Option<IMFAttributes> = None;
        unsafe {
            MFCreateAttributes(&mut attrs, 3).map_err(|e| format!("MFCreateAttributes: {e}"))?;
        }
        let attrs = attrs.ok_or("MFCreateAttributes returned null")?;
        unsafe {
            attrs
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(|e| format!("SetUINT32 HW_TRANSFORMS: {e}"))?;
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(|e| format!("SetUINT32 ADVANCED_VIDEO_PROCESSING: {e}"))?;
        }

        let reader = unsafe {
            MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
                .map_err(|e| format!("MFCreateSourceReaderFromURL: {e}"))?
        };
        let video_idx = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        let media_type =
            unsafe { MFCreateMediaType().map_err(|e| format!("MFCreateMediaType: {e}"))? };
        unsafe {
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| format!("SetGUID major type: {e}"))?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|e| format!("SetGUID subtype RGB32: {e}"))?;
            reader
                .SetCurrentMediaType(video_idx, None, &media_type)
                .map_err(|e| format!("SetCurrentMediaType RGB32: {e}"))?;
        }

        let current_type = unsafe {
            reader
                .GetCurrentMediaType(video_idx)
                .map_err(|e| format!("GetCurrentMediaType: {e}"))?
        };
        let frame_size = unsafe {
            current_type
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|e| format!("GetUINT64 FRAME_SIZE: {e}"))?
        };
        let source_width = (frame_size >> 32) as u32;
        let source_height = (frame_size & 0xFFFF_FFFF) as u32;
        let thumb_width = width.clamp(64, 640);
        let thumb_height = height.clamp(36, 360);
        let jpeg_quality = quality.clamp(30, 92);

        let mut out = Vec::with_capacity(times_sec.len());
        for &time_sec in times_sec {
            let safe_time = if time_sec.is_finite() {
                time_sec.max(0.0)
            } else {
                0.0
            };
            let propvar = make_i64_propvariant((safe_time * 10_000_000.0) as i64);
            unsafe {
                let _ = reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &propvar);
            }

            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                let _ = reader.ReadSample(
                    video_idx,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                );
            }

            if (flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                out.push(String::new());
                continue;
            }

            let Some(sample) = sample else {
                out.push(String::new());
                continue;
            };

            let buffer = unsafe {
                sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| format!("ConvertToContiguousBuffer: {e}"))?
            };
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut length = 0u32;
            unsafe {
                buffer
                    .Lock(&mut data_ptr, None, Some(&mut length))
                    .map_err(|e| format!("IMFMediaBuffer::Lock: {e}"))?;
            }

            let thumb_result = (|| -> Result<String, String> {
                let raw = unsafe { std::slice::from_raw_parts(data_ptr, length as usize) };
                let mut rgba = raw.to_vec();
                for px in rgba.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }

                let Some(img) = image::RgbaImage::from_raw(source_width, source_height, rgba)
                else {
                    return Ok(String::new());
                };
                let resized = image::imageops::resize(
                    &img,
                    thumb_width,
                    thumb_height,
                    image::imageops::FilterType::Triangle,
                );
                let mut jpg = Vec::new();
                let mut enc =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, jpeg_quality);
                enc.encode_image(&image::DynamicImage::ImageRgba8(resized))
                    .map_err(|e| format!("JpegEncoder: {e}"))?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
                Ok(format!("data:image/jpeg;base64,{b64}"))
            })();

            unsafe {
                let _ = buffer.Unlock();
            }
            out.push(thumb_result.unwrap_or_default());
        }

        Ok(out)
    })();

    let _ = mf_shutdown();
    result
}
