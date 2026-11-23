"""Regenerates golden.json by drawing preview.html in headless Edge or Chrome.

The Screen Recorder preview runs in WebView2, so a Chromium Canvas 2D is the
reference the GPU export must match. Usage: python generate.py [browser.exe]
"""

from __future__ import annotations

import html
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CANDIDATES = (
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
)


def find_browser(explicit: str | None) -> str:
    for candidate in ([explicit] if explicit else []) + list(CANDIDATES):
        if candidate and Path(candidate).is_file():
            return candidate
    raise SystemExit("no Edge or Chrome found; pass the browser path")


def render(browser: str) -> dict:
    with tempfile.TemporaryDirectory() as profile:
        dom = subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                f"--user-data-dir={profile}",
                "--dump-dom",
                (HERE / "preview.html").as_uri(),
            ],
            capture_output=True,
            text=True,
            timeout=120,
            check=True,
        ).stdout
    match = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
    if not match:
        raise SystemExit("preview.html produced no output")
    return json.loads(html.unescape(match.group(1)))


def main() -> None:
    browser = find_browser(sys.argv[1] if len(sys.argv) > 1 else None)
    result = render(browser)
    golden = {
        "version": 1,
        "description": (
            "Cursor drop shadow as the Canvas 2D preview draws it (cursorGraphics.ts): "
            "a white square cursor, 22 cursor units wide at cursor scale 2, centred on "
            "(64, 64) of a white 128 x 128 frame. Values are encoded grey levels along "
            "row y=64, column x=64 and the diagonal, from 80 to 127. The GPU export test "
            "gpu_export::cursor_shadow_parity_tests renders the same scene. Regenerate "
            "with generate.py; never hand-edit numbers."
        ),
        "browser": result["userAgent"],
        "frame": {"size": 128, "cursorCenter": [64, 64], "cursorScale": 2, "halfSquare": 11},
        "cases": result["cases"],
    }
    (HERE / "golden.json").write_text(json.dumps(golden, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {len(golden['cases'])} cases with {browser}")


if __name__ == "__main__":
    main()
