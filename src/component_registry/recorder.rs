//! Verified delivery and launch leases for the external Screen Recorder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Result, bail};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    resolve_owned_path,
};
use super::{ComponentLease, RemovalOutcome};

mod install;
mod recovery;
mod staging;
mod update;
mod verification;

pub(crate) const BUNDLE_ID: &str = "screen-recorder";
pub(crate) const WEB_ID: &str = "recorder-web";
pub(crate) const WORKER_ID: &str = "recorder-worker";
const ARCHITECTURE: &str = "x64";
const WORKER_PATH: &str = "bin/x64/sgt-recorder-worker.exe";
const BUNDLE_WEB_PATH: &str = "web";
const MAX_COMPONENT_FILES: usize = 512;
const MAX_VERIFICATION_WORKERS: usize = 8;

struct RecorderFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct RecorderDelivery {
    id: &'static str,
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [RecorderFile],
}

include!(concat!(env!("OUT_DIR"), "/recorder_delivery.rs"));

pub(crate) struct RecorderComponents {
    pub(crate) web_root: PathBuf,
    pub(crate) worker_path: PathBuf,
    _leases: Vec<ComponentLease>,
    _files: Vec<std::fs::File>,
}

pub(crate) fn ensure_ready(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<RecorderComponents> {
    let _mutation = super::acquire_mutation_guard()?;
    let deliveries = delivery_group()?;

    // A healthy install is the normal case, and acquisition already proves every
    // owned byte. Probing it first keeps the open path to a single pass over the
    // inventories instead of restating them before hashing them.
    if let Ok(components) = acquire_verified_components(&deliveries) {
        return Ok(components);
    }

    install::ensure_group(&deliveries, cancelled, on_progress)?;
    match acquire_verified_components(&deliveries) {
        Ok(components) => Ok(components),
        Err(first_error) => {
            install::repair_group(&deliveries, cancelled, |_, _| {})?;
            acquire_verified_components(&deliveries).map_err(|retry_error| {
                anyhow::anyhow!(
                    "recorder verification failed before and after repair: {first_error:#}; {retry_error:#}"
                )
            })
        }
    }
}

fn acquire_verified_components(
    deliveries: &[&'static RecorderDelivery],
) -> Result<RecorderComponents> {
    let mut leases = Vec::with_capacity(deliveries.len());
    for delivery in deliveries {
        leases.push(super::acquire(delivery.id)?);
    }
    let mut files = Vec::new();
    let mut roots = Vec::with_capacity(deliveries.len());
    for delivery in deliveries {
        let root = version_root(delivery)?;
        files.extend(verification::lock_component_files(&root, delivery.files)?);
        validate_exact_tree(&root, delivery.files)?;
        roots.push((delivery.id, root));
    }
    let bundle_root = roots
        .iter()
        .find_map(|(id, root)| (*id == BUNDLE_ID).then_some(root));
    let web_root = bundle_root.map_or_else(
        || component_root(&roots, WEB_ID),
        |root| Ok(root.join(BUNDLE_WEB_PATH)),
    )?;
    let worker_root = bundle_root.map_or_else(
        || component_root(&roots, WORKER_ID),
        |root| Ok(root.clone()),
    )?;
    let worker_path = resolve_owned_path(&worker_root, Path::new(WORKER_PATH))?;
    verification::validate_x64_pe(&worker_path)?;
    Ok(RecorderComponents {
        web_root,
        worker_path,
        _leases: leases,
        _files: files,
    })
}

fn component_root(roots: &[(&str, PathBuf)], id: &str) -> Result<PathBuf> {
    roots
        .iter()
        .find_map(|(candidate, root)| (*candidate == id).then(|| root.clone()))
        .ok_or_else(|| anyhow::anyhow!("Screen Recorder component root is unavailable"))
}

pub(crate) fn refresh_catalog_after_open() {
    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Io,
        "recorder-catalog-refresh",
        || {
            super::update_catalog::refresh_for_use(BUNDLE_ID, "before-open");
            retire_legacy_split_installations();
        },
    );
}

fn retire_legacy_split_installations() {
    if delivery(BUNDLE_ID).is_err() {
        return;
    }
    for id in [WORKER_ID, WEB_ID] {
        match super::request_remove(id) {
            Ok(RemovalOutcome::Missing | RemovalOutcome::Removed) => {}
            Ok(outcome) => crate::log_info!(
                "[ScreenRecord] retired component cleanup deferred id={id} outcome={outcome:?}"
            ),
            Err(error) => crate::log_info!(
                "[ScreenRecord] retired component cleanup failed id={id} error={error:#}"
            ),
        }
    }
}

#[cfg(test)]
mod startup_path_tests {
    #[test]
    fn normal_open_hashes_each_delivery_inventory_once() {
        let source = include_str!("recorder.rs");
        let start = source.find("fn acquire_verified_components(").unwrap();
        let end = source
            .find("pub(crate) fn refresh_catalog_after_open()")
            .unwrap();
        let acquisition = &source[start..end];
        assert_eq!(acquisition.matches("lock_component_files(").count(), 1);
        assert!(!acquisition.contains("validate_install("));
    }

    #[test]
    fn healthy_open_verifies_the_inventory_once() {
        let source = include_str!("recorder.rs");
        let start = source.find("pub(crate) fn ensure_ready(").unwrap();
        let end = source.find("fn acquire_verified_components(").unwrap();
        let ready = &source[start..end];
        let acquire = ready.find("acquire_verified_components(").unwrap();
        let install = ready.find("install::ensure_group(").unwrap();
        assert!(
            acquire < install,
            "a healthy install must be acquired before any restating install pass"
        );
    }

    #[test]
    fn verification_workers_stay_bounded_and_never_idle() {
        assert_eq!(super::verification::verification_workers(0), 1);
        assert_eq!(super::verification::verification_workers(1), 1);
        assert!(super::verification::verification_workers(512) <= super::MAX_VERIFICATION_WORKERS);
        assert!(super::verification::verification_workers(512) >= 1);
    }

    #[test]
    fn catalog_refresh_is_not_on_the_component_ready_path() {
        let source = include_str!("recorder.rs");
        let start = source.find("pub(crate) fn ensure_ready(").unwrap();
        let end = source.find("fn acquire_verified_components(").unwrap();
        assert!(!source[start..end].contains("refresh_for_use"));
    }
}

pub(crate) fn ensure_ready_with_badge(cancelled: &AtomicBool) -> Result<RecorderComponents> {
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name());
    let result = ensure_ready(cancelled, |done, total| badge.report(done, total));
    badge.finish();
    result
}

pub(crate) fn download_title() -> String {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    crate::overlay::auto_copy_badge::format_locale(
        locale.downloading_component_fmt,
        &[("name", &component_name())],
    )
}

pub(crate) fn is_installed() -> bool {
    delivery_group().is_ok_and(|deliveries| {
        deliveries
            .iter()
            .all(|delivery| validate_status(delivery).is_ok())
    })
}

pub(crate) fn delivery_available() -> bool {
    delivery_group().is_ok()
}

pub(crate) fn installed_size() -> u64 {
    delivery_group().map_or(0, |deliveries| {
        deliveries
            .into_iter()
            .filter(|entry| validate_status(entry).is_ok())
            .map(|entry| entry.unpacked_size_bytes)
            .sum()
    })
}

pub(crate) fn remove_all() -> Result<()> {
    let _worker_shutdown = crate::overlay::screen_record::stop_for_component_removal()?;
    for id in [BUNDLE_ID, WORKER_ID, WEB_ID] {
        remove_one(id)?;
    }
    Ok(())
}

pub(crate) fn purge_all_recorded_recoveries() -> Result<Vec<recovery::CleanupOutcome>> {
    let _mutation = super::acquire_mutation_guard()?;
    recovery::purge_all_recorded()
}

pub(crate) fn remove_from_manager() -> Result<()> {
    let name = component_name();
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let removing = crate::overlay::auto_copy_badge::format_locale(
        locale.removing_component_fmt,
        &[("name", &name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &removing,
        "",
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );

    let result = remove_all();
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match &result {
        Ok(()) => (
            locale.component_removed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
        Err(error) => (
            locale.component_remove_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", &name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
    result
}

pub(crate) fn download_from_manager(
    stop: std::sync::Arc<AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let _activity = crate::install_activity::register(stop.clone())?;
    let badge = use_badge
        .then(|| crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name()));
    set_download_state(0.0);
    let result = ensure_ready(&stop, |done, total| {
        let progress = done as f32 / total.max(1) as f32 * 100.0;
        set_download_state(progress);
        if let Some(badge) = &badge {
            badge.report(done, total);
        }
    })
    .map(drop);
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if let Some(badge) = &badge {
        badge.finish();
    }
    if let Err(error) = &result {
        let locale = crate::overlay::auto_copy_badge::locale_text();
        let title = crate::overlay::auto_copy_badge::format_locale(
            locale.component_install_failed_fmt,
            &[("name", &component_name())],
        );
        crate::overlay::auto_copy_badge::show_detailed_notification(
            &title,
            &error.to_string(),
            crate::overlay::auto_copy_badge::NotificationType::Error,
        );
    }
    result
}

fn set_download_state(progress: f32) {
    let title = download_title();
    let message = component_message();
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title;
        state.download_message = message;
        state.download_progress = progress;
    }
}

fn component_name() -> String {
    current_locale()
        .auxiliary
        .managed_tools
        .tool_screen_recorder_card
        .to_string()
}

fn component_message() -> String {
    current_locale()
        .auxiliary
        .managed_tools
        .tool_screen_recorder_payload
        .to_string()
}

fn current_locale() -> crate::gui::locale::LocaleText {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
}

fn remove_one(id: &str) -> Result<()> {
    match super::request_remove_and_wait(id)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => unreachable!("waited removal cannot remain pending"),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{id} contains {} unrecorded or unsafe path(s); they were preserved",
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!(
                "{id} is required by installed components: {}",
                dependents.join(", ")
            )
        }
    }
}

fn delivery(id: &str) -> Result<&'static RecorderDelivery> {
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.id == id)
    {
        return Ok(delivery);
    }
    RECORDER_DELIVERIES
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow::anyhow!("Screen Recorder download contract is unavailable"))
}

fn delivery_group() -> Result<Vec<&'static RecorderDelivery>> {
    if let Ok(bundle) = delivery(BUNDLE_ID) {
        return Ok(vec![bundle]);
    }
    Ok(vec![delivery(WEB_ID)?, delivery(WORKER_ID)?])
}

fn version_root(delivery: &RecorderDelivery) -> Result<PathBuf> {
    super::component_version_root(delivery.id, delivery.version)
}

fn dependencies(id: &str) -> Vec<String> {
    match id {
        WORKER_ID => vec![WEB_ID.to_string()],
        _ => Vec::new(),
    }
}

fn validate_install(delivery: &RecorderDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

fn validate_status(delivery: &RecorderDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &RecorderDelivery,
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::validate_version_root(delivery.id, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != delivery.id
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != dependencies(delivery.id)
        || receipt.files.len() != delivery.files.len()
    {
        bail!("recorder component receipt does not match this build");
    }
    for expected in delivery.files {
        let owned = owned_file(expected);
        if !receipt.files.iter().any(|entry| same_file(entry, &owned)) {
            bail!("recorder component receipt inventory does not match this build");
        }
        let path = resolve_owned_path(&root, Path::new(expected.path))?;
        if !matches(&path, &owned)? {
            bail!("recorder component failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn validate_exact_tree(root: &Path, files: &[RecorderFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_COMPONENT_FILES + 1)?;
    actual.retain(|path| path != Path::new(RECEIPT_NAME));
    if actual.len() != files.len()
        || actual
            .iter()
            .any(|path| !files.iter().any(|file| Path::new(file.path) == path))
    {
        bail!("recorder component contains unowned files");
    }
    Ok(())
}

fn owned_file(file: &RecorderFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.into(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn same_file(left: &OwnedComponentFile, right: &OwnedComponentFile) -> bool {
    left.path == right.path
        && left.size_bytes == right.size_bytes
        && left.sha256.eq_ignore_ascii_case(&right.sha256)
}

fn receipt(delivery: &RecorderDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: dependencies(delivery.id),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod acceptance_tests {
    #[test]
    fn tracked_delivery_contains_a_complete_recorder_group() {
        assert!(super::delivery_available());
    }

    #[test]
    #[ignore = "opens the local recorder window"]
    fn active_recorder_is_stopped_before_removal_returns() {
        crate::overlay::screen_record::show_screen_record();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while !crate::overlay::screen_record::worker_process_is_active() {
            assert!(
                std::time::Instant::now() < deadline,
                "recorder worker did not become active"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        super::remove_all().unwrap();

        assert!(!crate::overlay::screen_record::post_script(
            "void 0".to_string()
        ));
    }
}
