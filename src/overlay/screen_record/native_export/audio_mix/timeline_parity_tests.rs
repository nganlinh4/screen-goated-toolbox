//! The exported picture and the mixed audio walk the same timeline from two
//! sides: the video by stepping output frames through the speed curve, the
//! audio by mapping source time to output time. These tests pin them together.

use super::super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};
use super::calculate_mix_output_duration;
use super::time_map::{OutputTimeMapper, get_audio_volume};
use crate::overlay::screen_record::gpu_pipeline::frame_times;

fn segment(start_time: f64, end_time: f64) -> TrimSegment {
    TrimSegment {
        start_time,
        end_time,
    }
}

fn speed(time: f64, speed: f64) -> SpeedPoint {
    SpeedPoint { time, speed }
}

fn assert_frames_match_audio(fps: u32, segments: &[TrimSegment], speeds: &[SpeedPoint]) {
    let frames = frame_times(fps, 0.0, 0.0, segments, speeds);
    let audio = calculate_mix_output_duration(0.0, 0.0, segments, speeds);
    let video = frames.len() as f64 / fps as f64;
    // One frame per kept segment of rounding is the most the two can disagree.
    let tolerance = segments.len() as f64 / fps as f64 + 1e-6;
    assert!(
        (video - audio).abs() <= tolerance,
        "video {video:.4}s vs audio {audio:.4}s for {} segments",
        segments.len()
    );
    assert!(frames.windows(2).all(|pair| pair[1] > pair[0]));
    assert!(frames.iter().all(|time| {
        segments
            .iter()
            .any(|kept| *time >= kept.start_time - 1e-9 && *time < kept.end_time)
    }));
}

#[test]
fn frame_count_matches_audio_duration_across_cuts_and_speeds() {
    let cuts = [segment(0.0, 2.0), segment(3.5, 5.0), segment(8.0, 9.25)];
    assert_frames_match_audio(30, &cuts, &[]);
    assert_frames_match_audio(60, &cuts, &[speed(0.0, 2.0)]);
    assert_frames_match_audio(30, &cuts, &[speed(0.0, 0.5), speed(4.0, 0.5)]);
    assert_frames_match_audio(
        24,
        &[segment(1.0, 6.0)],
        &[speed(1.0, 1.0), speed(3.0, 3.0), speed(6.0, 1.0)],
    );
}

#[test]
fn frames_skip_cut_regions_and_start_on_each_kept_segment() {
    let cuts = [segment(0.0, 1.0), segment(5.0, 6.0)];
    // 8 fps keeps every step exact in binary floating point.
    let frames = frame_times(8, 0.0, 0.0, &cuts, &[]);
    assert_eq!(frames.len(), 16);
    assert_eq!(frames[0], 0.0);
    assert_eq!(frames[8], 5.0);
    assert!(!frames.iter().any(|time| (1.0..5.0).contains(time)));
}

#[test]
fn volume_envelope_eases_between_points_and_holds_at_the_ends() {
    let points = [
        DeviceAudioPoint {
            time: 1.0,
            volume: 0.2,
        },
        DeviceAudioPoint {
            time: 3.0,
            volume: 1.0,
        },
    ];
    assert_eq!(get_audio_volume(0.0, &points), 0.2);
    assert_eq!(get_audio_volume(9.0, &points), 1.0);
    assert!((get_audio_volume(2.0, &points) - 0.6).abs() < 1e-9);
    assert!(
        get_audio_volume(1.5, &points) < 0.4,
        "cosine ease starts slowly"
    );
    assert_eq!(get_audio_volume(5.0, &[]), 1.0);
}

#[test]
fn frames_stay_on_the_audio_timeline_across_many_fractional_cuts() {
    // Fifteen kept segments that are each a fraction of a frame longer than
    // a whole number of frames, at plain and doubled speed.
    let cuts: Vec<TrimSegment> = (0..15)
        .map(|index| segment(index as f64 * 2.0, index as f64 * 2.0 + 1.01))
        .collect();
    for speeds in [vec![], vec![speed(0.0, 2.0)]] {
        let fps = 30;
        let frames = frame_times(fps, 0.0, 0.0, &cuts, &speeds);
        let audio = calculate_mix_output_duration(0.0, 0.0, &cuts, &speeds);
        let video = frames.len() as f64 / fps as f64;
        assert!(
            (video - audio).abs() <= 1.0 / fps as f64 + 1e-6,
            "video {video:.4}s vs audio {audio:.4}s: cuts must not add a frame each"
        );
        // Every frame shows the source instant the audio plays at that moment.
        let mut mapper = OutputTimeMapper::new(cuts.clone(), speeds.clone());
        for (index, &time) in frames.iter().enumerate() {
            let audio_at = mapper.map_source_time(time).unwrap();
            let frame_at = index as f64 / fps as f64;
            assert!(
                (audio_at - frame_at).abs() < 1e-3,
                "frame {index} at {frame_at:.4}s shows audio time {audio_at:.4}s"
            );
        }
    }
}

#[test]
fn overlay_frames_index_the_same_output_frames_as_the_video() {
    use crate::overlay::screen_record::native_export::overlay_frames::{
        OverlayAtlasMetadata, generate_overlay_frames,
    };
    let meta: OverlayAtlasMetadata =
        serde_json::from_value(serde_json::json!({ "atlasWidth": 1, "atlasHeight": 1 })).unwrap();
    let cuts: Vec<TrimSegment> = (0..7)
        .map(|index| segment(index as f64 * 3.0, index as f64 * 3.0 + 1.37))
        .collect();
    let speeds = [speed(0.0, 1.0), speed(10.0, 2.5)];
    let overlays = generate_overlay_frames(&meta, &cuts, &speeds, 30, 1920.0, 1080.0);
    assert_eq!(
        overlays.len(),
        frame_times(30, 0.0, 0.0, &cuts, &speeds).len()
    );
}
