#[test]
fn prewarm_runs_off_the_caller_thread_and_only_once() {
    let source = include_str!("setup.rs");
    let start = source
        .find("pub(crate) fn prewarm_shared_gpu_context()")
        .unwrap();
    let body = &source[start..];
    assert!(
        body.contains("STARTED.call_once("),
        "repeated window shows must not spawn repeated prewarm threads"
    );
    assert!(
        body.find("thread::Builder::new()").unwrap()
            < body.find("match shared_gpu_context(None)").unwrap(),
        "the pipeline build must happen on the spawned thread, not the caller"
    );
}

#[test]
fn showing_the_window_starts_the_prewarm() {
    let source = include_str!("../window_proc.rs");
    let start = source.find("WM_APP_SHOW =>").unwrap();
    let end = source.find("WM_APP_UPDATE_SETTINGS =>").unwrap();
    assert!(
        source[start..end].contains("prewarm_shared_gpu_context()"),
        "the export pipeline must start building as soon as the window is shown"
    );
}

#[test]
#[ignore = "builds the real GPU pipeline"]
fn prewarm_absorbs_the_pipeline_build_before_export_asks_for_it() {
    super::prewarm_shared_gpu_context();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !super::SHARED_GPU_CONTEXT.is_ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "prewarm did not finish building the shared context"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let started = std::time::Instant::now();
    assert!(super::shared_gpu_context(None).is_ok());
    assert!(
        started.elapsed() < std::time::Duration::from_millis(50),
        "export acquired the prewarmed context in {:?}, expected it to be ready",
        started.elapsed()
    );
}
