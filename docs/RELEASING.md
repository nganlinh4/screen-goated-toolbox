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

One release is one gitignored file: `tmp-release-notes-<VERSION>.md`. Every
channel lives in it as a `## ` section, so the whole release reads in one place.

```markdown
# v<VERSION>

## github
- plain English bullet
...

## play en-US
...

## play vi-VN
...
```

`## ` at the start of a line delimits sections, so demote any heading inside a
section body. Read sections back with the helper rather than by hand:

```powershell
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --list
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --section github
```

The `github` section uses the established format: plain English bullets with no
added release heading, followed by the current Zalo support-group line, then a
horizontal rule and the VietQR donation footer:

```text
_Nhóm chat hỗ trợ SGT tại Việt Nam: https://zalo.me/g/arxevk379_

---

💙 **Ủng hộ tác giả** — Người dùng Việt Nam có thể ủng hộ qua VietQR: [bấm vào đây](https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT).
```

Those two trailing lines are the only non-English copy allowed. Keep the
donation link identical to the one in `README.md`. Never include Vietnamese
release notes, Google Play notes, or store metadata in a GitHub release body.

Map every bullet to a real commit. Owner reviews notes before any publish step.

Each `play <locale>` section has a 500-character limit. Never publish a `play`
section as the GitHub body or the reverse: the GitHub body carries the Zalo and
VietQR lines, which do not belong on a store listing.

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
.\scripts\validate-windows-targets.ps1 -Arch x64
```

Run relevant frontend/mobile tests for changed subsystems. Do not waive failures to cut a release.

For a release containing the creation mini apps, build the separately tracked
runtime first and point all host builds at its generated delivery manifest:

### Mandatory creation-runtime release checkpoint

This is a blocking release requirement. Do not proceed to the Windows or
Android host builds until every item is complete:

1. Rebuild the Windows creation runtime from the reviewed private source.
2. Rebuild both Android creation-runtime distributions from that same source.
3. Regenerate the Windows, Android, and combined delivery manifests only after
   all runtime artifacts have been rebuilt. Never reuse an earlier manifest.
4. Replace every creation-runtime binary and manifest on the existing GitHub
   runtime-bundles release. An Android-only refresh or a local-only build is not
   a completed runtime release.
5. Query the GitHub release again and verify every uploaded asset's name, size,
   and checksum against the newly generated manifests.
6. Use that exact verified combined delivery manifest for all subsequent
   Windows, Android Full, and Android Play app builds.

The private runtime source and its build scripts live in the nested, gitignored
`native/sgt_3d_generator_runtime/` repository; its build output lands in the
gitignored `local-runtime-bundles/sgt_creation_runtime/`.

```powershell
.\native\sgt_3d_generator_runtime\scripts\build_exe.ps1
.\native\sgt_3d_generator_runtime\scripts\build_android_runtime.ps1 -CopyToBundleDirectory
$env:SGT_CREATION_RUNTIME_DELIVERY_MANIFEST = 'C:\WORK\screen-goated-toolbox\local-runtime-bundles\sgt_creation_runtime\sgt_creation_runtime.delivery.json'
```

Native libraries must be built for 16 KB memory pages or Play rejects the
update. Verify before publishing that every `PT_LOAD` segment in every `.so`
inside the AAB reports `p_align` of at least 16384.

The same manifest version and feature handshake must feed Windows, Android Full,
and Android Play. It supplies private delivery locations and integrity metadata
at build time; never copy those values into tracked host source, fixtures,
tests, or documentation. A host built without the manifest must fail closed and
report that the creation engine is not included.

## 6. Build Windows

```powershell
.\build.ps1
```

Expected build artifacts:

- `target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe`

Smoke-test the x64 artifact on suitable hardware. GitHub publishes the x64 artifact only.

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
python scripts/release_notes.py tmp-release-notes-<VERSION>.md --section github |
  Set-Content -Path tmp-github-body.txt -Encoding utf8
gh release create v<VERSION> `
  --verify-tag `
  --draft `
  --title "Screen Goated Toolbox v<VERSION>" `
  --notes-file "tmp-github-body.txt" `
  "target/x86_64-pc-windows-msvc/release/ScreenGoatedToolbox_v<VERSION>.exe" `
  "target/release/ScreenGoatedToolbox_v<VERSION>.apk"
```

Every GitHub release must attach both the Windows x64 executable and the Full
Android APK. Review title, body (English bullets plus the step 3 Zalo and
VietQR footer), binaries, sizes, and checksums in GitHub UI. Publish only after
owner approval.

## 10. Publish Google Play

Explicit owner approval after review is the publication gate for both release
channels. Once approval is given, publish the reviewed GitHub draft and submit
the reviewed AAB to the Google Play production track without requesting a second
confirmation. Until then, keep the GitHub release as a draft and do not commit a
Play release edit.

Upload the AAB in Play Console, or use the repository helper:

```powershell
python -m pip install google-api-python-client google-auth
$env:PLAY_SERVICE_ACCOUNT_JSON = '<path to the Play service-account JSON>'
python scripts/play_publish.py `
  --aab "target/release/ScreenGoatedToolbox_v<VERSION>.aab" `
  --track production `
  --notes-md "tmp-release-notes-<VERSION>.md" `
  --fraction 1.0
```

Publish straight to `production`. Never use the `internal` track. `--notes-md`
publishes every `## play <locale>` section in one release, so locales cannot
drift apart. Keep the service-account JSON outside the repository.

### Send the release for review (Play Console, manual)

The helper reports which commit path it took:

- `Google review follows for production.` — the edit was submitted, nothing else
  to do here.
- `NOT yet submitted: ...` — the API refused to submit and committed with
  `changesNotSentForReview=true`. The release exists on the track but no review
  has started, so it will never reach users until it is sent by hand.

Google exposes no API call to submit an already-committed edit; it is Console
only. After a rejection Play forces this path for every later edit.

1. Play Console → the app → **Publishing overview**.
2. Read the banner first. **Some recent changes were rejected** means a previous
   submission failed review; open **Policy status** and confirm every listed
   violation is actually fixed before resubmitting.
3. Check **Policy status → Policy issues** for anything under *App updates with
   these issues will be rejected*. Those block approval even when the release
   itself is fine.
4. Only once those are clear, use **Submit N changes for review** on Publishing
   overview.
5. Confirm the release moves to *In review* on **Test and release → Latest
   releases and bundles**.

Do not resubmit while an appeal is still open on the same violation; wait for
the appeal outcome so the appeal and the new submission do not conflict.

## 11. Finish

- Publish GitHub draft.
- Confirm the Play release reached *In review*, then *Available on Google Play*.
  A committed release that was never sent for review reaches nobody.
- Verify download/install/update paths from a clean client.
- Record any release-only caveat in durable docs, not temporary chat notes.
