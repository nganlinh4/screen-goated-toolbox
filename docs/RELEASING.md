# Releasing Screen Goated Toolbox

End-to-end checklist for cutting a release. Windows ships as a GitHub release (`.exe`);
Android ships to Google Play (`.aab`). Replace `<VERSION>` with the semver (e.g. `5.2.0`).

> Machine-specific details (embedding server SSH, etc.) live in the **gitignored**
> `docs/RELEASING.local.md`. If you don't have it, ask the repo owner.

---

## 1. Bump the version
- Set `version = "<VERSION>"` in `Cargo.toml` (this is the canonical app version — Windows + Android both derive from it).
- Android `versionCode` is derived automatically from the semver. For a **re-upload at the same semver**, pass `-PversionCodeOverride=<int>` to the Android build (see step 5).

## 2. Draft & review release notes  →  `tmp-release-notes-<VERSION>.txt`
*(this file is gitignored — it's a local draft)*
1. **Read every commit since the last release** (`git log <last-tag>..HEAD`). Base the notes on what actually changed.
2. **Write** the notes into `tmp-release-notes-<VERSION>.txt`:
   - English bullet list first, then a `---`, then the Vietnamese section (`_Phiên bản tiếng Việt:_`).
   - End with the Zalo support-group line.
3. **Verify each line you wrote maps to a real commit** — no invented or hallucinated entries. Cross-check every bullet against the commit log.
4. **Hand it to the user to check & edit.** Do not proceed past this point until the user has reviewed and approved the notes.

## 3. Rebuild the help index (embedding server)
The in-app help assistant uses a RAG index (`help-index.json`) built by embedding the codebase.
- Bring up the embedding server and run the reindex per **`docs/RELEASING.local.md`** (SSH to the MG4 box → `nemotron-embed`, then `python scripts/help_index_build.py` in this repo).
- Confirm `help-index.json` regenerated before building.

## 4. Build Windows
```powershell
powershell -File build.ps1            # x64 only
powershell -File build.ps1 -Arch all  # x64 + arm64
```
Produces `target/.../ScreenGoatedToolbox_v<VERSION>.exe` (and `-arm64.exe` for arm64).

## 5. Build Android
```powershell
# from C:\WORK\screen-goated-toolbox\mobile
.\build-release.ps1
# re-upload at same semver → bump the Play versionCode:
.\build-release.ps1 -PversionCodeOverride=<int>
```
Produces the Play `.aab` and (currently) a sideload `.apk`. See **Android distribution** note below.

## 6. GitHub release (Windows `gh` CLI) — as a DRAFT
Rename the built APK to match the EXE base name (same name, different extension), then:
```powershell
gh release create v<VERSION> `
  --draft `
  --title "Screen Goated Toolbox v<VERSION>" `
  --notes-file tmp-release-notes-<VERSION>.txt `
  "ScreenGoatedToolbox_v<VERSION>.exe" `
  "ScreenGoatedToolbox_v<VERSION>.apk"
```
- **Draft** so it can be reviewed on GitHub before going public.
- Title is exactly `Screen Goated Toolbox v<VERSION>`.
- Body is the **exact** contents of `tmp-release-notes-<VERSION>.txt` (bilingual EN + VI).
- After reviewing the draft on GitHub, publish it.

## 7. Google Play
- Upload the `.aab` to the relevant track (closed test → production once approved).
- Add the same release notes in the Play Console release.

---

## Android distribution (transition in progress)
The app is now live on Google Play. We are moving Android **off** the GitHub `.apk` sideload path:
- **Going forward:** Android updates come from **Google Play** (auto-update). New users install from the Play listing.
- The in-app Android updater (`mobile/.../updater/`, governed by `.claude/parity/app-update.md`) is being switched from "download the GitHub `.apk`" to "open the Play Store."
- Existing sideload (`full`-flavor) users **cannot** auto-migrate to Play (different signing key) — they must uninstall and reinstall from Play.
- Once the switch lands, **drop the `.apk` from step 6** (Windows `.exe` only) and stop building the sideload APK in step 5.

## Quick reference
| Step | Command / file |
|---|---|
| Version | `Cargo.toml` → `version = "<VERSION>"` |
| Release notes | `tmp-release-notes-<VERSION>.txt` (read commits → write → verify → user approves) |
| Help index | `docs/RELEASING.local.md` → `python scripts/help_index_build.py` |
| Build Windows | `powershell -File build.ps1 [-Arch all]` |
| Build Android | `mobile\build-release.ps1 [-PversionCodeOverride=<int>]` |
| GitHub release | `gh release create v<VERSION> --draft --title "Screen Goated Toolbox v<VERSION>" --notes-file …` |
| Play | upload `.aab` to track |
