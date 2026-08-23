#!/usr/bin/env python3
"""Discharge the contracts' verification conditions with solc's model checker.

`verification.toml` records, per contract, which engine proved it and how many
conditions came back safe. This runs the checker and fails unless the result
still matches: a condition that stops being provable is a regression, and a
count that quietly drops is the same regression wearing a smaller number.

Why BMC and not CHC. CHC reasons across transactions and is the stronger
engine, but it does not terminate on DedaloClaim within ten minutes, so it
cannot be a gate. BMC proves each function from an *arbitrary* starting state,
which is weaker than an inductive invariant and much stronger than nothing: it
covers every arithmetic and assertion condition in the contract. That trade is
recorded in verification.toml rather than left for a reader to discover.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGETS = "assert,overflow,underflow,divByZero,popEmptyArray,outOfBounds,balance"

SAFE = re.compile(r"check is safe")
WARNING = re.compile(r"^Warning:", re.MULTILINE)
UNSUPPORTED = re.compile(r"does not yet implement")


def check(solc: str, source: Path, engine: str, timeout_ms: int) -> tuple[int, list[str]]:
    """Return (conditions proved safe, complaints)."""
    result = subprocess.run(
        [
            solc,
            "--model-checker-engine", engine,
            "--model-checker-targets", TARGETS,
            "--model-checker-timeout", str(timeout_ms),
            "--model-checker-show-proved-safe",
            "--model-checker-show-unproved",
            str(source),
        ],
        capture_output=True,
        text=True,
        cwd=ROOT / "contracts",
    )
    output = result.stdout + result.stderr
    if result.returncode != 0:
        return 0, [f"solc exited {result.returncode}:\n{output.strip()}"]

    proved = len(SAFE.findall(output))
    complaints = []

    # An unsupported construct is not a neutral event: the checker stops
    # constraining the surrounding state, so every guard near it becomes
    # invisible and conditions "pass" by not being asked.
    if UNSUPPORTED.search(output):
        complaints.append(
            "the model checker met a construct it does not implement, so it "
            "stopped constraining state around it. Nothing here is proved "
            "until that construct is gone — custom errors were the last one."
        )

    for warning in WARNING.finditer(output):
        line_end = output.find("\n", warning.start())
        complaints.append(output[warning.start(): line_end].strip())

    return proved, complaints


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solc", default="solc", help="path to the solc binary")
    args = parser.parse_args()

    manifest = tomllib.loads((ROOT / "verification.toml").read_text())
    contracts = manifest.get("contracts", {})
    if not contracts:
        print("verification.toml declares no contracts", file=sys.stderr)
        return 1

    failed = False
    for relative, entry in sorted(contracts.items()):
        source = ROOT / "contracts" / relative
        engine = entry.get("engine", "bmc")
        expected = entry["conditions_proved"]

        proved, complaints = check(args.solc, source, engine, 60_000)

        if complaints:
            failed = True
            print(f"FAIL {relative} [{engine}]")
            for complaint in complaints:
                print(f"       {complaint}")
        elif proved < expected:
            failed = True
            print(f"FAIL {relative} [{engine}]: {proved} conditions proved, "
                  f"verification.toml records {expected}")
            print("       a condition stopped being provable, or the contract shrank")
        else:
            if proved > expected:
                failed = True
                print(f"FAIL {relative} [{engine}]: {proved} conditions proved, "
                      f"verification.toml still records {expected}")
                print("       more is good — record it, so the number keeps meaning something")
            else:
                print(f"ok   {relative} [{engine}]: {proved} conditions proved safe")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
