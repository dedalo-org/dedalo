#!/usr/bin/env python3
"""Fold the pending changes into a released CHANGELOG section.

`git cliff` generates entries from Conventional Commit subjects, but a
changelog is a document for people, so anything hand-written under
`## Unreleased` is carried into the release rather than discarded.

    scripts/update-changelog.py <version> [--changelog CHANGELOG.md]

Reads the generated section from stdin (`git cliff --unreleased --strip all`).
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

MARKER = "<!-- New sections are prepended here by `git cliff` during a release. -->"
UNRELEASED = re.compile(r"^## Unreleased\s*$", re.MULTILINE)
ANY_HEADING = re.compile(r"^## ", re.MULTILINE)


def split_unreleased(changelog: str) -> tuple[str, str]:
    """Return the changelog without its Unreleased section, and that section's body."""
    match = UNRELEASED.search(changelog)
    if not match:
        return changelog, ""

    body_start = match.end()
    next_heading = ANY_HEADING.search(changelog, body_start)
    body_end = next_heading.start() if next_heading else len(changelog)
    body = changelog[body_start:body_end].strip("\n")
    remainder = changelog[: match.start()] + changelog[body_end:]
    return remainder, body


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", help="version being released, without a leading v")
    parser.add_argument("--changelog", default="CHANGELOG.md", type=Path)
    args = parser.parse_args()

    generated = sys.stdin.read().strip("\n")
    path: Path = args.changelog
    changelog = path.read_text()

    if MARKER not in changelog:
        print(f"{path}: insertion marker not found", file=sys.stderr)
        return 1

    if f"## [{args.version}]" in changelog:
        print(f"{path}: version {args.version} is already released", file=sys.stderr)
        return 1

    changelog, handwritten = split_unreleased(changelog)

    if generated:
        # The generated heading already carries the version, date and compare
        # link; hand-written notes slot in underneath it.
        section = generated
        if handwritten:
            # Flag the seam so the reviewer of the release pull request folds
            # the two halves together instead of shipping duplicate headings.
            section = (
                f"{section}\n\n"
                "<!-- Carried over from Unreleased: merge these into the "
                "sections above before merging. -->\n\n"
                f"{handwritten}"
            )
    elif handwritten:
        print(
            "no conventional commits since the last tag; "
            "releasing the hand-written notes only",
            file=sys.stderr,
        )
        section = f"## {args.version}\n\n{handwritten}"
    else:
        print("nothing to release: no commits and no hand-written notes", file=sys.stderr)
        return 1

    updated = changelog.replace(MARKER, f"{MARKER}\n\n{section}", 1)
    # Collapse the runs of blank lines splicing can leave behind.
    updated = re.sub(r"\n{4,}", "\n\n\n", updated)
    path.write_text(updated)
    print(f"{path}: added the {args.version} section")
    return 0


if __name__ == "__main__":
    sys.exit(main())
