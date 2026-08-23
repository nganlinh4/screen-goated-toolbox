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
  - On app startup, Windows and Android `full` perform one background check against the signed stable update manifest.
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
  - A valid signed stable manifest is the primary authority for Windows and the
    Android `full` flavor. It names one exact Windows executable and one exact
    Full APK with URL, filename, byte size, and SHA-256.
  - GitHub release `tag_name` is normalized by removing the leading `v` only in
    the hardened fallback path.
  - Android flavor/build suffixes such as `-full`, `-play`, and `-debug` are ignored for update comparison.
  - The Android update **source and action depend on the distribution flavor**:
    - `play` flavor: uses the **Google Play In-App Updates API** (`com.google.android.play:app-update-ktx`), not GitHub. The startup/manual check queries `AppUpdateManager.appUpdateInfo`; if an update is available the primary action launches Play's **flexible** update flow (`startUpdateFlowForResult`), download progress is tracked via an `InstallStateUpdatedListener` (`Downloading` → `Downloaded`), and the `Downloaded` action calls `completeUpdate()` to restart and apply. No GitHub call and no hand-rendered release notes (Play owns the changelog). Implemented in `androidApp/src/play/.../updater/PlayInAppUpdateManager.kt`.
    - `full` (sideload) flavor: signed-manifest driven
      (`androidApp/src/full/.../updater/AppUpdateRepository.kt`). A manifest must
      use the stable channel, a strictly newer normalized version, the exact
      `ScreenGoatedToolbox_v<version>.apk` filename, an HTTPS GitHub release-asset
      URL, positive size, and lowercase SHA-256. Only a missing primary manifest
      may use the hardened GitHub fallback. A present manifest with an invalid
      signature or contract fails closed instead of downgrading authority. The
      fallback rejects drafts,
      prereleases, staging tags, malformed versions/assets, absent digest/size,
      and ambiguous APK assets. All GitHub package-selection code is confined to
      this flavor source set.
- Output contract:
  - `full` flavor must show the same latest-version and release-notes data that Windows uses from GitHub Releases.
  - `play` flavor mirrors Play's update availability instead of GitHub; it shows the current version, a check action, and the in-app flexible update flow.
  - Android performs the same startup auto-check-once-per-launch semantics for both flavors (GitHub for `full`, Play for `play`).

## Failure And Recovery
- Permission/runtime failures:
  - None specific to the check itself.
- Timeout/retry behavior:
  - Network, signature, schema, monotonicity, or package-contract failures move
    the section to `Error`. The hardened fallback is consulted only when the
    primary manifest returns HTTP 404, and must find one unambiguous stable release.
  - Manual retry triggers a fresh check from `Error`, `Idle`, or `UpToDate`.
- Stop/reset behavior:
  - None.

## Fixtures
- Shared fixtures:
  - `parity-fixtures/app-update/latest-release.json`
- Platform-specific tests:
- Android unit tests cover signature verification, manifest validation, version
  normalization, and hardened fallback rejection/selection.

## Deviations
- Windows performs an in-place executable update and can request restart.
- Android cannot mirror Windows' in-place executable replacement, so update delivery diverges by flavor:
  - `play` flavor uses Google Play In-App Updates (flexible flow): the update downloads in-app and `completeUpdate()` restarts to apply it. This is the closest Android analog to Windows' in-place update; release notes are owned by Play rather than mirrored from GitHub.
  - `full` (sideload) flavor opens the exact signed `.apk` asset for a user-driven
    install. Android package installation verifies the app signing identity;
    because a browser owns the downloaded file, the app cannot perform the
    Windows in-process byte verification after handoff. Existing sideload
    installs must reinstall from Play (different signing key) to migrate.
