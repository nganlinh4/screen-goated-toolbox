use super::*;

#[test]
fn failed_supervision_preserves_existing_output_and_segments_without_stitching() {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let nonce = u128::from_le_bytes(nonce);
    let root = std::env::temp_dir().join(format!(
        "sgt-capture-stop-{}-{nonce:032x}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let final_video = root.join("final.mp4");
    let segment = root.join("segment.mp4");
    std::fs::write(&final_video, b"existing output").unwrap();
    std::fs::write(&segment, b"recorded segment").unwrap();
    let mut supervisor = CaptureSupervisor {
        stop: Arc::new(AtomicBool::new(false)),
        thread: Some(std::thread::spawn(|| {
            anyhow::bail!("process retirement pending")
        })),
        ffmpeg: super::super::process::test_executable(),
        final_video: final_video.clone(),
        started_at: Instant::now(),
    };
    assert!(
        supervisor
            .stop(Duration::from_secs(1))
            .unwrap_err()
            .to_string()
            .contains("retirement pending")
    );
    drop(supervisor);
    assert_eq!(std::fs::read(&final_video).unwrap(), b"existing output");
    assert_eq!(std::fs::read(&segment).unwrap(), b"recorded segment");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_file(final_video).unwrap();
    std::fs::remove_file(segment).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn repeated_quick_exits_fall_back_to_software_at_any_time() {
    let config = CaptureProcessConfig {
        monitor_index: 0,
        fps: 30,
        include_cursor: true,
        width: 1920,
        height: 1080,
        bitrate: 8_000_000,
    };
    let mut failover = EncoderFailover::new(CaptureEncoder::MediaFoundation);
    // Long healthy segments never trigger the fallback.
    for _ in 0..10 {
        failover.record_exit(Duration::from_secs(600), &config);
    }
    assert_eq!(failover.encoder(), CaptureEncoder::MediaFoundation);
    // A driver reset half an hour in: every respawn dies at once.
    failover.record_exit(Duration::from_millis(600), &config);
    failover.record_exit(Duration::ZERO, &config);
    assert_eq!(failover.encoder(), CaptureEncoder::MediaFoundation);
    failover.record_exit(Duration::from_millis(500), &config);
    assert_eq!(failover.encoder(), CaptureEncoder::Software);
    for _ in 0..5 {
        failover.record_exit(Duration::ZERO, &config);
    }
    assert_eq!(failover.encoder(), CaptureEncoder::Software);
}

#[test]
fn missing_initial_segment_cannot_shift_recording_origin() {
    let segments = [CompletedSegment {
        path: PathBuf::from("unused.mp4"),
        started_offset: Duration::from_secs(2),
        encoded_duration: Duration::from_secs(1),
    }];
    let error = validate_segments(&segments).unwrap_err();
    assert!(error.to_string().contains("lost its initial video segment"));
}

#[test]
fn segment_paths_are_session_owned() {
    let final_path = Path::new(r"C:\recordings\recording_1_compat.mp4");
    assert_eq!(
        segment_path(final_path, 7),
        PathBuf::from(r"C:\recordings\recording_1_compat.part007.mp4")
    );
    assert_eq!(
        concat_list_path(final_path),
        PathBuf::from(r"C:\recordings\recording_1_compat.concat.txt")
    );
}

#[test]
fn concat_manifest_normalizes_windows_paths() {
    let segments = [
        CompletedSegment {
            path: PathBuf::from(r"C:\recordings\one.mp4"),
            started_offset: Duration::ZERO,
            encoded_duration: Duration::from_millis(500),
        },
        CompletedSegment {
            path: PathBuf::from(r"C:\recordings\two.mp4"),
            started_offset: Duration::from_secs(1),
            encoded_duration: Duration::from_millis(500),
        },
    ];
    let manifest = concat_manifest(&segments, Duration::from_secs(2));
    assert_eq!(
        manifest,
        "file 'C:/recordings/one.mp4'\nduration 1.000000\n\
         file 'C:/recordings/two.mp4'\nduration 1.000000\n"
    );
}
