use crate::overlay::screen_record::audio_engine;
use crate::overlay::screen_record::d3d_interop::{VideoProcessor, create_direct3d_surface};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, ID3D11Multithread,
};
use windows::core::Interface;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows_capture::{
    SendDirectX,
    capture::{Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
};

use super::CaptureHandler;
use super::cursor_sampler::{CaptureFlags, compute_cursor_sample_interval, spawn_cursor_sampler};
use super::encoder_utils::{
    MfEncoderCreateConfig, clone_app_interface_to_wc, clone_wc_interface_to_app,
    compute_window_max_pending_frames, compute_window_vram_pool_frames,
    create_video_encoder_with_canvas_fallback, exact_encoder_canvas, mf_hw_accel_override,
    select_target_fps, should_ignore_window_frame, should_prefer_mf_hw_accel,
};
use super::pump_thread::{resolve_monitor_capture_size, resolve_window_capture_size, spawn_frame_pump};
use super::types::{
    ACTIVE_CAPTURE_CONTROL, AUDIO_ENCODING_FINISHED, AUDIO_PATH,
    ENCODER_ACTIVE, ENCODER_MAX_PENDING_FRAMES, ENCODING_FINISHED,
    LAST_CAPTURE_FRAME_HEIGHT, LAST_CAPTURE_FRAME_WIDTH, LAST_RECORDING_FPS,
    MAX_CATCHUP_SUBMITS_PER_CALLBACK, MIC_AUDIO_ENCODING_FINISHED, MIC_AUDIO_PATH,
    MIC_AUDIO_START_OFFSET_MS, NO_READY_VRAM_FRAME, SHOULD_STOP, SHOULD_STOP_AUDIO,
    TIMESTAMP_RESYNC_THRESHOLD_100NS, VIDEO_PATH, VramFrame, WEBCAM_ENCODING_FINISHED,
    WEBCAM_VIDEO_PATH, WEBCAM_VIDEO_START_OFFSET_MS,
};

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = String;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = serde_json::from_str::<CaptureFlags>(&ctx.flags).unwrap_or_else(|e| {
            // Backward compatibility for legacy plain monitor-id flags.
            eprintln!(
                "[CaptureHandler::new] flags JSON parse failed ({e}), raw={:?}",
                ctx.flags
            );
            CaptureFlags {
                target_type: "monitor".to_string(),
                target_id: ctx.flags.clone(),
                fps: None,
                device_audio_enabled: true,
                device_audio_mode: "all".to_string(),
                device_audio_app_pid: None,
                mic_enabled: false,
                webcam_enabled: true,
            }
        });
        eprintln!(
            "[CaptureHandler::new] target_type={:?}, target_id={:?}",
            flags.target_type, flags.target_id
        );

        let (width, height, monitor_hz, target_id_print) = if flags.target_type == "window" {
            let hwnd_val = flags.target_id.parse::<usize>().unwrap_or(0);
            resolve_window_capture_size(hwnd_val)
        } else {
            let monitor_index = flags.target_id.parse::<usize>().unwrap_or(0);
            resolve_monitor_capture_size(monitor_index)?
        };

        // Prefer the exact even capture size. Some MF encoder/device combinations only
        // accept 16-aligned canvases, so we retry with that fallback on init failure.
        let preferred_canvas = exact_encoder_canvas(width, height);

        let app_data_dir = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("screen-goated-toolbox")
            .join("recordings");

        std::fs::create_dir_all(&app_data_dir)?;

        let video_path = app_data_dir.join(format!(
            "recording_{}.mp4",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let mic_audio_path = video_path.with_file_name(format!(
            "{}_mic.wav",
            video_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("recording")
        ));
        let webcam_video_path = video_path.with_file_name(format!(
            "{}_webcam.mp4",
            video_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("recording")
        ));

        *VIDEO_PATH.lock().unwrap() = Some(video_path.to_string_lossy().to_string());
        *AUDIO_PATH.lock().unwrap() = Some(video_path.to_string_lossy().to_string());
        *MIC_AUDIO_PATH.lock().unwrap() = None;
        *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
        MIC_AUDIO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
        WEBCAM_VIDEO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);

        let target_fps = flags.fps.unwrap_or_else(|| select_target_fps(monitor_hz));
        *LAST_RECORDING_FPS.lock().unwrap() = Some(target_fps);
        LAST_CAPTURE_FRAME_WIDTH.store(width as usize, Ordering::Relaxed);
        LAST_CAPTURE_FRAME_HEIGHT.store(height as usize, Ordering::Relaxed);
        let frame_interval_100ns = 10_000_000 / target_fps as i64;

        // DYNAMIC BITRATE CALCULATION
        // Prior 0.35 bpp target could trigger intermittent MediaFoundation HW encoder
        // backpressure during heavy gameplay. Use a more stable 0.22 bpp target.
        // 1920x1080 @ 60fps = ~27 Mbps
        // 2560x1440 @ 60fps = ~48 Mbps
        // 3840x2160 @ 60fps = ~109 Mbps
        let pixel_count = preferred_canvas.width as u64 * preferred_canvas.height as u64;
        let target_bitrate = (pixel_count as f64 * target_fps as f64 * 0.22) as u32;

        // Keep a quality floor while capping peak encoder pressure.
        let final_bitrate = target_bitrate.clamp(8_000_000, 80_000_000);

        let (sample_rate, channels) = audio_engine::get_default_audio_config();
        let mf_hw_preferred = should_prefer_mf_hw_accel(
            &flags.target_type,
            target_fps,
            preferred_canvas.width,
            preferred_canvas.height,
        );
        let (encoder, canvas, encoder_uses_hw) = match create_video_encoder_with_canvas_fallback(
            MfEncoderCreateConfig {
                enc_w: preferred_canvas.width,
                enc_h: preferred_canvas.height,
                target_fps,
                final_bitrate,
                sample_rate,
                channels,
                video_path: &video_path,
                prefer_hw: mf_hw_preferred,
            },
            width,
            height,
        ) {
            Ok((encoder, canvas)) => (encoder, canvas, mf_hw_preferred),
            Err(error) if mf_hw_preferred && mf_hw_accel_override().is_none() => {
                eprintln!(
                    "[CaptureBackend] HW encoder init failed, retrying software path: {}",
                    error
                );
                let (encoder, canvas) = create_video_encoder_with_canvas_fallback(
                    MfEncoderCreateConfig {
                        enc_w: preferred_canvas.width,
                        enc_h: preferred_canvas.height,
                        target_fps,
                        final_bitrate,
                        sample_rate,
                        channels,
                        video_path: &video_path,
                        prefer_hw: false,
                    },
                    width,
                    height,
                )?;
                (encoder, canvas, false)
            }
            Err(error) => return Err(error),
        };
        let enc_w = canvas.width;
        let enc_h = canvas.height;
        let audio_handle = encoder.create_audio_handle();
        println!(
            "Initializing VideoEncoder: {}x{} @ {}fps (Hz={}), Codec: H264 (MediaFoundation {}), Bitrate: {} Mbps, TargetType: {}, TargetID: {}",
            enc_w,
            enc_h,
            target_fps,
            monitor_hz,
            if encoder_uses_hw { "HW" } else { "SW" },
            final_bitrate / 1_000_000,
            flags.target_type,
            target_id_print
        );

        SHOULD_STOP_AUDIO.store(false, Ordering::SeqCst);
        AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        let device_audio_source = if !flags.device_audio_enabled {
            audio_engine::DeviceAudioCaptureSource::Disabled
        } else if flags.device_audio_mode == "app" {
            flags
                .device_audio_app_pid
                .map(audio_engine::DeviceAudioCaptureSource::SingleApp)
                .unwrap_or(audio_engine::DeviceAudioCaptureSource::SystemOutput)
        } else {
            audio_engine::DeviceAudioCaptureSource::SystemOutput
        };
        let start = Instant::now();
        audio_engine::record_audio(
            audio_handle,
            start,
            SHOULD_STOP_AUDIO.clone(),
            AUDIO_ENCODING_FINISHED.clone(),
            device_audio_source,
        );
        if flags.mic_enabled {
            MIC_AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
            match audio_engine::record_mic_audio_sidecar(
                mic_audio_path.to_string_lossy().to_string(),
                start,
                SHOULD_STOP_AUDIO.clone(),
                MIC_AUDIO_ENCODING_FINISHED.clone(),
                &MIC_AUDIO_START_OFFSET_MS,
            ) {
                Ok(()) => {
                    *MIC_AUDIO_PATH.lock().unwrap() =
                        Some(mic_audio_path.to_string_lossy().to_string());
                }
                Err(error) => {
                    MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                    *MIC_AUDIO_PATH.lock().unwrap() = None;
                    eprintln!("[MicCapture] {}", error);
                }
            }
        } else {
            MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
            *MIC_AUDIO_PATH.lock().unwrap() = None;
        }
        if flags.webcam_enabled {
            WEBCAM_ENCODING_FINISHED.store(false, Ordering::SeqCst);
            match super::super::webcam_capture::record_webcam_video_sidecar(
                webcam_video_path.to_string_lossy().to_string(),
                start,
                SHOULD_STOP_AUDIO.clone(),
                WEBCAM_ENCODING_FINISHED.clone(),
                &WEBCAM_VIDEO_START_OFFSET_MS,
            ) {
                Ok(()) => {
                    *WEBCAM_VIDEO_PATH.lock().unwrap() =
                        Some(webcam_video_path.to_string_lossy().to_string());
                }
                Err(error) => {
                    WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                    *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
                    eprintln!("[WebcamCapture] {}", error);
                }
            }
        } else {
            WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
            *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
        }

        ENCODER_ACTIVE.store(true, Ordering::SeqCst);
        ENCODING_FINISHED.store(false, Ordering::SeqCst);
        let cursor_sampler_stop = Arc::new(AtomicBool::new(false));

        let is_window_capture = flags.target_type == "window";
        let app_d3d_device: windows::Win32::Graphics::Direct3D11::ID3D11Device =
            clone_wc_interface_to_app(&ctx.device)
                .map_err(|e| format!("Failed to bridge capture D3D11 device: {e}"))?;
        // Enable D3D11 multithread protection to prevent CopyResource races.
        let mt: ID3D11Multithread = app_d3d_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (capture): {e}"))?;
        unsafe { let _ = mt.SetMultithreadProtected(true); }
        let app_d3d_context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext =
            clone_wc_interface_to_app(&ctx.device_context)
                .map_err(|e| format!("Failed to bridge capture D3D11 context: {e}"))?;
        let max_pending_frames = if is_window_capture {
            compute_window_max_pending_frames(target_fps)
        } else {
            ENCODER_MAX_PENDING_FRAMES
        };
        let window_vram_pool_frames = if is_window_capture {
            compute_window_vram_pool_frames(max_pending_frames)
        } else {
            3
        };
        let mut vram_frames = Vec::with_capacity(window_vram_pool_frames);
        for _ in 0..window_vram_pool_frames {
            let texture = VideoProcessor::create_texture(
                &app_d3d_device,
                enc_w,
                enc_h,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
            )
            .map_err(|e| format!("Failed to create VRAM ring texture: {e}"))?;
            let surface = create_direct3d_surface(&texture)
                .map_err(|e| format!("Failed to create WinRT surface for VRAM ring: {e}"))?;
            let surface = clone_app_interface_to_wc(&surface)
                .map_err(|e| format!("Failed to bridge WinRT surface to encoder type: {e}"))?;
            vram_frames.push(VramFrame {
                texture: SendDirectX::new(texture),
                surface: SendDirectX::new(surface),
                in_flight: Arc::new(AtomicUsize::new(0)),
            });
        }
        let vram_pool = Arc::new(vram_frames);
        let latest_ready_idx = Arc::new(AtomicUsize::new(NO_READY_VRAM_FRAME));
        let video_processor = if width != enc_w || height != enc_h {
            match VideoProcessor::new_with_frame_rate(
                &app_d3d_device,
                &app_d3d_context,
                width,
                height,
                enc_w,
                enc_h,
                target_fps,
            ) {
                Ok(vp) => Some((width, height, vp)),
                Err(e) => {
                    eprintln!(
                        "[CaptureHandler] GPU resize path unavailable for {}x{} -> {}x{}: {}",
                        width, height, enc_w, enc_h, e
                    );
                    None
                }
            }
        } else {
            None
        };
        let pump_stop = Arc::new(AtomicBool::new(false));
        let pump_submitted = Arc::new(AtomicUsize::new(0));
        let pump_dropped = Arc::new(AtomicUsize::new(0));

        let pump = encoder.create_frame_pump();
        let cursor_sample_interval = compute_cursor_sample_interval(target_fps);
        let cursor_sampler_thread = Some(spawn_cursor_sampler(
            start,
            cursor_sampler_stop.clone(),
            cursor_sample_interval,
        ));
        let encoder_shared = Arc::new(Mutex::new(Some(encoder)));

        // For window capture, spawn a pump thread that submits the cached
        // frame at constant FPS.  WGC only delivers frames when the window
        // content changes, which can be <1 fps for a static window.
        if is_window_capture {
            spawn_frame_pump(
                vram_pool.clone(),
                latest_ready_idx.clone(),
                pump_stop.clone(),
                pump_submitted.clone(),
                pump_dropped.clone(),
                encoder_shared.clone(),
                frame_interval_100ns,
                max_pending_frames,
                start,
                pump,
            );
        }

        Ok(Self {
            encoder: encoder_shared,
            target_fps,
            frame_interval_100ns,
            start,
            cursor_sampler_stop,
            cursor_sampler_thread,
            next_submit_timestamp_100ns: Some(0), // Anchor exactly to start time
            last_pending_frames: 0,
            frame_count: 0,
            window_arrivals: 0,
            window_enqueued: 0,
            window_dropped: 0,
            window_paced_skips: 0,
            stats_window_start: Instant::now(),
            enc_w,
            enc_h,
            is_window_capture,
            vram_pool,
            latest_ready_idx,
            write_idx: 0,
            video_processor,
            d3d_device: app_d3d_device,
            d3d_context: app_d3d_context,
            pump_stop,
            pump_submitted,
            pump_dropped,
            max_pending_frames,
            last_ignored_window_frame: None,
            vram_pool_exhausted_logged: false,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        *ACTIVE_CAPTURE_CONTROL.lock() = Some(capture_control.clone());

        if !ENCODER_ACTIVE.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut queue_depth = 0usize;
        let mut dropped_total = 0usize;

        if self.is_window_capture {
            // Window capture: stage latest frame into VRAM ring; pump thread
            // submits at constant target_fps.
            let frame_w = frame.width();
            let frame_h = frame.height();
            if should_ignore_window_frame(frame_w, frame_h) {
                if self.last_ignored_window_frame != Some((frame_w, frame_h)) {
                    eprintln!(
                        "[FramePump] ignoring implausible window frame {}x{}; keeping last good frame",
                        frame_w, frame_h
                    );
                    self.last_ignored_window_frame = Some((frame_w, frame_h));
                }
            } else {
                self.last_ignored_window_frame = None;
                LAST_CAPTURE_FRAME_WIDTH.store(frame_w as usize, Ordering::Relaxed);
                LAST_CAPTURE_FRAME_HEIGHT.store(frame_h as usize, Ordering::Relaxed);
                let was_empty =
                    self.latest_ready_idx.load(Ordering::Acquire) == NO_READY_VRAM_FRAME;
                match self.stage_frame_in_vram(frame) {
                    Ok(Some(slot)) => {
                        self.vram_pool_exhausted_logged = false;
                        self.latest_ready_idx.store(slot, Ordering::Release);
                        self.window_enqueued = self.window_enqueued.saturating_add(1);
                        if was_empty {
                            eprintln!(
                                "[FramePump] first frame staged in VRAM: frame={}x{} enc={}x{}",
                                frame_w, frame_h, self.enc_w, self.enc_h
                            );
                        }
                    }
                    Ok(None) => {
                        if !self.vram_pool_exhausted_logged {
                            eprintln!(
                                "[FramePump] all staged surfaces still in flight; keeping last good frame until encoder drains"
                            );
                            self.vram_pool_exhausted_logged = true;
                        }
                    }
                    Err(e) => {
                        eprintln!("[FramePump] VRAM stage failed: {}", e);
                    }
                }
            }

            if let Some(encoder) = self.encoder.lock().as_ref() {
                queue_depth = encoder.pending_video_frames();
                dropped_total = encoder.dropped_video_frames();
            }
        } else {
            // Display capture: submit directly to encoder with pacing/catch-up.
            let now_100ns = (self.start.elapsed().as_nanos() / 100) as i64;
            let mut should_submit = false;
            let mut frames_to_submit = 0u32;

            let mut due_100ns = self.next_submit_timestamp_100ns.unwrap_or(0);

            if now_100ns.saturating_add(TIMESTAMP_RESYNC_THRESHOLD_100NS) < due_100ns {
                due_100ns = now_100ns;
            }

            if now_100ns >= due_100ns {
                let due_ticks = ((now_100ns.saturating_sub(due_100ns)) / self.frame_interval_100ns)
                    .saturating_add(1);
                let missed_ticks = due_ticks.saturating_sub(1) as u32;
                frames_to_submit = due_ticks as u32;
                self.window_paced_skips = self.window_paced_skips.saturating_add(missed_ticks);
                self.next_submit_timestamp_100ns = Some(
                    due_100ns.saturating_add(self.frame_interval_100ns.saturating_mul(due_ticks)),
                );
                should_submit = true;
            } else {
                self.window_paced_skips = self.window_paced_skips.saturating_add(1);
                self.next_submit_timestamp_100ns = Some(due_100ns);
            }

            if should_submit {
                let frame_w = frame.width();
                let frame_h = frame.height();
                let staged_mismatch_slot = if frame_w != self.enc_w || frame_h != self.enc_h {
                    match self.stage_frame_in_vram(frame) {
                        Ok(Some(slot)) => Some(slot),
                        Ok(None) => {
                            eprintln!(
                                "Encoder GPU resize fallback skipped: no free staged surface for {}x{} -> {}x{}",
                                frame_w, frame_h, self.enc_w, self.enc_h
                            );
                            None
                        }
                        Err(e) => {
                            eprintln!(
                                "Encoder GPU resize fallback error ({}x{} -> {}x{}): {}",
                                frame_w, frame_h, self.enc_w, self.enc_h, e
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                let mut encoder_guard = self.encoder.lock();
                if let Some(encoder) = encoder_guard.as_mut() {
                    let mut remaining = frames_to_submit.max(1);
                    let mut submitted = 0u32;
                    while remaining > 0 {
                        if submitted >= MAX_CATCHUP_SUBMITS_PER_CALLBACK {
                            encoder.skip_video_frames(remaining);
                            self.window_dropped = self.window_dropped.saturating_add(remaining);
                            break;
                        }

                        if frame_w == self.enc_w && frame_h == self.enc_h {
                            match encoder.send_frame_nonblocking(frame, ENCODER_MAX_PENDING_FRAMES)
                            {
                                Ok(true) => {
                                    self.window_enqueued = self.window_enqueued.saturating_add(1);
                                    submitted = submitted.saturating_add(1);
                                    remaining -= 1;
                                }
                                Ok(false) => {
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Encoder error: {}", e);
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                            }
                        } else {
                            let Some(slot) = staged_mismatch_slot else {
                                encoder.skip_video_frames(remaining);
                                self.window_dropped = self.window_dropped.saturating_add(remaining);
                                break;
                            };

                            let surface = SendDirectX::new(self.vram_pool[slot].surface.0.clone());
                            match encoder.send_directx_surface_nonblocking(
                                surface,
                                self.max_pending_frames,
                                Some(self.vram_pool[slot].in_flight.clone()),
                            ) {
                                Ok(true) => {
                                    self.window_enqueued = self.window_enqueued.saturating_add(1);
                                    submitted = submitted.saturating_add(1);
                                    remaining -= 1;
                                }
                                Ok(false) => {
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Encoder GPU resize submit error: {}", e);
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                            }
                        }
                    }
                    queue_depth = encoder.pending_video_frames();
                    dropped_total = encoder.dropped_video_frames();
                }
            } else if let Some(encoder) = self.encoder.lock().as_ref() {
                queue_depth = encoder.pending_video_frames();
                dropped_total = encoder.dropped_video_frames();
            }
        }

        self.report_capture_stats(queue_depth, dropped_total);

        if SHOULD_STOP.load(Ordering::SeqCst) {
            self.shutdown_and_finalize();
            capture_control.stop();
        }

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.shutdown_and_finalize();
        Ok(())
    }
}
