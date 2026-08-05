"""Upload an Android App Bundle to Google Play and roll it out to a track.

Uses the Google Play Developer API (androidpublisher v3) — no Gradle/AGP
coupling. Takes an already-built .aab (e.g. from mobile/build-release.ps1).

Setup (one-time):
  1. Play Console -> Setup -> API access -> create a service account
     (links to Google Cloud), download its JSON key.
  2. In API access, grant that service account release permission for this app.
  3. Keep the JSON key OUTSIDE the repo; point PLAY_SERVICE_ACCOUNT_JSON at it
     (or pass --credentials).

Install deps:
  pip install google-api-python-client google-auth

Examples:
  # Full production rollout with release notes:
  python scripts/play_publish.py --aab mobile/.../androidApp-play-release.aab \
      --track production --notes-file play-notes-5.3.0.txt

  # Staged 20% rollout:
  python scripts/play_publish.py --aab <bundle> --track production --fraction 0.2

  # Internal track (no review wait), to smoke-test the pipeline:
  python scripts/play_publish.py --aab <bundle> --track internal
"""

import argparse
import os
import sys

PACKAGE_NAME = "dev.screengoated.toolbox.mobile"
SCOPES = ["https://www.googleapis.com/auth/androidpublisher"]
PLAY_NOTE_LIMIT = 500  # Google Play caps release notes at 500 chars per language.


def main() -> int:
    parser = argparse.ArgumentParser(description="Publish an AAB to Google Play.")
    parser.add_argument("--aab", help="Path to the .aab to upload (omit if --version-code given).")
    parser.add_argument("--version-code", type=int,
                        help="Use an already-uploaded versionCode instead of uploading a new .aab.")
    parser.add_argument("--track", default="production",
                        help="Track: production | beta | alpha | internal (default: production).")
    parser.add_argument("--notes-file", action="append", default=[],
                        help="Release-notes text file (<=500 chars for Play). "
                             "Repeat with --lang to publish several locales in one release.")
    parser.add_argument("--lang", action="append", default=[],
                        help="Release-notes language for the matching --notes-file "
                             "(default: en-US). Repeat once per --notes-file.")
    parser.add_argument("--fraction", type=float, default=1.0,
                        help="Rollout fraction 0<f<=1 (1.0 = full, default).")
    parser.add_argument("--credentials",
                        default=os.environ.get("PLAY_SERVICE_ACCOUNT_JSON"),
                        help="Service-account JSON path (or set PLAY_SERVICE_ACCOUNT_JSON).")
    args = parser.parse_args()

    if not args.credentials or not os.path.exists(args.credentials):
        return _fail("Service-account JSON not found. Set PLAY_SERVICE_ACCOUNT_JSON or pass --credentials.")
    if not args.version_code and not (args.aab and os.path.exists(args.aab)):
        return _fail("Provide --aab <path> or --version-code <int>.")
    if not (0.0 < args.fraction <= 1.0):
        return _fail("--fraction must be between 0 (exclusive) and 1.0.")

    try:
        from google.oauth2 import service_account
        from googleapiclient.discovery import build
        from googleapiclient.http import MediaFileUpload
    except ImportError:
        return _fail("Missing deps. Run: pip install google-api-python-client google-auth")

    creds = service_account.Credentials.from_service_account_file(args.credentials, scopes=SCOPES)
    service = build("androidpublisher", "v3", credentials=creds, cache_discovery=False)
    edits = service.edits()

    edit_id = edits.insert(packageName=PACKAGE_NAME, body={}).execute()["id"]

    if args.version_code:
        version_code = args.version_code
        print(f"  using existing versionCode {version_code} (skipping upload)")
    else:
        print(f"Uploading {os.path.basename(args.aab)} ...")
        media = MediaFileUpload(args.aab, mimetype="application/octet-stream", resumable=True)
        bundle = edits.bundles().upload(
            packageName=PACKAGE_NAME, editId=edit_id, media_body=media,
        ).execute()
        version_code = bundle["versionCode"]
        print(f"  uploaded versionCode {version_code}")

    release = {
        "versionCodes": [str(version_code)],
        "status": "completed" if args.fraction >= 1.0 else "inProgress",
    }
    if args.fraction < 1.0:
        release["userFraction"] = args.fraction
        # countryTargeting is only valid on staged (in-progress) releases.
        release["countryTargeting"] = {"includeRestOfWorld": True}
    if args.notes_file:
        langs = args.lang or ["en-US"]
        if len(langs) != len(args.notes_file):
            return _fail(
                f"Got {len(args.notes_file)} --notes-file and {len(langs)} --lang. "
                "Pass one --lang per --notes-file so each locale keeps its own text."
            )
        release_notes = []
        for path, lang in zip(args.notes_file, langs):
            notes = open(path, encoding="utf-8").read().strip()
            if len(notes) > PLAY_NOTE_LIMIT:
                print(f"  WARNING: {lang} notes are {len(notes)} chars; "
                      f"Play caps at {PLAY_NOTE_LIMIT}. Truncating.")
                notes = notes[:PLAY_NOTE_LIMIT]
            release_notes.append({"language": lang, "text": notes})
            print(f"  notes {lang}: {len(notes)} chars from {os.path.basename(path)}")
        release["releaseNotes"] = release_notes

    edits.tracks().update(
        packageName=PACKAGE_NAME, editId=edit_id, track=args.track,
        body={"track": args.track, "releases": [release]},
    ).execute()

    edits.commit(packageName=PACKAGE_NAME, editId=edit_id).execute()
    pct = f"{args.fraction * 100:.0f}%"
    print(f"Done: '{args.track}' release committed (versionCode {version_code}, rollout {pct}). "
          f"Google review follows for production.")
    return 0


def _fail(message: str) -> int:
    print(f"ERROR: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
