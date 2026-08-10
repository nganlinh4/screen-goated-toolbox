#!/usr/bin/env python3
"""Build and sign an append-only SGT component update catalog."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path

from ecdsa import NIST256p, SigningKey
from ecdsa.util import sigencode_string


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCES = ROOT / "component-delivery" / "update-catalog-v1.sources.json"
PUBLIC_KEY = ROOT / "component-delivery" / "update-catalog-p256-public-key.hex"


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise SystemExit(f"missing catalog input: {path}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid JSON {path}: {error}") from error


def private_key(args: argparse.Namespace) -> SigningKey:
    if args.private_key:
        encoded = args.private_key.read_bytes()
    else:
        value = os.environ.get("SGT_UPDATE_CATALOG_P256_PRIVATE_KEY_PEM_BASE64", "")
        if not value:
            raise SystemExit("update catalog signing key is unavailable")
        try:
            encoded = base64.b64decode(value, validate=True)
        except ValueError as error:
            raise SystemExit("update catalog signing key is not valid base64") from error
    try:
        key = SigningKey.from_pem(encoded.decode("ascii"))
    except (UnicodeDecodeError, ValueError) as error:
        raise SystemExit("update catalog signing key is not valid PEM") from error
    if key.curve != NIST256p:
        raise SystemExit("update catalog key must be ECDSA P-256")
    return key


def public_hex(key: SigningKey) -> str:
    return key.verifying_key.to_string("uncompressed").hex()


def build_payload(sources: dict[str, object]) -> dict[str, object]:
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
        path = (ROOT / relative).resolve()
        if ROOT not in path.parents:
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sources", type=Path, default=DEFAULT_SOURCES)
    parser.add_argument("--private-key", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    sources = load_json(args.sources)
    if not isinstance(sources, dict) or sources.get("schemaVersion") != 1:
        raise SystemExit("unsupported update catalog source schema")
    key = private_key(args)
    expected_public = PUBLIC_KEY.read_text(encoding="ascii").strip().lower()
    if public_hex(key) != expected_public:
        raise SystemExit("signing key does not match the tracked update public key")

    payload = build_payload(sources)
    catalog = (json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
    digest = hashlib.sha256(catalog).hexdigest()
    sequence = int(payload["sequence"])
    stem = f"sgt-component-catalog-v{sequence:06d}-{digest[:16]}"
    signature = key.sign_digest_deterministic(
        hashlib.sha256(catalog).digest(),
        hashfunc=hashlib.sha256,
        sigencode=sigencode_string,
    )

    args.output.mkdir(parents=True, exist_ok=True)
    catalog_path = args.output / f"{stem}.json"
    signature_path = args.output / f"{stem}.sig"
    catalog_path.write_bytes(catalog)
    signature_path.write_bytes(signature)
    manifest = {
        "schemaVersion": 1,
        "sequence": sequence,
        "catalog": {
            "asset": catalog_path.name,
            "sizeBytes": len(catalog),
            "sha256": digest,
        },
        "signature": {
            "asset": signature_path.name,
            "sizeBytes": len(signature),
            "sha256": hashlib.sha256(signature).hexdigest(),
            "algorithm": "ecdsa-p256-sha256-raw",
        },
    }
    (args.output / "sgt-component-catalog.packages.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    print(catalog_path)
    print(signature_path)


if __name__ == "__main__":
    main()
