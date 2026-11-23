# App Update Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/app/logic.rs`
  - `src/gui/settings_ui/global/update_section.rs`
- Supporting state/logic:
  - `src/updater.rs`
  - `src/updater/controller.rs`
  - `src/updater/lifecycle.rs`
  - `src/updater/manifest.rs`
  - `src/updater/github.rs`
  - `mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/updater/AppUpdateSource.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/updater/AppUpdateStateOwner.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/updater/AppUpdateModels.kt`
- UI/output owners:
  - `src/gui/settings_ui/global/mod.rs`
  - `src/gui/locale/en.rs`
  - `src/gui/locale/vi.rs`
  - `src/gui/locale/ko.rs`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/AppUpdateSection.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/MobileUpdateLocales.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainActivity.kt`

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
  - Windows and Play managed delivery includes `Downloading`.
  - Windows exposes `UpdatedAndRestartRequired` after verified staging.
  - Play additionally exposes `Downloaded` and `Installing`.
    Applying a downloaded update stays pending until Play restarts the app.
    A failed completion request returns to `Downloaded` with a visible error
    and a retry action;it does not discard the downloaded update.
- Transition rules:
  - Check admission is atomic. Startup checks run once per controller lifetime;
    a manual/startup check already in flight is coalesced. Checks do not replace
    an active download,downloaded update,installation or staged restart action.
  - Results and queued UI effects belong to the exact admitted operation. A
    retired check cannot restore old availability,clear an install state or show
    a notification. Each current notification is consumed at most once.
  - The Full browser handoff reads the current controller state at activation,
    rather than a previously rendered snapshot,and requires current availability.
  - Play query snapshots prioritize active installation/download state. Unknown
    availability,unsupported flexible updates or missing release identity are
    errors;only an explicit no-update result is shown as up to date.
  - Windows and Play consume a checked managed-update action once. Duplicate starts do not
    cancel or replace admitted work. Starting a fresh check retires old actions.
  - Comparison is against the canonical shared app version, not platform-specific debug/flavor suffixes.
  - A valid signed stable manifest is the primary authority for Windows and the
    Android `full` flavor. It names one exact Windows executable and one exact
    Full APK with URL, filename, byte size, and SHA-256.
  - Both platforms validate the complete signed manifest, including both package
    contracts. Schema and sizes are JSON integers, flags are JSON booleans,
    and notes are text. Invalid field types are rejected without coercion.
    Manifest notes may be omitted;fallback notes may also be null.
  - Feed bytes are strict UTF-8. Verification binds the exact response bytes;
    malformed encoding is rejected even when its signature is valid.
    Only the primary manifest's HTTP 404 permits fallback. A missing signature,
    other HTTP status, transport failure, oversized response or rejected signed
    contract remains an error without consulting release metadata.
  - Fallback selection validates the platform's own exact package. Duplicate exact
    installer/APK names are ambiguous even when their values are identical. A
    fallback digest must carry the explicit `sha256:` algorithm prefix.
    Invalid JSON field types can reject release metadata before package selection;
    unrelated package entries do not excuse malformed integer sizes.
  - Fallback compares every valid candidate in a completed bounded traversal.
    Identical highest-version records may overlap across pages. Conflicting
    highest-version package contracts or notes fail closed, regardless of order;
    conflicts below a newer unambiguous candidate do not block that candidate.
    Package equality is platform-specific, including name, URL, size and digest.
  - A partial or empty page completes the release census. An overfull page is
    rejected. Reaching the ten-page budget with a full page is an error, rather
    than reporting a candidate from an incomplete census. Later page failures
    discard the partial result. Selection retains only the current highest
    candidate and its ambiguity state between pages.
  - GitHub release `tag_name` is normalized by removing the leading `v` only in
    the hardened fallback path.
  - Android flavor/build suffixes such as `-full`, `-play`, and `-debug` are ignored for update comparison.
  - The Android update **source and action depend on the distribution flavor**:
    - `play` flavor: uses the **Google Play In-App Updates API** (`com.google.android.play:app-update-ktx`), not GitHub. The startup/manual check queries `AppUpdateManager.appUpdateInfo`; if an update is available the primary action launches Play's **flexible** update flow (`startUpdateFlowForResult`), download progress is tracked via an `InstallStateUpdatedListener` (`Downloading` → `Downloaded`), and the `Downloaded` action calls `completeUpdate()` to restart and apply. No GitHub call and no hand-rendered release notes (Play owns the changelog). Implemented in `androidApp/src/play/.../updater/PlayInAppUpdateManager.kt`.
      The launch action reports the SDK's actual acceptance boolean. A declined
      launch or thrown failure returns false;absence of an exception alone does
      not prove that the flexible update flow started.
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
  - A retired Windows controller rejects queued and late results before UI effects.
    Active I/O may finish without reactivating the retired controller.
  - Android's app-scoped controller survives activity recreation. Recreating its
    UI does not repeat startup checks or already consumed update notifications.

## Fixtures
- Shared fixtures:
  - `parity-fixtures/app-update/latest-release.json`
  - `parity-fixtures/app-update/play-flow.json` (Play-only launch acceptance)
  - `parity-fixtures/app-update/lifecycle.json` (operation and effect ownership)
  - `parity-fixtures/app-update/play-query.json` (Play query projection)
  - `parity-fixtures/app-update/authority.json` (signed bytes and source authority)
  - `parity-fixtures/app-update/fallback-selection.json` (completed release census)
- Platform-specific tests:
- Android unit tests cover signature verification, manifest validation, version
  normalization, and hardened fallback rejection/selection.
- Shared acceptance cases exercise both package contracts,strict field types,
  platform-specific fallback ambiguity/digests,and version comparison using
  canonical Windows versions and Android flavor/debug names.

## Deviations
- Windows performs an in-place executable update and can request restart.
- Android cannot mirror Windows' in-place executable replacement, so update delivery diverges by flavor:
  - `play` flavor uses Google Play In-App Updates (flexible flow): the update downloads in-app and `completeUpdate()` restarts to apply it. This is the closest Android analog to Windows' in-place update; release notes are owned by Play rather than mirrored from GitHub.
  - `full` (sideload) flavor opens the exact signed `.apk` asset for a user-driven
    install. Android package installation verifies the app signing identity;
    because a browser owns the downloaded file, the app cannot perform the
    Windows in-process byte verification after handoff. Existing sideload
    installs must reinstall from Play (different signing key) to migrate.
