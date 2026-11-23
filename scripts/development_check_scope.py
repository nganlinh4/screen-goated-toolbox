"""Select development checks from committed GitHub event inputs."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess


def requires_checks(event_name, event, repo):
    if event_name == "workflow_dispatch":
        return True
    if event_name == "pull_request":
        base = event["pull_request"]["base"]["sha"]
        head = event["pull_request"]["head"]["sha"]
    elif event_name == "push":
        base, head = event["before"], event["after"]
        if event.get("deleted") is True:
            if head != "0" * 40:
                raise ValueError("A branch deletion requires an empty head identity")
            return False
        if base == "0" * 40:
            return True
    else:
        return True
    if not all(isinstance(value, str) and re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", value) for value in (base, head)):
        raise ValueError("Development diff requires complete commit identities")
    result = subprocess.run(
        ["git", "diff", "--quiet", "--no-renames", base, head, "--", ".",
         ":!README.md", ":!AGENTS.md", ":!docs/DEVELOPMENT.md", ":!docs/RELEASING.md"],
        cwd=repo, check=False, capture_output=True,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError("Cannot determine development check scope from Git history")
    return result.returncode == 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event-file", type=Path, default=os.environ.get("GITHUB_EVENT_PATH"), required="GITHUB_EVENT_PATH" not in os.environ)
    parser.add_argument("--event-name", default=os.environ.get("GITHUB_EVENT_NAME"), required="GITHUB_EVENT_NAME" not in os.environ)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    event = json.loads(args.event_file.read_text(encoding="utf-8"))
    selected = str(requires_checks(args.event_name, event, repo)).lower()
    print(f"Development checks required: {selected}")
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"required={selected}\n")


if __name__ == "__main__":
    main()
