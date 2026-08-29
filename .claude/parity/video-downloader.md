# Video Downloader Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/settings_ui/download_manager/run_download.rs`
  - `src/gui/settings_ui/download_manager/ui/ui_main.rs`
- Supporting process/update logic:
  - `src/gui/settings_ui/download_manager/ytdlp_process.rs`
- Android implementation:
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/downloader/DownloaderRepository.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/downloader/DownloaderDownloadFlow.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/downloader/ui/DownloaderScreenContent.kt`

## Behavior Contract
- Quality selection:
  - The last explicit video quality choice is persisted.
  - New video sessions display that persisted choice instead of forcing the dropdown back to Best.
  - Selecting Best clears the persisted height preference and returns downloads to unrestricted best quality.
- Immediate download while analysis is running:
  - If a selected or remembered quality exists, the primary button must say it will prefer that height or lower.
  - Downloads started before analysis completes must use the same height-limited selector that Windows uses after quality selection.
  - For a remembered `720p` choice, Android must pass `bestvideo[height<=720]+bestaudio/best[height<=720]`.
- Success actions:
  - A completed download exposes Open File, Copy video, and Open Folder actions.
  - Copy video must place a readable file URI on the clipboard through the app FileProvider.
- Default output location:
  - Android's default downloader folder is the public user Downloads folder: `/storage/emulated/0/Download/SGT`.
  - It must not default to the app-scoped external files data folder.
  - Because modern Android restricts raw writes to public Downloads, the default path may stage yt-dlp output in app-owned external storage and then publish the completed file through MediaStore.
- yt-dlp lifecycle:
  - Both platforms execute yt-dlp without user configuration or user plugin directories, so an unrelated machine customization cannot alter mini-app behavior.
  - Full extraction uses an app-delivered JavaScript runtime and matching EJS scripts; it never downloads executable challenge code during a user operation.
  - Analysis and download failures retain a bounded useful yt-dlp stderr message instead of replacing it with a generic failure.
  - First use installs the exact immutable yt-dlp identity selected by the app's verified delivery contract.
  - A typed yt-dlp failure refreshes the signed SGT component catalog and retries once only when that catalog selects a different verified identity.
  - Neither platform invokes yt-dlp's upstream self-updater or a mutable latest/nightly URL at runtime.
  - Analysis remains optional: users can start a download while analysis is running or after analysis fails.

## Shared Fixture
- `parity-fixtures/video-downloader/recovery.json` owns the platform-neutral command isolation, error, and retry transitions.

## Deviations
- Windows can download directly into arbitrary desktop paths.
- Android defaults to public Downloads/SGT and still supports user-selected internal-storage folders through the existing Android folder picker path.
