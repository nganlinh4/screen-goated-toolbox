use super::*;

#[test]
fn silent_or_missing_clip_keeps_its_duration_before_audible_clip() {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "sgt-audio-download-timeline-{}-{}",
        std::process::id(),
        u64::from_ne_bytes(random)
    ));
    std::fs::create_dir(&dir).unwrap();
    let silent = dir.join("silent.wav");
    let audible = dir.join("audible.wav");
    write_silent_wav(&silent, 1.0).unwrap();
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&audible, spec).unwrap();
    for _ in 0..96_000 {
        writer.write_sample(12_000i16).unwrap();
    }
    writer.finalize().unwrap();
    let clip: AudioDownloadClipJob = serde_json::from_value(json!({
        "clipId":"clip", "clipName":"Clip", "trimStart":0.0, "duration":1.0,
        "segment":{"crop":null,"cursorVisibilitySegments":null,
            "trimSegments":[{"startTime":0.0,"endTime":1.0}]}
    }))
    .unwrap();
    let source = |path: &Path| ExportAudioSource {
        path: path.to_string_lossy().into_owned(),
        volume_points: vec![],
        start_offset_sec: 0.0,
        source_in_sec: Some(0.0),
        source_out_sec: Some(1.0),
        playback_rate: 1.0,
        implicit_edge_fade_sec: 0.0,
    };
    for (index, first) in [silent.clone(), dir.join("missing.wav")].iter().enumerate() {
        let (first_wav, first_audible) = render_clip_audio(
            &[source(first)],
            &clip,
            &[],
            &dir,
            &format!("first-{index}"),
        )
        .unwrap()
        .unwrap();
        let (second_wav, second_audible) = render_clip_audio(
            &[source(&audible)],
            &clip,
            &[],
            &dir,
            &format!("second-{index}"),
        )
        .unwrap()
        .unwrap();
        assert!(!first_audible);
        assert!(second_audible);
        let merged = dir.join(format!("merged-{index}.wav"));
        concat_wavs(&[first_wav, second_wav], &merged).unwrap();
        let mut reader = hound::WavReader::open(&merged).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 192_000);
        assert_eq!(samples[48_000], 0);
        assert!(samples[144_000].abs() > 100);
    }
    for entry in std::fs::read_dir(&dir).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn device_track_without_device_audio_contributes_no_source() {
    let config: AudioDownloadConfig = serde_json::from_value(json!({
        "trackKind": "device", "format": "wav", "clips": []
    }))
    .unwrap();
    let clip: AudioDownloadClipJob = serde_json::from_value(json!({
        "clipId":"clip", "clipName":"Clip", "trimStart":0.0, "duration":1.0,
        "sourceVideoPath": "video-without-audio.mp4", "deviceAudioPath": "",
        "segment":{"crop":null,"cursorVisibilitySegments":null,
            "trimSegments":[{"startTime":0.0,"endTime":1.0}]}
    }))
    .unwrap();
    assert!(build_clip_sources(&config, &clip, 0.0).is_empty());

    let with_audio: AudioDownloadClipJob = serde_json::from_value(json!({
        "clipId":"clip", "clipName":"Clip", "trimStart":0.0, "duration":1.0,
        "sourceVideoPath": "video.mp4", "deviceAudioPath": "video.mp4",
        "segment":{"crop":null,"cursorVisibilitySegments":null,
            "trimSegments":[{"startTime":0.0,"endTime":1.0}]}
    }))
    .unwrap();
    let sources = build_clip_sources(&config, &with_audio, 0.0);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].path, "video.mp4");
}
