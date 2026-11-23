use super::*;
use std::cell::{Cell, RefCell};

const SAMPLE_100NS: i64 = 10_000_000 / AUDIO_SAMPLE_RATE as i64;

#[test]
fn final_chunk_is_truncated_at_the_clip_end() {
    // A 1024-frame AAC chunk that starts 10 ms before the clip end only
    // contributes the 480 frames that fit.
    let clip_end = 10_000_000;
    let placement = place_chunk(1024, clip_end - 100_000, clip_end - 100_000, clip_end);
    assert_eq!(
        placement,
        ChunkPlacement {
            skip_frames: 0,
            write_frames: 480
        }
    );
}

#[test]
fn chunk_past_the_clip_end_writes_nothing() {
    let placement = place_chunk(1024, 10_000_000, 10_000_000, 10_000_000);
    assert_eq!(placement.write_frames, 0);
}

#[test]
fn overlap_with_earlier_audio_is_discarded_not_appended_late() {
    // The cursor is 5 ms ahead of where this chunk belongs: its first 240
    // frames are dropped so the rest stays on the clip's timeline.
    let placement = place_chunk(1024, 1_000_000, 1_050_000, 20_000_000);
    assert_eq!(
        placement,
        ChunkPlacement {
            skip_frames: 240,
            write_frames: 784
        }
    );
}

#[test]
fn sub_sample_rounding_never_drops_audio() {
    let placement = place_chunk(1024, 1_000_000, 1_000_000 + SAMPLE_100NS - 1, 20_000_000);
    assert_eq!(placement.skip_frames, 0);
    assert_eq!(placement.write_frames, 1024);
}

fn write_test_clip(path: &std::path::Path, frames: u32, audio_sec: f64) {
    mf_decode::mf_startup().unwrap();
    let (device, _) = create_d3d11_device().unwrap();
    let multithread: ID3D11Multithread = device.cast().unwrap();
    unsafe {
        let _ = multithread.SetMultithreadProtected(true);
    }
    let manager = DxgiDeviceManager::new(&device).unwrap();
    let (encoder, audio) = MfEncoder::new(
        &path.to_string_lossy(),
        EncoderConfig {
            codec: VideoCodec::H264,
            input_surface_format: VideoInputSurfaceFormat::Argb32,
            width: 320,
            height: 240,
            fps_num: 30,
            fps_den: 1,
            bitrate_kbps: 200,
        },
        &manager,
        Some(&AudioConfig {
            sample_rate: AUDIO_SAMPLE_RATE,
            channels: AUDIO_CHANNELS,
            bitrate_kbps: AUDIO_BITRATE_KBPS,
        }),
    )
    .unwrap();
    let audio = audio.unwrap();
    let frame_100ns = encoder.frame_duration_100ns();
    let pixels = vec![128u8; 320 * 240 * 4];
    let mut audio_cursor = 0i64;
    let audio_end = (audio_sec * 10_000_000.0) as i64;
    let tone: Vec<u8> = (0..1024 * AUDIO_CHANNELS as usize)
        .flat_map(|i| ((i as f32 * 0.05).sin() * 0.3).to_le_bytes())
        .collect();
    for frame in 0..frames {
        let timestamp = frame as i64 * frame_100ns;
        while audio_cursor <= timestamp + frame_100ns && audio_cursor < audio_end {
            write_audio_chunk(&audio, &encoder, &tone, &mut audio_cursor).unwrap();
        }
        encoder
            .write_frame_cpu(&pixels, timestamp, frame_100ns)
            .unwrap();
    }
    while audio_cursor < audio_end {
        write_audio_chunk(&audio, &encoder, &tone, &mut audio_cursor).unwrap();
    }
    encoder.finalize().unwrap();
}

struct TestDir(std::path::PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn test_dir() -> TestDir {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "sgt-native-stitch-{}-{}",
        std::process::id(),
        u64::from_ne_bytes(random)
    ));
    std::fs::create_dir_all(&path).unwrap();
    TestDir(path)
}

#[test]
#[ignore = "requires Media Foundation H.264/AAC encoding on a D3D11 device"]
fn stitch_reports_progress_and_stops_on_cancel() {
    let dir = test_dir();
    let clip_path = dir.0.join("clip.mp4");
    write_test_clip(&clip_path, 60, 2.1);
    let clips = [
        StitchClip {
            path: &clip_path,
            trim_start_sec: 0.0,
            duration_sec: 2.0,
        },
        StitchClip {
            path: &clip_path,
            trim_start_sec: 0.0,
            duration_sec: 2.0,
        },
    ];
    let config = StitchConfig {
        width: 320,
        height: 240,
        framerate: 30,
        bitrate_kbps: 200,
    };

    let reported = RefCell::new(Vec::new());
    let progress = |fraction: f64| reported.borrow_mut().push(fraction);
    stitch_clips_to_mp4(
        &clips,
        &dir.0.join("whole.mp4"),
        &config,
        &StitchObserver {
            cancelled: &|| false,
            progress: &progress,
        },
    )
    .unwrap();
    let reported = reported.into_inner();
    assert!(reported.len() >= 4, "{reported:?}");
    assert!(reported.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(reported.last().copied(), Some(1.0));

    let polls = Cell::new(0u32);
    let cancelled = || {
        polls.set(polls.get() + 1);
        polls.get() > 20
    };
    let error = stitch_clips_to_mp4(
        &clips,
        &dir.0.join("cancelled.mp4"),
        &config,
        &StitchObserver {
            cancelled: &cancelled,
            progress: &|_| {},
        },
    )
    .unwrap_err();
    assert_eq!(error, STITCH_CANCELLED);
    assert_eq!(polls.get(), 21, "cancel must stop the frame loop at once");
}
