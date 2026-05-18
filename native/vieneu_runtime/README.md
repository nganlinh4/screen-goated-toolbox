# VieNeu Runtime Bundle

This runtime is distributed as a managed Screen Goated Toolbox bundle. The app downloads
`dist/sgt_vieneu_runtime.manifest.json`, fetches the listed release chunks, verifies size
and SHA-256, extracts the archive, then launches the bundled Python runtime.

Do not install VieNeu with pip on the end user's machine from the app. Any Python package
build work belongs in the release build step below.

## Build

From Windows PowerShell:

```powershell
native\vieneu_runtime\scripts\build_runtime.ps1 -Version 2026.05.17
```

Upload chunks to the shared runtime release:

```powershell
native\vieneu_runtime\scripts\build_runtime.ps1 -Version 2026.05.17 -Upload
```

The manifest is committed under `native/vieneu_runtime/dist/`; chunk files are release
assets under the `sgt-runtime-bundles` GitHub release.
