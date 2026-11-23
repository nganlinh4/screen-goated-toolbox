use super::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const ROLE: &str = "SGT_TEST_CREATION_LIFECYCLE_ROLE";
const BARRIER: &str = "SGT_TEST_CREATION_SWEEP_BARRIER";

fn wait_for(path: &Path) {
    let until = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(
            Instant::now() < until,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn spawn(name: &str, role: &str, root: &Path) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(name)
        .arg("--nocapture")
        .env(crate::paths::RUNTIME_STATE_ROOT_ENV, root)
        .env(ROLE, role);
    if role == "sweep" {
        command.env(BARRIER, root);
    }
    if role == "submit" {
        command.env("SGT_TEST_INTENT_PUBLICATION_UNCERTAIN", "1");
    }
    command.spawn().unwrap()
}

struct ReleaseOnDrop(PathBuf);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"release");
    }
}

#[test]
fn pressure_sweep_cannot_delete_a_concurrently_accepted_snapshot() {
    let name = format!(
        "{}::pressure_sweep_cannot_delete_a_concurrently_accepted_snapshot",
        module_path!().split_once("::").unwrap().1
    );
    let role = std::env::var(ROLE).unwrap_or_default();
    if role == "sweep" {
        crate::overlay::creation_source_snapshot::sweep_pressure().unwrap();
        return;
    }
    if role == "submit" {
        let root = PathBuf::from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV).unwrap());
        std::fs::write(root.join("submit-started"), b"started").unwrap();
        let source = root.join("source.png");
        let request = StartJobRequest {
            image_path: source.to_string_lossy().to_string(),
            source_descriptors: Vec::new(),
            output_dir: Some(root.join("output").to_string_lossy().to_string()),
            final_output_dir: None,
            model: "simple".to_string(),
            background_mode: "opaque".to_string(),
            output_name: String::new(),
            dispatch_id: String::new(),
        };
        let error = start_job(request).unwrap_err();
        assert!(error.contains("uncertain"), "{error}");
        assert!(STATE.lock().unwrap().jobs.is_empty());
        crate::overlay::creation_submission_test::assert_retained("svg", &source);
        return;
    }

    let root = std::env::temp_dir().join(format!(
        "sgt-creation-lifecycle-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(root.starts_with(&temp) && root != temp);
    image::RgbaImage::new(2, 2)
        .save(root.join("source.png"))
        .unwrap();
    let release = root.join("sweep-release");
    let _release_on_drop = ReleaseOnDrop(release.clone());
    let mut sweeper = spawn(&name, "sweep", &root);
    wait_for(&root.join("sweep-ready"));
    let mut submitter = spawn(&name, "submit", &root);
    wait_for(&root.join("submit-started"));
    let until = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < until {
        assert!(
            submitter.try_wait().unwrap().is_none(),
            "submission ran while sweep held a stale protected list"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(&release, b"release").unwrap();
    assert!(sweeper.wait().unwrap().success());
    assert!(submitter.wait().unwrap().success());
    std::fs::remove_dir_all(root).unwrap();
}
