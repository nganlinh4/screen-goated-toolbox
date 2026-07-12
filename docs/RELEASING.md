# Release Checklist

Windows ships through GitHub Releases. Android Play flavor ships through Google Play. Keep machine secrets and private endpoints in gitignored `docs/RELEASING.local.md`.

## 1. Establish scope

```powershell
git status --short
git log <PREVIOUS_TAG>..HEAD --oneline
```

- Start from a clean, reviewed tree.
- Read every commit since the previous tag.
- Do not infer release notes from filenames alone.

## 2. Bump version

Set `[package].version` in `Cargo.toml`. Desktop and Android derive their public version from this value.

Confirm generated/versioned surfaces before continuing:

```powershell
rg -n 'version\s*=|FILEVERSION|ProductVersion' Cargo.toml app.rc mobile
```

## 3. Draft release notes

Create gitignored `tmp-release-notes-<VERSION>.txt`:

1. English bullets.
2. `---` separator.
3. Vietnamese section headed `_Phiên bản tiếng Việt:_`.
4. Support-group line if still current.
5. Donation footer:

```text
---

💙 **Ủng hộ tác giả** — Người dùng Việt Nam có thể ủng hộ qua VietQR: [bấm vào đây](https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT).
```

Map every bullet to a real commit. Owner reviews notes before any publish step.

Google Play release notes have a 500-character limit per language; maintain a separate short file when publishing to Play.

## 4. Refresh help index

Start the private embedding service described in `docs/RELEASING.local.md`, then:

```powershell
python scripts/help_index_build.py
git diff --stat -- help-index.json
```

Confirm `help-index.json` changed for the intended source tree and contains no local secrets.

## 5. Validate

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
.\scripts\validate-windows-targets.ps1 -Arch all
```

Run relevant frontend/mobile tests for changed subsystems. Do not waive failures to cut a release.

## 6. Build Windows

```powershell
.\build.ps1 -Arch x64
.\build.ps1 -Arch arm64
# or both
.\build.ps1 -Arch all
```

Expected build artifacts:

- `target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe`
- `target/aarch64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>-arm64.exe`

Smoke-test each architecture on suitable hardware. GitHub currently publishes x64; attach ARM64 only when its release boundary has been explicitly approved.

## 7. Build Android

The release wrapper always builds the signed full-flavor APK. `-IncludeAab` also builds the Play AAB:

```powershell
.\mobile\build-release.ps1 -IncludeAab
```

Expected copied artifacts:

- `target/release/ScreenGoatedToolbox_v<VERSION>.apk`
- `target/release/ScreenGoatedToolbox_v<VERSION>.aab`

The Play AAB is the store artifact. Treat the full APK as development/direct-distribution output only.

## 8. Finalize the release commit and tag

After owner review, commit the release changes, then create and push the tag from that exact commit:

```powershell
git status --short
git tag -a v<VERSION> -m "Screen Goated Toolbox v<VERSION>"
git push origin HEAD
git push origin v<VERSION>
```

Verify the remote tag resolves to the reviewed release commit before creating a release.

## 9. Draft GitHub release

Create a draft first; use paths produced by step 6:

```powershell
gh release create v<VERSION> `
  --verify-tag `
  --draft `
  --title "Screen Goated Toolbox v<VERSION>" `
  --notes-file "tmp-release-notes-<VERSION>.txt" `
  "target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe"
```

If ARM64 is approved for that release, append its built artifact explicitly.

Review title, body, binaries, sizes, and checksums in GitHub UI. Publish only after owner approval.

## 10. Publish Google Play

Upload the AAB in Play Console, or use the repository helper:

```powershell
python -m pip install google-api-python-client google-auth
$env:PLAY_SERVICE_ACCOUNT_JSON = 'C:\secure\play-service-account.json'
python scripts/play_publish.py `
  --aab "target/release/ScreenGoatedToolbox_v<VERSION>.aab" `
  --track internal `
  --notes-file "play-notes-<VERSION>.txt" `
  --fraction 1.0
```

Test `internal` first. Promote or change track only after validating the uploaded build. Keep service-account JSON outside the repository.

## 11. Finish

- Publish GitHub draft.
- Promote Play release.
- Verify download/install/update paths from a clean client.
- Record any release-only caveat in durable docs, not temporary chat notes.
