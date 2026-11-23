# Video Downloader Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/settings_ui/download_manager/run_download.rs`
  - `src/gui/settings_ui/download_manager/ui/ui_main.rs`
- Supporting process/update logic:
  - `src/gui/settings_ui/download_manager/ytdlp_process.rs`
- Android implementation:
  - `mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/downloader/DownloaderRepository.kt`
  - `mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/downloader/DownloaderDownloadFlow.kt`
  - `mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/downloader/ui/DownloaderScreenContent.kt`

## Behavior Contract
- Quality selection:
  - The last explicit video quality choice is persisted.
  - Fresh installs prefer 1080p. Existing settings with an absent or empty preference retain Best, and new sessions inherit the saved choice.
  - The quality picker is available before scanning. It offers Best, Prefer 1080p, Prefer 720p, and the remembered quality. A short-debounced background scan adds discovered qualities without changing the selection. The picker can also request a scan.
  - A completed video download shows its reported resolution when all completed items share a known resolution. The result never changes the saved preference.
  - Selecting Best clears the persisted height preference and returns downloads to unrestricted best quality.
- Immediate download while analysis is running:
  - During a scan, the status explains that more choices are coming. The primary button lets the user download now with the shown preference. Starting a download cancels a running or pending scan for that URL.
  - Downloads started before analysis completes must use the same selector that Windows uses after quality selection.
  - Video always passes `-f bv*+ba/b`. A remembered `720p` choice adds `-S res:720`: a preference for the best format whose short side is at most 720, else the smallest above. It is never a `[height<=…]` filter, which fails on sites whose formats carry no height and caps portrait video by its long side.
  - Analysis labels each quality by its short side, so a 1080x1920 Short lists as `1080p`. Formats without a video codec contribute nothing; thumbnail sizes are never qualities.
- Output name: `%(title).150B.%(ext)s` on both platforms. Post-text titles (Facebook, X, TikTok) otherwise exceed the 255-unit file-name limit and yt-dlp fails with `Errno 22`.
- Success actions:
  - A completed download exposes Open File, Copy video, and Open Folder actions.
  - Copy video must place a readable file URI on the clipboard through the app FileProvider.
- Default output location:
  - Android's default downloader folder is the public user Downloads folder: `/storage/emulated/0/Download/SGT`.
  - It must not default to the app-scoped external files data folder.
  - Because modern Android restricts raw writes to public Downloads, the default path may stage yt-dlp output in app-owned external storage and then publish the completed file through MediaStore.
- A destination choice belongs to the downloader invocation that opened the picker. Cancelling, failing or closing that invocation retains the previous destination; a late result from a retired picker cannot alter a replacement. Windows uses one background native folder dialog at a time and preserves the filesystem path as Unicode. Android uses its platform picker and storage permissions.
- yt-dlp lifecycle:
  - Both platforms execute yt-dlp without user configuration or user plugin directories, so an unrelated machine customization cannot alter mini-app behavior.
  - Both platforms pass `--` before the user's input, so input that starts with `-` (a bare video ID, or text such as `--config-locations=...`) is a URL operand and never a yt-dlp option.
  - Full extraction uses an app-delivered JavaScript runtime and matching EJS scripts; it never downloads executable challenge code during a user operation.
  - Analysis and download failures retain a bounded useful yt-dlp stderr message instead of replacing it with a generic failure.
  - Windows installs the exact immutable identity selected by the signed SGT component catalog. A typed tool failure can refresh that catalog and retry once with a different verified identity.
  - Android Full adopts the verified bootstrap yt-dlp and independently checks official upstream stable releases on first use, periodically while the downloader is used, and on explicit request. Release discovery resolves an exact tag and verifies its signed checksum before running the Android compatibility probe. Python, FFmpeg, and the Android launchers keep their verified delivery contracts.
  - Android activates verified candidates atomically for new jobs and retains the previous usable version. Running processes keep their original executable until they exit. A failed or interrupted check keeps the current tool available. Android Play excludes the downloader.
  - Both platforms own the installation transaction and launch isolated commands. Android's upstream release lookup is discovery only.
  - Analysis remains optional: users can start a download while analysis is running or after analysis fails.
- Analysis ownership:
  - Each tab has a distinct lifetime identity and each URL edit retires its previous analysis, including when a previously entered URL is restored.
  - Results and errors are published together only for the live tab and input generation that requested them. Closing, resetting or reordering a tab cannot redirect an old result.
  - Scans have bounded concurrency and resource ownership. Cancellation and timeout affect only the scan's owned child and cannot terminate download processes. Pending scans coalesce to the latest request for each tab.
  - Analysis reports subtitle availability without changing the user's subtitle preference. An input edit clears the previous URL's subtitle selection, while remembered quality remains user-owned.

## Shared Fixture
- `parity-fixtures/video-downloader/recovery.json` owns the platform-neutral command isolation, error, and retry transitions.
- `parity-fixtures/video-downloader/analysis-lifecycle.json` owns analysis identity, stale completion, preferences, and concurrency transitions.
- `parity-fixtures/video-downloader/folder-picker.json` owns destination selection, cancellation and stale-result transitions.

## Deviations
- Windows can download directly into arbitrary desktop paths.
- Android defaults to public Downloads/SGT and still supports user-selected internal-storage folders through the existing Android folder picker path.
