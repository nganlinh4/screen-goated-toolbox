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
   - End with the Zalo support-group line, then the **donation footer** below — one clean line, identical on every release (VietQR link only, never a bundled image):

```
---

💙 **Ủng hộ tác giả** — Người dùng Việt Nam có thể ủng hộ qua VietQR: [bấm vào đây](https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT).
```
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
Two ways to push the `.aab` to a track:

**a) Console UI** — upload the `.aab` to the track (production / closed / etc.), add release notes, roll out.

**b) CLI (no browser)** — `scripts/play_publish.py` via the Play Developer API:
```
pip install google-api-python-client google-auth      # one-time
set PLAY_SERVICE_ACCOUNT_JSON=C:\path\to\play-service-account.json
python scripts/play_publish.py --aab <path-to.aab> --track production \
    --notes-file play-notes-<VERSION>.txt --fraction 1.0
```
- One-time setup: Play Console → Setup → API access → create a service account, download its JSON key, grant it release permission. Keep the key **outside the repo** (it's gitignored as a safety net).
- Play caps release notes at **500 chars/language**, so use a short `play-notes-<VERSION>.txt`, not the full GitHub `tmp-release-notes` file.
- Use `--track internal` first to smoke-test the pipeline; `--fraction 0.2` for a staged rollout.

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
