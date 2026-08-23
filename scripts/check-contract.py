#!/usr/bin/env python3
"""Build every deployable in verification.toml and check it can be deployed.

A contract that exceeds its chain's size limit is not a contract, and the
limit is on the *compressed* artifact, which is not something you can eyeball
from the source. So it is measured, against the number the manifest declares,
on every run.

This is the whole gate. What the contract *does* is `dedalo::chain::vault`,
which is ordinary Rust and tested with the rest of the money path — the point
of keeping the deployable thin is that there is nothing here left to verify.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def build(crate: Path, target: str) -> Path:
    subprocess.run(
        ["cargo", "build", "--release", "--target", target],
        cwd=crate,
        check=True,
    )
    artifacts = sorted((crate / "target" / target / "release").glob("*.wasm"))
    if not artifacts:
        raise SystemExit(f"{crate}: built nothing for {target}")
    if len(artifacts) > 1:
        raise SystemExit(f"{crate}: {len(artifacts)} artifacts, expected one")
    return artifacts[0]


def compressed_size(raw: bytes) -> int:
    """Bytes after brotli, which is what the chain measures.

    Quality 11 because that is what the chain's own tooling uses; anything
    lower would report a smaller artifact than the one actually submitted, and
    a size gate that flatters the artifact is worse than none.
    """
    if shutil.which("brotli") is None:
        raise SystemExit("brotli is not on PATH; it is what the size limit is measured with")
    result = subprocess.run(
        ["brotli", "-q", "11", "-c"], input=raw, capture_output=True, check=True
    )
    return len(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()

    manifest = tomllib.loads((ROOT / "verification.toml").read_text())
    contracts = manifest.get("contracts", {})
    if not contracts:
        print("verification.toml declares no deployables", file=sys.stderr)
        return 1

    failed = False
    for relative, entry in sorted(contracts.items()):
        crate = ROOT / relative
        target = entry["target"]
        limit = entry["max_compressed_kib"]

        artifact = build(crate, target)
        raw = artifact.read_bytes()
        compressed = compressed_size(raw)

        raw_kib = len(raw) / 1024
        compressed_kib = compressed / 1024
        if compressed_kib > limit:
            failed = True
            print(
                f"FAIL {relative}: {compressed_kib:.1f} KiB compressed, "
                f"limit {limit} KiB — it cannot be deployed"
            )
        else:
            headroom = limit - compressed_kib
            print(
                f"ok   {relative}: {compressed_kib:.1f} KiB compressed "
                f"({raw_kib:.1f} raw), {headroom:.1f} KiB under the limit"
            )

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
