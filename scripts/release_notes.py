"""Read one release-notes file that holds every channel's text.

A release keeps a single `tmp-release-notes-<VERSION>.md`, so the whole release
can be reviewed in one place instead of one file per channel and locale:

    # v5.4.2

    ## github
    - user facing bullet
    ...

    ## play en-US
    ...

    ## play vi-VN
    ...

Section names are the text after `## `. `github` is the GitHub release body;
`play <locale>` is the Play listing for that locale.
"""

import argparse
import re
import sys

# Notes are Vietnamese as often as English, and the console here is not UTF-8.
# Without this every non-ASCII release note dies with UnicodeEncodeError.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")

SECTION = re.compile(r"^##\s+(.+?)\s*$")
PLAY_SECTION = re.compile(r"^play\s+(\S+)$", re.IGNORECASE)


def parse(path):
    """Return {section name: text} preserving order."""
    sections = {}
    name = None
    body = []
    for line in open(path, encoding="utf-8").read().splitlines():
        match = SECTION.match(line)
        if match:
            if name is not None:
                sections[name] = "\n".join(body).strip()
            name = match.group(1)
            body = []
        elif name is not None:
            body.append(line)
    if name is not None:
        sections[name] = "\n".join(body).strip()
    return sections


def play_locales(path):
    """Return [(locale, text)] for every `## play <locale>` section."""
    found = []
    for name, text in parse(path).items():
        match = PLAY_SECTION.match(name)
        if match:
            found.append((match.group(1), text))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("file", help="tmp-release-notes-<VERSION>.md")
    parser.add_argument("--section", help="Print one section verbatim (e.g. github).")
    parser.add_argument("--list", action="store_true", help="List section names.")
    args = parser.parse_args()

    sections = parse(args.file)
    if args.list or not args.section:
        for name, text in sections.items():
            print(f"{name}\t{len(text)} chars")
        return 0
    if args.section not in sections:
        print(f"No '{args.section}' section in {args.file}. "
              f"Found: {', '.join(sections) or '(none)'}", file=sys.stderr)
        return 1
    print(sections[args.section])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
