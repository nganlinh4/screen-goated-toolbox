//! Isolated tests for uncertain Creation publication outcomes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD: &str = "SGT_TEST_CREATION_SUBMISSION_CHILD";
const INJECT_UNCERTAIN: &str = "SGT_TEST_INTENT_PUBLICATION_UNCERTAIN";
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

pub(crate) mod queued;

pub(super) fn run_isolated(test_name: &str, child: impl FnOnce(&Path)) {
    run_with_publication(test_name, true, child);
}

pub(super) fn run_isolated_without_fault(test_name: &str, child: impl FnOnce(&Path)) {
    run_with_publication(test_name, false, child);
}

fn run_with_publication(test_name: &str, uncertain: bool, child: impl FnOnce(&Path)) {
    if std::env::var_os(CHILD).is_some() {
        let root = PathBuf::from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV).unwrap());
        child(&root);
        return;
    }
    let root = reserve_root().unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(root.starts_with(&temp) && root != temp);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(crate::paths::RUNTIME_STATE_ROOT_ENV, &root)
        .env(CHILD, "1");
    if uncertain {
        command.env(INJECT_UNCERTAIN, "1");
    } else {
        command.env_remove(INJECT_UNCERTAIN);
    }
    let result = command.output().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        result.status.success(),
        "isolated Creation test failed: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn reserve_root() -> std::io::Result<PathBuf> {
    let prefix = format!(
        "sgt-creation-uncertain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    reserve_root_with(|| {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

fn reserve_root_with(mut candidate: impl FnMut() -> PathBuf) -> std::io::Result<PathBuf> {
    for _ in 0..64 {
        let root = candidate();
        match std::fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a unique Creation fixture directory",
    ))
}

pub(super) fn input_image(root: &Path) -> PathBuf {
    let input = root.join("source.png");
    image::RgbaImage::new(2, 2).save(&input).unwrap();
    input
}

pub(super) fn assert_retained(product: &str, source: &Path) {
    let local = std::fs::canonicalize(crate::paths::app_runtime_local_data_dir()).unwrap();
    let path = local.join("active-creation-intents.json");
    let journal: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let intents = journal["entries"].as_array().unwrap();
    assert_eq!(intents.len(), 1);
    let intent = &intents[0];
    assert_eq!(intent["product"], product);
    let arguments = &intent["arguments"];
    let descriptor = &arguments["sourceDescriptors"][0];
    let copied = Path::new(descriptor["path"].as_str().unwrap());
    assert!(copied.starts_with(local.join("creation-source-snapshots")));
    assert_eq!(
        std::fs::read(copied).unwrap(),
        std::fs::read(source).unwrap()
    );
    let staging = Path::new(arguments["outputDir"].as_str().unwrap());
    assert!(staging.starts_with(local.join("creation-staging")));
    assert!(staging.join(".sgt-staging.json").is_file());
}

#[test]
fn fixture_reservation_preserves_collisions_and_bounds_other_errors() -> anyhow::Result<()> {
    let parent = reserve_root()?;
    let directory = parent.join("existing-directory");
    let file = parent.join("existing-file");
    std::fs::create_dir(&directory)?;
    std::fs::write(directory.join("sentinel"), b"keep directory")?;
    std::fs::write(&file, b"keep file")?;
    let reserved = parent.join("reserved");
    let mut candidates = [directory.clone(), file.clone(), reserved.clone()].into_iter();
    assert_eq!(reserve_root_with(|| candidates.next().unwrap())?, reserved);
    assert_eq!(
        std::fs::read(directory.join("sentinel"))?,
        b"keep directory"
    );
    assert_eq!(std::fs::read(&file)?, b"keep file");

    let mut calls = 0;
    let error = reserve_root_with(|| {
        calls += 1;
        parent.join("missing-parent").join("child")
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);

    let mut calls = 0;
    let error = reserve_root_with(|| {
        calls += 1;
        directory.clone()
    })
    .unwrap_err();
    assert_eq!(calls, 64);
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    let temp = std::fs::canonicalize(std::env::temp_dir())?;
    let parent = std::fs::canonicalize(parent)?;
    assert!(parent.starts_with(&temp) && parent != temp);
    std::fs::remove_dir_all(parent)?;
    Ok(())
}

#[test]
fn concurrent_fixture_reservations_have_distinct_owned_directories() -> anyhow::Result<()> {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
    let roots = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    reserve_root()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<std::io::Result<Vec<_>>>()
    })?;
    assert_eq!(
        roots.iter().collect::<std::collections::HashSet<_>>().len(),
        16
    );
    let temp = std::fs::canonicalize(std::env::temp_dir())?;
    for root in roots {
        let root = std::fs::canonicalize(root)?;
        assert!(root.starts_with(&temp) && root != temp);
        assert_eq!(std::fs::read_dir(&root)?.count(), 0);
        std::fs::remove_dir(root)?;
    }
    Ok(())
}
