# Text input

Windows owns the editor document,styles and revision-aware edit operations.
Android's overlay asset generator imports those sources;its native adapter owns
document identity,window lifetime,touch dragging and IME focus.

Generate the shared overlay assets from the repository root before running the
script-level contract suite:

```powershell
Push-Location mobile
.\gradlew.bat :androidApp:generatePresetOverlayAssets --console=plain
Pop-Location
node --test src/overlay/text_input/contract_runtime.test.cjs
node src/overlay/text_input/delivery_runtime.test.cjs
```

The contract suite executes production scripts against a DOM API double and
replays the shared text-input fixture. It covers messages,selection,revision-aware
cleanup and platform adapters. It does not open a native window or establish
WebView layout,IME or hardware acceptance. Android's document and rendering JVM
tests run in both Full and Play unit suites and depend on the generated assets;
see `mobile/README.md`.
