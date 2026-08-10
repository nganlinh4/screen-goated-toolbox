# Computer Control Engine

This x64 worker owns Computer Control's data-only setup planning and provider
frame interpretation. The desktop host remains the authority for credentials,
network transport, audio, observations, freshness, confirmation, effects,
receipts, cancellation, reconnects, and process lifetime.

The worker accepts only the versioned, authenticated, size-bounded protocol in
`../computer_control_protocol`. It has no effect channel or provider credential.
Every normal setup extends the complete static catalog with validated dynamic
integration declarations; it never substitutes a smaller catalog.

Development checks do not require a release build:

```powershell
cargo test --manifest-path native/computer_control_engine/Cargo.toml
cargo clippy --manifest-path native/computer_control_engine/Cargo.toml --all-targets -- -D warnings
```

At the release checkpoint, `scripts/build-computer-control-engine-pack.ps1`
builds the x64 worker and creates a deterministic content-addressed archive with
its complete resolved third-party license inventory. The signed host consumes
only delivery metadata generated after the published asset is downloaded and
verified by `scripts/verify_computer_control_release.py`.
