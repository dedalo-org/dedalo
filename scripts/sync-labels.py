#!/usr/bin/env python3
"""Reconcile the repository's labels with .github/labels.yml.

Written against the `gh` CLI rather than a third-party action: label syncing
needs `issues: write`, and that is not a permission to hand to an unmaintained
dependency.

    scripts/sync-labels.py [--dry-run] [--prune] [--current <json>]

`--current` reads the existing labels from a file instead of calling `gh`,
which is what the unit test uses.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import yaml


def gh(args: list[str], capture: bool = False) -> str:
    result = subprocess.run(
        ["gh", *args], capture_output=capture, text=True, check=True
    )
    return result.stdout if capture else ""


def current_labels(source: Path | None) -> dict[str, dict]:
    if source is not None:
        raw = json.loads(source.read_text())
    else:
        raw = json.loads(gh(["label", "list", "--limit", "200", "--json", "name,color,description"], capture=True))
    return {label["name"]: label for label in raw}


def plan(desired: list[dict], existing: dict[str, dict], prune: bool) -> list[tuple[str, str, dict]]:
    """Return the ordered list of (action, name, label) operations."""
    operations: list[tuple[str, str, dict]] = []
    for label in desired:
        name = label["name"]
        colour = str(label.get("color", "ededed")).lstrip("#").lower()
        description = label.get("description", "")
        found = existing.get(name)
        if found is None:
            operations.append(("create", name, {"color": colour, "description": description}))
        elif (
            str(found.get("color", "")).lstrip("#").lower() != colour
            or (found.get("description") or "") != description
        ):
            operations.append(("update", name, {"color": colour, "description": description}))

    if prune:
        wanted = {label["name"] for label in desired}
        # Deleting a label removes it from every issue that carries it, so
        # this only ever happens when explicitly asked for.
        for name in sorted(existing.keys() - wanted):
            operations.append(("delete", name, {}))
    return operations


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", default=".github/labels.yml", type=Path)
    parser.add_argument("--current", type=Path, default=None)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--prune", action="store_true")
    args = parser.parse_args()

    desired = yaml.safe_load(args.manifest.read_text())
    if not isinstance(desired, list):
        print(f"{args.manifest}: expected a list of labels", file=sys.stderr)
        return 1

    operations = plan(desired, current_labels(args.current), args.prune)
    if not operations:
        print("labels are already in sync")
        return 0

    for action, name, label in operations:
        print(f"{action:>6}  {name}")
        if args.dry_run:
            continue
        if action == "create":
            gh(["label", "create", name, "--color", label["color"], "--description", label["description"]])
        elif action == "update":
            gh(["label", "edit", name, "--color", label["color"], "--description", label["description"]])
        else:
            gh(["label", "delete", name, "--yes"])

    print(f"\n{len(operations)} change(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
