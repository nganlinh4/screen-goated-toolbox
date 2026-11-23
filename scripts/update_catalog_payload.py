"""The exact bytes and published name of an SGT component update catalog.

Shared by the signing build and the source verifier, so the verifier can tell
whether a sequence was already published for different contents without
needing the signing dependency.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG_ASSET = re.compile(r"^sgt-component-catalog-v(\d{6})-([0-9a-f]{16})\.json$")


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise SystemExit(f"missing catalog input: {path}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid JSON {path}: {error}") from error


def build_payload(sources: dict[str, object], root: Path = ROOT) -> dict[str, object]:
    contracts = []
    seen = set()
    for source in sources.get("contracts", []):
        if not isinstance(source, dict):
            raise SystemExit("catalog contract source must be an object")
        name = source.get("name")
        relative = source.get("path")
        platform = source.get("platform")
        if not all(isinstance(value, str) and value for value in (name, relative, platform)):
            raise SystemExit("catalog contract source fields are invalid")
        if name in seen:
            raise SystemExit(f"duplicate catalog contract: {name}")
        seen.add(name)
        path = (root / relative).resolve()
        if root not in path.parents:
            raise SystemExit(f"catalog contract escapes repository: {relative}")
        contracts.append(
            {"name": name, "platform": platform, "delivery": load_json(path)}
        )
    return {
        "schemaVersion": 1,
        "sequence": sources["sequence"],
        "channel": sources["channel"],
        "minHostVersion": sources["minHostVersion"],
        "maxHostVersionExclusive": sources["maxHostVersionExclusive"],
        "contracts": contracts,
        "policies": sources.get("policies", []),
    }


def catalog_bytes(payload: dict[str, object]) -> bytes:
    return (json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def catalog_stem(sequence: int, catalog: bytes) -> str:
    digest = hashlib.sha256(catalog).hexdigest()
    return f"sgt-component-catalog-v{sequence:06d}-{digest[:16]}"


def sequence_errors(sequence: int, expected_asset: str, published_names: list[str]) -> list[str]:
    """A published sequence is immutable: reject reuse for other bytes, or going back.

    Re-running a build whose catalog is already published (a partial retry)
    produces the same name and passes.
    """
    published: dict[int, set[str]] = {}
    for name in published_names:
        if match := CATALOG_ASSET.match(name):
            published.setdefault(int(match.group(1)), set()).add(name)
    errors = []
    if published and sequence < max(published):
        errors.append(
            f"catalog sequence {sequence} is below the published sequence {max(published)}"
        )
    if sequence in published and expected_asset not in published[sequence]:
        errors.append(
            f"catalog sequence {sequence} is already published with different contents; "
            "increment `sequence` in component-delivery/update-catalog-v1.sources.json"
        )
    return errors
