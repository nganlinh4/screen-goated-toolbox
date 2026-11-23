//! Header-only probes for imported media: whether a video carries audio, and
//! how long an audio file plays. Neither decodes any samples.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use windows::Win32::Media::MediaFoundation::*;

use crate::overlay::screen_record::mf_decode;

pub(super) fn probe_media_has_audio(path: &Path) -> Result<bool, String> {
    mf_decode::mf_startup()?;

    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    let wide_path: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let reader = unsafe {
        MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
            .map_err(|e| format!("MFCreateSourceReaderFromURL audio probe: {e}"))?
    };

    let audio_index = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    Ok(unsafe { reader.GetNativeMediaType(audio_index, 0).is_ok() })
}

/// Probe an audio file's duration in seconds without decoding any samples.
/// Symphonia covers every accepted audio extension.
pub(super) fn probe_audio_duration_seconds(path: &Path) -> Result<f64, String> {
    let file = File::open(path).map_err(|e| format!("Open audio source: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("Symphonia probe: {e}"))?;

    let track = select_audio_track(format.tracks())
        .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio());
    if let (Some(n_frames), Some(sr)) = (
        track.num_frames,
        params.and_then(|parameters| parameters.sample_rate),
    ) && sr > 0
    {
        return Ok(n_frames as f64 / sr as f64);
    }
    if let (Some(time_base), Some(duration)) = (track.time_base, track.duration)
        && let Some(time) = time_base.calc_duration(duration)
    {
        return Ok(time.as_secs_f64());
    }

    Err("Symphonia: cannot determine audio duration".to_string())
}

fn audio_params(
    track: &symphonia::core::formats::Track,
) -> Option<&symphonia::core::codecs::audio::AudioCodecParameters> {
    track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .filter(|params| params.codec != CODEC_ID_NULL_AUDIO)
}

fn select_audio_track(
    tracks: &[symphonia::core::formats::Track],
) -> Option<&symphonia::core::formats::Track> {
    tracks
        .iter()
        .find(|track| {
            audio_params(track).is_some_and(|params| {
                params.sample_rate.is_some()
                    || params.channels.is_some()
                    || params.codec == CODEC_ID_OPUS
            })
        })
        .or_else(|| tracks.iter().find(|track| audio_params(track).is_some()))
}
