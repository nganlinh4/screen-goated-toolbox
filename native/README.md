# Native Runtimes

SGT-owned Windows runtime crates, sidecars, packaging scripts, and manifests live here. The desktop host remains under `src/`; model/install metadata remains in `catalog/model_catalog.json`.

## Active or Packaged Paths

- [Computer Control engine](computer_control_engine/README.md) — removable x64
  data-only planning and provider-protocol worker.
- `language_catalog/` — complete compact ISO 639 lookup crate shared by the
  signed host and recorder worker; exhaustive tests preserve upstream mappings.

## Rules

From the repository root on Windows, run the repository-owned native package
checkpoint with Python 3.11 or newer and the desktop Rust prerequisites:

```powershell
python -m scripts.check_native --list
python -m scripts.check_native
python -m scripts.check_native --crate recorder_worker
```

The command discovers tracked and new non-ignored `native/*/Cargo.toml` packages,
checks formatting, runs locked unit/doc tests, then strict all-target Clippy. It
requires each package's lockfile to be included in Git rather than ignored and
stops at the first failure. Artifacts use the managed `cargo/dev` cache described
in [Development](../docs/DEVELOPMENT.md); it does not prune caches or package or
publish components. The recorder uses its existing direct-source format check
and verifies the pinned egui dependencies. Other packages use `cargo fmt`.
The Development workflow runs this same command on Windows for shared host and
native changes, so a root-only green result cannot hide a native check failure.

Ignored independent source checkouts are owned and checked in their own
repositories rather than discovered as packages of this repository. Removed
offline speech runtimes are listed in [Offline engines](../docs/OFFLINE-ENGINES.md).

- Keep build, package, manifest, catalog, installer, and host-loader claims aligned.
- Do not call a runtime active because a prototype source tree exists; verify the shipped artifact and clean-install URL.
- Never commit model weights, credentials, machine paths, or generated environments.
- Validate the exact host protocol and a real inference request before publishing.
