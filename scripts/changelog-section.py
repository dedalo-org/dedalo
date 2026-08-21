#!/usr/bin/env python3
"""Print one release's section from CHANGELOG.md.

Used to fill in GitHub release notes, so the notes and the changelog cannot
drift apart: there is only one text, and it lives in the repository.

    scripts/changelog-section.py 0.1.0 [--changelog CHANGELOG.md]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", help="version to extract, without a leading v")
    parser.add_argument("--changelog", default="CHANGELOG.md", type=Path)
    args = parser.parse_args()

    changelog = args.changelog.read_text()
    version = args.version.lstrip("v")

    # Match both the linked heading git-cliff writes and a plain one.
    heading = re.compile(
        rf"^## \[?{re.escape(version)}\]?.*$",
        re.MULTILINE,
    )
    match = heading.search(changelog)
    if not match:
        print(f"{args.changelog}: no section for {version}", file=sys.stderr)
        return 1

    body_start = match.end()
    next_heading = re.compile(r"^## ", re.MULTILINE).search(changelog, body_start)
    body_end = next_heading.start() if next_heading else len(changelog)

    body = changelog[body_start:body_end]
    # Drop the generated-by footer if this is the last section in the file.
    body = re.sub(r"<!--.*?-->", "", body, flags=re.DOTALL)
    print(body.strip("\n"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
