use std::fs::File;
use std::path::Path;
use std::time::Instant;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::audio::{Channels, Layout};
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::codecs::CODEC_TYPE_OPUS;
use symphonia::core::codecs::CodecRegistry;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;
use symphonia_adapter_libopus::OpusDecoder;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::core::Interface;

use crate::overlay::screen_record::d3d_interop::{D3D11GpuFence, create_d3d11_device};
use crate::overlay::screen_record::mf_audio::{AudioConfig, AudioStream, MfAudioDecoder};
use crate::overlay::screen_record::mf_decode::{self, DecodedFrame, DxgiDeviceManager, MfDecoder};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};
use crate::overlay::screen_record::native_export::config::compute_default_video_bitrate_kbps;

use super::{
    NORMALIZED_IMPORT_AUDIO_BITRATE_KBPS, NORMALIZED_IMPORT_AUDIO_CHANNELS,
    NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE,
};

const IMPORT_REORDER_WINDOW: usize = 6;

enum ImportAudioDecoder {
    Mf(MfAudioDecoder),
    Symphonia(SymphoniaImportAudioDecoder),
}

impl ImportAudioDecoder {
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Mf(decoder) => decoder.sample_rate(),
            Self::Symphonia(decoder) => decoder.sample_rate(),
        }
    }

    fn channels(&self) -> u32 {
        match self {
            Self::Mf(decoder) => decoder.channels(),
            Self::Symphonia(decoder) => decoder.channels(),
        }
    }

    fn read_samples(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        match self {
            Self::Mf(decoder) => decoder.read_samples(),
            Self::Symphonia(decoder) => decoder.read_samples(),
        }
    }
}

struct SymphoniaImportAudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u32,
    time_base: Option<TimeBase>,
    next_pts_100ns: i64,
    pending_sample: Option<(Vec<u8>, i64)>,
}

impl SymphoniaImportAudioDecoder {
    fn new(file_path: &str) -> Result<Self, String> {
        let file = File::open(file_path).map_err(|e| format!("Open audio source: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
        {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("Symphonia probe: {e}"))?;

        let format = probed.format;
        let track = select_symphonia_audio_track(format.tracks())
            .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

        let track_id = track.id;
        let mut codec_params = track.codec_params.clone();
        backfill_opus_codec_params(file_path, &mut codec_params);
        let time_base = codec_params.time_base;
        let next_pts_100ns = time_base
            .map(|time_base| symphonia_timestamp_to_100ns(time_base, codec_params.start_ts))
            .unwrap_or(0);

        let mut codec_registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut codec_registry);
        codec_registry.register_all::<OpusDecoder>();
        let decoder = codec_registry
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Symphonia decoder init: {e}"))?;

        let mut decoder = Self {
            format,
            decoder,
            track_id,
            sample_rate: codec_params.sample_rate.unwrap_or(0),
            channels: codec_params
                .channels
                .map(|channels| channels.count() as u32)
                .unwrap_or(0),
            time_base,
            next_pts_100ns,
            pending_sample: None,
        };

        let first_sample = decoder
            .decode_next_sample()
            .map_err(|error| format!("Symphonia first decode: {error}"))?
            .ok_or_else(|| "Symphonia: no decodable audio frames found".to_string())?;
        decoder.pending_sample = Some(first_sample);

        if decoder.sample_rate == 0 {
            return Err("Symphonia: missing sample rate after first decoded frame".to_string());
        }
        if decoder.channels == 0 {
            return Err("Symphonia: missing channel layout after first decoded frame".to_string());
        }

        Ok(decoder)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u32 {
        self.channels
    }

    fn read_samples(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        if let Some(sample) = self.pending_sample.take() {
            return Ok(Some(sample));
        }

        self.decode_next_sample()
    }

    fn decode_next_sample(&mut self) -> Result<Option<(Vec<u8>, i64)>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(ref error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => continue,
                Err(error) => return Err(format!("Symphonia next_packet: {error}")),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let packet_timestamp_100ns = self
                .time_base
                .map(|time_base| symphonia_timestamp_to_100ns(time_base, packet.ts()));

            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => continue,
                Err(SymphoniaError::IoError(ref error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(format!("Symphonia decode: {error}")),
            };

            let spec = *decoded.spec();
            if self.sample_rate == 0 {
                self.sample_rate = spec.rate;
            }
            if self.channels == 0 {
                self.channels = spec.channels.count() as u32;
            }

            let mut sample_buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buffer.copy_interleaved_ref(decoded);

            let samples = sample_buffer.samples();
            if samples.is_empty() {
                continue;
            }

            let timestamp_100ns = packet_timestamp_100ns.unwrap_or(self.next_pts_100ns);
            let channel_count = self.channels.max(1) as usize;
            let samples_per_channel = samples.len() / channel_count;
            let duration_100ns =
                ((samples_per_channel as i64) * 10_000_000 / self.sample_rate as i64).max(1);
            self.next_pts_100ns = timestamp_100ns + duration_100ns;

            return Ok(Some((
                bytemuck::cast_slice(samples).to_vec(),
                timestamp_100ns,
            )));
        }
    }
}

pub(super) fn normalize_imported_video_mf(
    input_path: &Path,
    output_path: &Path,
    trace_id: &str,
) -> Result<bool, String> {
    let started_at = Instant::now();
    let input_path_str = input_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();

    mf_decode::mf_startup()?;
    let metadata = mf_decode::probe_video_metadata(&input_path_str)?;
    if metadata.width == 0 || metadata.height == 0 {
        return Err("Media Foundation probe returned zero-sized video".to_string());
    }
    if metadata.width % 2 != 0 || metadata.height % 2 != 0 {
        return Err(format!(
            "Media Foundation import normalize requires even dimensions, got {}x{}",
            metadata.width, metadata.height
        ));
    }

    let fps_num = metadata.fps_num.max(1);
    let fps_den = metadata.fps_den.max(1);
    let nominal_fps = ((fps_num as f64 / fps_den as f64).round() as u32).max(1);
    let bitrate_kbps =
        compute_default_video_bitrate_kbps(metadata.width, metadata.height, nominal_fps);
    let nominal_frame_duration_100ns = ((10_000_000i64 * fps_den as i64) / fps_num as i64).max(1);

    crate::log_info!(
        "[VideoImport:{}][Normalize][MF] start input=\"{}\" output=\"{}\" {}x{} fps={}/{} bitrate_kbps={}",
        trace_id,
        input_path.display(),
        output_path.display(),
        metadata.width,
        metadata.height,
        fps_num,
        fps_den,
        bitrate_kbps
    );

    let (d3d11_device, d3d11_context) = create_d3d11_device()?;
    {
        let multithread: ID3D11Multithread = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread: {e}"))?;
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
    }
    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;
    let copy_fence = D3D11GpuFence::new(&d3d11_device, &d3d11_context)?;
    let video_decoder = MfDecoder::new(&input_path_str, &device_manager, true)?;

    let has_audio = probe_has_audio_track(&input_path_str)?;
    let mut audio_decoder = if has_audio {
        Some(open_import_audio_decoder(&input_path_str, trace_id)?)
    } else {
        None
    };
    let audio_config = audio_decoder.as_ref().map(|decoder| AudioConfig {
        sample_rate: decoder.sample_rate(),
        channels: decoder.channels(),
        bitrate_kbps: NORMALIZED_IMPORT_AUDIO_BITRATE_KBPS,
    });

    let encoder_config = EncoderConfig {
        codec: VideoCodec::H264,
        input_surface_format: VideoInputSurfaceFormat::Nv12,
        width: metadata.width,
        height: metadata.height,
        fps_num,
        fps_den,
        bitrate_kbps,
    };
    let (encoder, audio_stream) = MfEncoder::new(
        &output_path_str,
        encoder_config,
        &device_manager,
        audio_config.as_ref(),
    )?;

    let mut reorder_queue = Vec::with_capacity(IMPORT_REORDER_WINDOW);
    let mut video_eof = false;
    let mut pending_frame =
        next_video_frame_in_presentation_order(&video_decoder, &mut reorder_queue, &mut video_eof)?
            .ok_or_else(|| "No video frames decoded from imported file".to_string())?;

    let mut pending_audio_sample = if let Some(decoder) = audio_decoder.as_mut() {
        decoder.read_samples()?
    } else {
        None
    };

    let base_pts_100ns = pending_audio_sample
        .as_ref()
        .map(|(_, ts)| *ts)
        .map(|audio_ts| audio_ts.min(pending_frame.pts_100ns))
        .unwrap_or(pending_frame.pts_100ns);

    let mut audio_chunks_written = 0u64;
    let mut last_audio_end_100ns = 0i64;
    let mut last_video_end_100ns = 0i64;
    let mut video_frames_encoded = 0u64;

    loop {
        let next_frame = next_video_frame_in_presentation_order(
            &video_decoder,
            &mut reorder_queue,
            &mut video_eof,
        )?;
        let Some(next_frame) = next_frame else {
            break;
        };

        let frame_duration_100ns =
            (next_frame.pts_100ns - pending_frame.pts_100ns).max(nominal_frame_duration_100ns);
        if let (Some(decoder), Some(stream)) = (audio_decoder.as_mut(), audio_stream.as_ref()) {
            let current_video_timestamp_100ns = (pending_frame.pts_100ns - base_pts_100ns).max(0);
            drain_audio_until_target(
                stream,
                &encoder,
                decoder,
                &mut pending_audio_sample,
                base_pts_100ns,
                current_video_timestamp_100ns,
                &mut last_audio_end_100ns,
                &mut audio_chunks_written,
            )?;
        }
        encode_video_frame(
            &encoder,
            &d3d11_device,
            &d3d11_context,
            &copy_fence,
            &pending_frame,
            base_pts_100ns,
            frame_duration_100ns,
            &mut last_video_end_100ns,
        )?;
        video_frames_encoded += 1;
        pending_frame = next_frame;
    }

    encode_video_frame(
        &encoder,
        &d3d11_device,
        &d3d11_context,
        &copy_fence,
        &pending_frame,
        base_pts_100ns,
        nominal_frame_duration_100ns,
        &mut last_video_end_100ns,
    )?;
    video_frames_encoded += 1;

    if let (Some(decoder), Some(stream)) = (audio_decoder.as_mut(), audio_stream.as_ref()) {
        let flush_target_100ns = last_video_end_100ns.max(0);
        drain_audio_until_target(
            stream,
            &encoder,
            decoder,
            &mut pending_audio_sample,
            base_pts_100ns,
            flush_target_100ns,
            &mut last_audio_end_100ns,
            &mut audio_chunks_written,
        )?;

        while let Some((pcm, timestamp_100ns)) = pending_audio_sample.take() {
            if write_audio_chunk(
                stream,
                &encoder,
                decoder,
                &pcm,
                timestamp_100ns,
                base_pts_100ns,
                &mut last_audio_end_100ns,
            )? {
                audio_chunks_written += 1;
            }
            pending_audio_sample = decoder.read_samples()?;
        }
    }
    encoder.finalize()?;

    crate::log_info!(
        "[VideoImport:{}][Normalize][MF] complete {:.3}s has_audio={} video_frames={} audio_chunks={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        has_audio,
        video_frames_encoded,
        audio_chunks_written
    );

    Ok(has_audio)
}

pub(super) fn probe_media_has_audio(path: &Path) -> Result<bool, String> {
    probe_has_audio_track(&path.to_string_lossy())
}

/// Probe an audio file's duration in seconds without decoding any samples.
/// Uses symphonia which already covers every audio extension we accept
/// (mp3, wav, m4a, flac, ogg, aac, alac, aiff, opus, …).
pub(super) fn probe_audio_duration_seconds(path: &Path) -> Result<f64, String> {
    let file = File::open(path).map_err(|e| format!("Open audio source: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Symphonia probe: {e}"))?;

    let track = select_symphonia_audio_track(probed.format.tracks())
        .ok_or_else(|| "Symphonia: no decodable audio track found".to_string())?;

    let params = &track.codec_params;
    if let (Some(n_frames), Some(sr)) = (params.n_frames, params.sample_rate)
        && sr > 0
    {
        return Ok(n_frames as f64 / sr as f64);
    }
    if let (Some(time_base), Some(n_frames)) = (params.time_base, params.n_frames) {
        let time = time_base.calc_time(n_frames);
        return Ok(time.seconds as f64 + time.frac);
    }

    Err("Symphonia: cannot determine audio duration".to_string())
}

fn open_import_audio_decoder(
    file_path: &str,
    trace_id: &str,
) -> Result<ImportAudioDecoder, String> {
    match MfAudioDecoder::new_with_preferred_output_format(
        file_path,
        Some(NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE),
        Some(NORMALIZED_IMPORT_AUDIO_CHANNELS),
    ) {
        Ok(decoder) => {
            let mode = if decoder.sample_rate() == NORMALIZED_IMPORT_AUDIO_SAMPLE_RATE
                && decoder.channels() == NORMALIZED_IMPORT_AUDIO_CHANNELS
            {
                "normalized"
            } else {
                "native"
            };
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio] using {} decode {}Hz {}ch",
                trace_id,
                mode,
                decoder.sample_rate(),
                decoder.channels()
            );
            Ok(ImportAudioDecoder::Mf(decoder))
        }
        Err(error) => {
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio] Media Foundation does not support this audio track on this machine, trying Symphonia instead",
                trace_id
            );
            crate::log_info!(
                "[VideoImport:{}][Normalize][MF][Audio][Detail] {}",
                trace_id,
                error
            );
            let decoder = SymphoniaImportAudioDecoder::new(file_path)?;
            crate::log_info!(
                "[VideoImport:{}][Normalize][Symphonia][Audio] using native decode {}Hz {}ch",
                trace_id,
                decoder.sample_rate(),
                decoder.channels()
            );
            Ok(ImportAudioDecoder::Symphonia(decoder))
        }
    }
}

fn symphonia_timestamp_to_100ns(time_base: TimeBase, timestamp: u64) -> i64 {
    let time = time_base.calc_time(timestamp);
    (time.seconds as i64 * 10_000_000) + (time.frac * 10_000_000.0).round() as i64
}

fn select_symphonia_audio_track<'a>(
    tracks: &'a [symphonia::core::formats::Track],
) -> Option<&'a symphonia::core::formats::Track> {
    tracks
        .iter()
        .find(|track| {
            let params = &track.codec_params;
            params.codec != CODEC_TYPE_NULL
                && (params.sample_rate.is_some()
                    || params.channels.is_some()
                    || params.channel_layout.is_some()
                    || params.codec == CODEC_TYPE_OPUS)
        })
        .or_else(|| {
            tracks
                .iter()
                .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        })
}

fn backfill_opus_codec_params(
    file_path: &str,
    codec_params: &mut symphonia::core::codecs::CodecParameters,
) {
    if codec_params.codec != CODEC_TYPE_OPUS {
        return;
    }

    if codec_params.sample_rate.is_none() {
        codec_params.with_sample_rate(48_000);
    }

    if codec_params.channels.is_some() {
        return;
    }

    let channel_count = opus_channel_count_from_extra_data(codec_params.extra_data.as_deref())
        .or_else(|| probe_opus_channel_count_from_mp4(file_path));

    let Some(channel_count) = channel_count else {
        return;
    };

    match channel_count {
        1 => {
            codec_params
                .with_channels(Layout::Mono.into_channels())
                .with_channel_layout(Layout::Mono);
        }
        2 => {
            codec_params
                .with_channels(Layout::Stereo.into_channels())
                .with_channel_layout(Layout::Stereo);
        }
        count if count > 0 => {
            let mut channels = Channels::empty();
            for index in 0..count.min(32) {
                channels |= Channels::from_bits_truncate(1u32 << index);
            }
            codec_params.with_channels(channels);
        }
        _ => {}
    }
}

fn opus_channel_count_from_extra_data(extra_data: Option<&[u8]>) -> Option<u8> {
    let extra_data = extra_data?;
    if extra_data.len() < 10 || &extra_data[..8] != b"OpusHead" {
        return None;
    }
    let count = extra_data[9];
    if count == 0 { None } else { Some(count) }
}

fn probe_opus_channel_count_from_mp4(file_path: &str) -> Option<u8> {
    let bytes = std::fs::read(file_path).ok()?;
    let pattern = b"dOps";
    let search_end = bytes.len().saturating_sub(pattern.len() + 6);

    for index in 0..=search_end {
        if &bytes[index..index + pattern.len()] != pattern {
            continue;
        }

        let version = bytes.get(index + 4).copied().unwrap_or_default();
        let channel_count = bytes.get(index + 5).copied().unwrap_or_default();
        if version == 0 && channel_count > 0 {
            return Some(channel_count);
        }
    }

    None
}

fn probe_has_audio_track(file_path: &str) -> Result<bool, String> {
    mf_decode::mf_startup()?;

    let mut attrs: Option<IMFAttributes> = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("MFCreateAttributes: {e}"))?;
    }
    let attrs = attrs.ok_or("MFCreateAttributes returned null")?;

    let wide_path: Vec<u16> = file_path.encode_utf16().chain(std::iter::once(0)).collect();
    let reader = unsafe {
        MFCreateSourceReaderFromURL(windows::core::PCWSTR(wide_path.as_ptr()), &attrs)
            .map_err(|e| format!("MFCreateSourceReaderFromURL audio probe: {e}"))?
    };

    let audio_index = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    Ok(unsafe { reader.GetNativeMediaType(audio_index, 0).is_ok() })
}

fn next_video_frame_in_presentation_order(
    decoder: &MfDecoder,
    reorder_queue: &mut Vec<DecodedFrame>,
    eof_reached: &mut bool,
) -> Result<Option<DecodedFrame>, String> {
    while reorder_queue.len() < IMPORT_REORDER_WINDOW && !*eof_reached {
        match decoder.read_frame()? {
            Some(frame) => reorder_queue.push(frame),
            None => *eof_reached = true,
        }
    }
    reorder_queue.sort_by(|left, right| right.pts_100ns.cmp(&left.pts_100ns));
    Ok(reorder_queue.pop())
}

fn encode_video_frame(
    encoder: &MfEncoder,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    copy_fence: &D3D11GpuFence,
    frame: &DecodedFrame,
    base_pts_100ns: i64,
    duration_100ns: i64,
    last_video_end_100ns: &mut i64,
) -> Result<(), String> {
    let texture = copy_frame_to_private_texture(device, context, copy_fence, frame)?;
    let mut timestamp_100ns = (frame.pts_100ns - base_pts_100ns).max(0);
    if timestamp_100ns < *last_video_end_100ns {
        timestamp_100ns = *last_video_end_100ns;
    }
    let duration_100ns = duration_100ns.max(1);
    encoder.write_frame_gpu(&texture, timestamp_100ns, duration_100ns)?;
    *last_video_end_100ns = timestamp_100ns + duration_100ns;
    Ok(())
}

fn copy_frame_to_private_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    copy_fence: &D3D11GpuFence,
    frame: &DecodedFrame,
) -> Result<ID3D11Texture2D, String> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        frame.texture.GetDesc(&mut desc);
    }
    desc.Usage = D3D11_USAGE_DEFAULT;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = 0;
    desc.MiscFlags = 0;

    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .map_err(|e| format!("CreateTexture2D import-private: {e}"))?;
    }
    let texture = texture.ok_or("CreateTexture2D import-private returned null")?;

    let source_resource: ID3D11Resource = frame
        .texture
        .cast()
        .map_err(|e| format!("frame texture -> ID3D11Resource: {e}"))?;
    let dest_resource: ID3D11Resource = texture
        .cast()
        .map_err(|e| format!("private texture -> ID3D11Resource: {e}"))?;

    unsafe {
        context.CopyResource(&dest_resource, &source_resource);
    }
    copy_fence.signal_and_wait();

    Ok(texture)
}

fn write_audio_chunk(
    stream: &AudioStream,
    encoder: &MfEncoder,
    decoder: &ImportAudioDecoder,
    pcm_data: &[u8],
    timestamp_100ns: i64,
    base_pts_100ns: i64,
    last_audio_end_100ns: &mut i64,
) -> Result<bool, String> {
    let channels = decoder.channels() as usize;
    if channels == 0 || pcm_data.is_empty() {
        return Ok(false);
    }

    let samples_per_channel = pcm_data.len() / (channels * 4);
    if samples_per_channel == 0 {
        return Ok(false);
    }

    let duration_100ns =
        ((samples_per_channel as i64) * 10_000_000 / decoder.sample_rate() as i64).max(1);
    let mut relative_timestamp_100ns = (timestamp_100ns - base_pts_100ns).max(0);
    if relative_timestamp_100ns < *last_audio_end_100ns {
        relative_timestamp_100ns = *last_audio_end_100ns;
    }

    stream.write_samples_direct(
        encoder.writer(),
        pcm_data,
        relative_timestamp_100ns,
        duration_100ns,
    )?;
    *last_audio_end_100ns = relative_timestamp_100ns + duration_100ns;
    Ok(true)
}

fn drain_audio_until_target(
    stream: &AudioStream,
    encoder: &MfEncoder,
    decoder: &mut ImportAudioDecoder,
    pending_audio_sample: &mut Option<(Vec<u8>, i64)>,
    base_pts_100ns: i64,
    target_timestamp_100ns: i64,
    last_audio_end_100ns: &mut i64,
    audio_chunks_written: &mut u64,
) -> Result<(), String> {
    loop {
        let should_write = pending_audio_sample
            .as_ref()
            .map(|(_, timestamp_100ns)| {
                (*timestamp_100ns - base_pts_100ns).max(0) <= target_timestamp_100ns
            })
            .unwrap_or(false);
        if !should_write {
            break;
        }

        let (pcm, timestamp_100ns) = pending_audio_sample
            .take()
            .ok_or_else(|| "Missing pending audio sample".to_string())?;
        if write_audio_chunk(
            stream,
            encoder,
            decoder,
            &pcm,
            timestamp_100ns,
            base_pts_100ns,
            last_audio_end_100ns,
        )? {
            *audio_chunks_written += 1;
        }
        *pending_audio_sample = decoder.read_samples()?;
    }

    Ok(())
}
