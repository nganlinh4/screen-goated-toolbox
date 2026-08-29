# Windows guide

## Main window and preset graph

The Windows app is a portable executable. The main window provides the visual preset graph, settings, history, and launchers for focused tools. Presets can be launched from the main window, preset wheel, favorite bubble, or configured shortcuts.

## Capture and overlays

Windows supports screen, window, region, microphone, system-audio, and supported per-app capture workflows. Result overlays can stream text, render Markdown, remain above other windows, and expose copy or save actions appropriate to the result.

## Computer Control

Computer Control combines the current screen context with a stable catalog of browser and operating-system tools. It may require browser integration or Windows permissions for a requested action. Consequential actions require the app's normal confirmation checkpoint. Stop cancels the active control job rather than merely closing its interface.

## Screen Record

Screen Record captures a display, window, or region and opens a project editor. The editor supports trimming, zoom and camera motion, cursor rendering, backgrounds, subtitles, narration, and export. Preview and export use the same composition parameters. Exported videos are written only when the user chooses an export action.

## Mini apps

PromptDJ provides a focused prompt and MIDI workflow. Translation Gummy provides continuous translation with its own compact controls. TTS Playground provides voice experimentation and export. Image Creator, Image to SVG, and Image to 3D use the shared Creation project history and explicit download behavior.

## Storage and cleanup

Settings and history live in the app's data folders. Optional component bytes and caches live under local app data. User exports belong in Downloads or another user-selected destination. Clean All removes managed optional components after stopping their owners; it does not remove recordings, exports, Creation results, source files, history, or settings.

## Troubleshooting

Web-based mini apps require the Microsoft Edge WebView2 Runtime. If a downloadable feature fails to open, use Downloaded Tools to inspect or repair its component. If capture fails, verify the requested Windows privacy permission and that the target still exists. If an existing verified component is available while offline, the app continues using it.
