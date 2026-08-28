#!/usr/bin/env python3
"""Hold each module to the coverage floor `verification.toml` declares.

Coverage was measured and never enforced: `cargo llvm-cov` ran, uploaded an
lcov file and printed a summary, and failed at nothing. A metric with no
threshold does not hold anything down — on a project whose README argues at
length about the difference between *proved*, *property-tested* and *merely
tested*, that is worse than not measuring.

## Why the floors are per module and not one number

A single global percentage would be satisfied by testing the CLI's table
renderer and would say nothing about `money`. Chasing it upward produces tests
written for the number rather than for the behaviour.

So the floor is declared next to the verification method, per module, in
`verification.toml`:

    [modules.money]
    method = "exhaustive"
    coverage_floor = 100

## Why they live in the manifest

Because the manifest is already the place where "how is this module held down"
is answered, and two files answering it is how they disagree.

## What a missing floor means

Nothing, deliberately. `cli` and `git` are covered by `tests/cli.rs` and by
end-to-end tests against real repositories, and a line-coverage floor there
would reward mocking. A module with no `coverage_floor` is not held to one, and
that is a stated position rather than an omission.

    cargo llvm-cov --workspace --all-features --json --output-path cov.json
    scripts/check-coverage.py --coverage cov.json
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path


def module_of(path: Path, src_root: Path) -> str | None:
    """The manifest key for a source file, or None if it is not one.

    `src/money/mod.rs` is `money`; `src/money/treasury.rs` is
    `money/treasury`. A `proofs.rs` belongs to the module it proves rather than
    being one of its own — it is test code, compiled only under `cfg(test)`,
    and counting it would measure whether the proofs test themselves.
    """
    try:
        relative = path.relative_to(src_root)
    except ValueError:
        return None
    if relative.suffix != ".rs":
        return None

    parts = list(relative.parts)
    if parts[-1] in {"proofs.rs", "main.rs"}:
        return None
    if parts == ["lib.rs"]:
        return "lib"
    if parts[-1] == "mod.rs":
        parts.pop()
    else:
        parts[-1] = parts[-1][: -len(".rs")]
    return "/".join(parts)


def floors(manifest: Path) -> dict[str, int]:
    """Every module that declares a floor, and what it declares."""
    data = tomllib.loads(manifest.read_text())
    return {
        name: entry["coverage_floor"]
        for name, entry in data.get("modules", {}).items()
        if "coverage_floor" in entry
    }


def tests_begin_at(path: Path) -> int:
    """Line where this file's `#[cfg(test)] mod tests` starts, or a sentinel.

    Coverage of a module should measure the module, not its tests. Counting
    test bodies makes the number meaningless in both directions: an
    `#[ignore]`d exhaustive proof reads as dozens of uncovered lines even
    though it is the strongest verification in the file, and a long test module
    that always runs inflates the percentage of code that is barely reached.

    `chain::merkle` is the case that makes this concrete — its exhaustive proof
    over every tree shape to sixty-four is `#[ignore]`d for runtime, and its
    body alone was thirty uncovered lines.
    """
    lines = path.read_text().splitlines()
    for index, line in enumerate(lines):
        if line.strip() != "#[cfg(test)]":
            continue
        # The attribute can sit above a `mod tests` or above a `mod proofs;`
        # declaration; only the former is a body in this file.
        for following in lines[index + 1 :]:
            if not following.strip():
                continue
            if following.startswith("mod tests"):
                return index + 1  # 1-indexed, and the attribute itself is test code
            break
    return 1 << 30


def measured(coverage: Path, src_root: Path) -> dict[str, tuple[int, int]]:
    """Covered and total lines per module, summed over its files.

    Only lines before the file's test module count — see `tests_begin_at`.
    """
    data = json.loads(coverage.read_text())
    totals: dict[str, tuple[int, int]] = {}

    for export in data.get("data", []):
        for file_entry in export.get("files", []):
            path = Path(file_entry["filename"])
            module = module_of(path, src_root)
            if module is None:
                continue

            cutoff = tests_begin_at(path)
            # `segments` is [line, column, count, has_count, is_region_entry, ...].
            # A region entry with a count is one executable place in the code.
            seen: dict[int, int] = {}
            for segment in file_entry.get("segments", []):
                line, _column, count, has_count, is_entry = segment[:5]
                if not has_count or not is_entry or line >= cutoff:
                    continue
                seen[line] = max(seen.get(line, 0), count)

            covered = sum(1 for hits in seen.values() if hits > 0)
            total = len(seen)
            previous = totals.get(module, (0, 0))
            totals[module] = (previous[0] + covered, previous[1] + total)

    return totals


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coverage", type=Path, required=True, help="cargo llvm-cov --json output")
    parser.add_argument("--manifest", type=Path, default=Path("verification.toml"))
    parser.add_argument("--src", type=Path, default=Path("src"))
    args = parser.parse_args()

    declared = floors(args.manifest)
    if not declared:
        print("no module declares a coverage_floor", file=sys.stderr)
        return 1

    src_root = args.src.resolve()
    totals = measured(args.coverage, src_root)

    failures: list[str] = []
    rows: list[tuple[str, float, int, str]] = []

    for module in sorted(declared):
        floor = declared[module]
        if module not in totals:
            failures.append(
                f"{module}: declares a floor of {floor}% and has no measured lines — "
                f"either the module moved or the coverage run did not include it"
            )
            continue

        covered, count = totals[module]
        # A module with no executable lines is vacuously covered. Saying 100%
        # would be a number about nothing.
        percent = 100.0 if count == 0 else covered / count * 100.0
        ok = percent + 1e-9 >= floor
        rows.append((module, percent, floor, "ok" if ok else "UNDER"))
        if not ok:
            missing = count - covered
            failures.append(
                f"{module}: {percent:.2f}% against a floor of {floor}% "
                f"({missing} of {count} lines unreached)"
            )

    width = max(len(row[0]) for row in rows) if rows else 20
    for module, percent, floor, verdict in rows:
        print(f"{module:<{width}}  {percent:6.2f}%  floor {floor:3d}%  {verdict}")

    if failures:
        print(file=sys.stderr)
        for failure in failures:
            print(f"coverage: {failure}", file=sys.stderr)
        print(
            "\nAn unreached line in a module held to 100% is a finding, not a chore:\n"
            "it is either a case the proofs do not cover, dead code to delete, or an\n"
            "`unreachable!()` that should say why it is unreachable.",
            file=sys.stderr,
        )
        return 1

    print(f"\n{len(rows)} module(s) at or above their declared floor")
    return 0


if __name__ == "__main__":
    sys.exit(main())
