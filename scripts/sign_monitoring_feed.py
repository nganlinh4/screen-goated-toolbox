#!/usr/bin/env python3
"""Sign the monitoring availability feed with the monitoring P-256 key.

Deliberately separate from the update-catalog key. That key signs runtime bundles
and is used by an attended workflow; this one is used unattended every two hours
and only influences model routing, so a compromise here must not reach anything
that delivers executables.

The wire format matches the update catalog so the client can reuse its existing
verification: a raw 64-byte P-256 signature over the SHA-256 digest of the feed
bytes, written beside the feed as `<name>.sig`.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import os
from pathlib import Path

from ecdsa import NIST256p, SigningKey
from ecdsa.util import sigencode_string

ENV_PRIVATE_KEY = "SGT_MONITORING_P256_PRIVATE_KEY_PEM_BASE64"


def signing_key() -> SigningKey:
    encoded = os.environ.get(ENV_PRIVATE_KEY, "")
    if not encoded:
        raise SystemExit(f"{ENV_PRIVATE_KEY} is unavailable")
    try:
        pem = base64.b64decode(encoded, validate=True)
    except Exception as error:  # noqa: BLE001 - surfaced verbatim to the operator
        raise SystemExit(f"{ENV_PRIVATE_KEY} is not valid base64") from error
    try:
        key = SigningKey.from_pem(pem)
    except Exception as error:  # noqa: BLE001
        raise SystemExit(f"{ENV_PRIVATE_KEY} is not a valid PEM key") from error
    if key.curve != NIST256p:
        raise SystemExit("monitoring signing key must be P-256")
    return key


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--public-key", type=Path, required=True,
                        help="tracked public key the signature must match")
    args = parser.parse_args()

    key = signing_key()
    tracked = args.public_key.read_text(encoding="utf-8").strip().lower()
    actual = key.get_verifying_key().to_string().hex()
    if tracked != actual:
        raise SystemExit("signing key does not match the tracked monitoring public key")

    payload = args.input.read_bytes()
    signature = key.sign_digest_deterministic(
        hashlib.sha256(payload).digest(),
        hashfunc=hashlib.sha256,
        sigencode=sigencode_string,
    )
    if len(signature) != 64:
        raise SystemExit("unexpected signature length")
    args.input.with_suffix(args.input.suffix + ".sig").write_bytes(signature)
    print(f"signed {args.input} ({len(payload)} bytes)")


if __name__ == "__main__":
    main()
