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
- Transition rules:
  - Comparison is against the canonical shared app version, not platform-specific debug/flavor suffixes.
  - GitHub release `tag_name` is normalized by removing the leading `v`.
  - Android flavor/build suffixes such as `-full`, `-play`, and `-debug` are ignored for update comparison.
  - The Android update **action target depends on the distribution flavor**:
    - `play` flavor: the action opens the Google Play listing (`market://details?id=dev.screengoated.toolbox.mobile`, web fallback `https://play.google.com/store/apps/details?id=...`). Play performs the actual update.
    - `full` (sideload) flavor: prefer the latest `.apk` asset URL; fall back to the release page URL when no `.apk` exists. (Legacy path — being phased out as Android moves to Play-only.)
- Output contract:
  - Android must show the same latest-version and release-notes data that Windows uses from GitHub Releases.
  - Android must perform the same startup auto-check semantics once per app launch.

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
- Android cannot mirror Windows' in-place replacement from the app UI:
  - `play` flavor delegates updates to Google Play — the action opens the Play listing and Play auto-updates installed builds.
  - `full` (sideload) flavor opens the GitHub `.apk` asset or release page for a user-driven install. This path is being retired in favor of Play-only distribution; existing sideload installs must reinstall from Play (different signing key) to migrate.
