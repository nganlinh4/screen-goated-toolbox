# App Update Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/app/logic.rs`
  - `src/gui/settings_ui/global/update_section.rs`
- Supporting state/logic:
  - `src/updater.rs`
- UI/output owners:
  - `src/gui/settings_ui/global/mod.rs`
  - `src/gui/locale/en.rs`
  - `src/gui/locale/vi.rs`
  - `src/gui/locale/ko.rs`

## Behavior Contract
- User-visible flow:
  - On app startup, Windows performs one background check against the latest GitHub release.
  - If a newer release exists, Windows surfaces a notification and the settings update section moves to `UpdateAvailable`.
  - The settings update section always exposes the current version and a manual check action.
  - Manual check transitions through `Idle -> Checking -> UpToDate | UpdateAvailable | Error`.
  - When an update is available, release notes are visible from the settings surface and the primary action moves to download/update.
- State model:
  - `Idle`
  - `Checking`
  - `UpToDate(currentVersion)`
  - `UpdateAvailable(version, body, releaseUrl, optionalAssetUrl)`
  - `Error(message)`
  - Play-flavor-only flexible-download states: `Downloading`, `Downloaded` (no Windows/`full` equivalent — they reflect Play's in-app flexible update progress).
- Transition rules:
  - Comparison is against the canonical shared app version, not platform-specific debug/flavor suffixes.
  - GitHub release `tag_name` is normalized by removing the leading `v` (`full` flavor only).
  - Android flavor/build suffixes such as `-full`, `-play`, and `-debug` are ignored for update comparison.
  - The Android update **source and action depend on the distribution flavor**:
    - `play` flavor: uses the **Google Play In-App Updates API** (`com.google.android.play:app-update-ktx`), not GitHub. The startup/manual check queries `AppUpdateManager.appUpdateInfo`; if an update is available the primary action launches Play's **flexible** update flow (`startUpdateFlowForResult`), download progress is tracked via an `InstallStateUpdatedListener` (`Downloading` → `Downloaded`), and the `Downloaded` action calls `completeUpdate()` to restart and apply. No GitHub call and no hand-rendered release notes (Play owns the changelog). Implemented in `updater/PlayInAppUpdateManager.kt`.
    - `full` (sideload) flavor: GitHub-driven (`updater/AppUpdateRepository.kt`) — prefer the latest `.apk` asset URL; fall back to the release page URL when no `.apk` exists.
- Output contract:
  - `full` flavor must show the same latest-version and release-notes data that Windows uses from GitHub Releases.
  - `play` flavor mirrors Play's update availability instead of GitHub; it shows the current version, a check action, and the in-app flexible update flow.
  - Android performs the same startup auto-check-once-per-launch semantics for both flavors (GitHub for `full`, Play for `play`).

## Failure And Recovery
- Permission/runtime failures:
  - None specific to the check itself.
- Timeout/retry behavior:
  - Network/parse failures move the section to `Error`.
  - Manual retry triggers a fresh check from `Error`, `Idle`, or `UpToDate`.
- Stop/reset behavior:
  - None.

## Fixtures
- Shared fixtures:
  - `parity-fixtures/app-update/latest-release.json`
- Platform-specific tests:
  - Android unit tests should cover version normalization and asset-selection fallback.

## Deviations
- Windows performs an in-place executable update and can request restart.
- Android cannot mirror Windows' in-place executable replacement, so update delivery diverges by flavor:
  - `play` flavor uses Google Play In-App Updates (flexible flow): the update downloads in-app and `completeUpdate()` restarts to apply it. This is the closest Android analog to Windows' in-place update; release notes are owned by Play rather than mirrored from GitHub.
  - `full` (sideload) flavor opens the GitHub `.apk` asset or release page for a user-driven install. Existing sideload installs must reinstall from Play (different signing key) to migrate.
