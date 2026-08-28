#!/usr/bin/env python3
"""Compare the branch ruleset in this repository against the live one.

`.github/rulesets/main.json` exists so the branch policy is *reviewable rather
than remembered*. That only holds while the file and the live ruleset say the
same thing, and nothing made them. They drifted: five rules were enabled
through the web UI and none of them reached the file, two of them blocking
every merge.

The file is authoritative. When this reports a difference, the answer is
normally to re-import the file — not to regenerate it from the live state,
which would launder an unreviewed change into the record.

    scripts/check-ruleset.py                      # compare, exit 1 on drift
    scripts/check-ruleset.py --write              # adopt live into the file

`--write` is for the case where the live change *was* deliberate and the file
is simply behind. It is a separate flag because the two situations look
identical from here and only a person knows which one it is.

Needs `gh` and a token that can read rulesets. Skips, rather than fails, when
neither is available — a gate nobody can run locally is a gate nobody runs.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO = "dedalo-org/dedalo"
FILE = Path(".github/rulesets/main.json")

# GitHub returns these whether or not anybody set them, and an imported file
# that omits them is not different — it is silent. Dropping them from both
# sides is what stops the comparison from reporting a difference that is not
# one.
NOISE = {"dismissal_restriction", "required_reviewers"}


def live_ruleset() -> dict | None:
    """The ruleset as GitHub currently enforces it, or None if unreachable."""
    if shutil.which("gh") is None:
        print("gh not found — skipping the comparison", file=sys.stderr)
        return None

    listing = subprocess.run(
        ["gh", "api", f"repos/{REPO}/rulesets", "--jq", ".[] | select(.name==\"main\") | .id"],
        capture_output=True,
        text=True,
    )
    if listing.returncode != 0 or not listing.stdout.strip():
        print("cannot read rulesets (no token, or none named `main`) — skipping", file=sys.stderr)
        return None

    ruleset_id = listing.stdout.strip().splitlines()[0]
    detail = subprocess.run(
        ["gh", "api", f"repos/{REPO}/rulesets/{ruleset_id}"],
        capture_output=True,
        text=True,
    )
    if detail.returncode != 0:
        print(f"cannot read ruleset {ruleset_id} — skipping", file=sys.stderr)
        return None

    return json.loads(detail.stdout)


def comparable(ruleset: dict) -> dict:
    """Reduce a ruleset to the parts the file is allowed to have an opinion on.

    Ids, timestamps and `_links` describe *this* ruleset object rather than the
    policy, so a file can never match them and should not try.
    """
    rules = []
    for rule in ruleset.get("rules", []):
        parameters = {
            key: value
            for key, value in (rule.get("parameters") or {}).items()
            if key not in NOISE
        }
        entry: dict = {"type": rule["type"]}
        if parameters:
            entry["parameters"] = parameters
        rules.append(entry)

    return {
        "name": ruleset.get("name"),
        "target": ruleset.get("target"),
        "enforcement": ruleset.get("enforcement"),
        "conditions": ruleset.get("conditions"),
        "rules": sorted(rules, key=lambda r: r["type"]),
        "bypass_actors": sorted(
            (
                {k: v for k, v in actor.items() if k != "actor_id"}
                | {"actor_id": actor.get("actor_id")}
                for actor in ruleset.get("bypass_actors", [])
            ),
            key=lambda a: (a.get("actor_type", ""), a.get("actor_id") or 0),
        ),
    }


def describe(label: str, ruleset: dict) -> list[str]:
    """One line per rule, for a diff a person can read."""
    lines = [f"{label}:"]
    for rule in ruleset["rules"]:
        parameters = rule.get("parameters")
        if parameters:
            rendered = ", ".join(f"{k}={json.dumps(v)}" for k, v in sorted(parameters.items()))
            lines.append(f"  {rule['type']}  ({rendered})")
        else:
            lines.append(f"  {rule['type']}")
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="overwrite the file with the live ruleset (only when the live change was deliberate)",
    )
    args = parser.parse_args()

    if not FILE.exists():
        print(f"{FILE} does not exist", file=sys.stderr)
        return 1

    live = live_ruleset()
    if live is None:
        return 0

    on_disk = comparable(json.loads(FILE.read_text()))
    enforced = comparable(live)

    if on_disk == enforced:
        print(f"{FILE} matches the ruleset GitHub enforces ({len(enforced['rules'])} rules)")
        return 0

    if args.write:
        FILE.write_text(json.dumps(enforced, indent=2) + "\n")
        print(f"wrote the live ruleset into {FILE} — commit it with a reason")
        return 0

    print("the ruleset in this repository is not the one GitHub enforces\n", file=sys.stderr)

    on_disk_types = {r["type"] for r in on_disk["rules"]}
    enforced_types = {r["type"] for r in enforced["rules"]}
    for rule_type in sorted(enforced_types - on_disk_types):
        print(f"  enforced, not in the file:  {rule_type}", file=sys.stderr)
    for rule_type in sorted(on_disk_types - enforced_types):
        print(f"  in the file, not enforced:  {rule_type}", file=sys.stderr)

    for label, ruleset in (("in the file", on_disk), ("enforced", enforced)):
        print(file=sys.stderr)
        print("\n".join(describe(label, ruleset)), file=sys.stderr)

    print(
        "\nThe file is authoritative: re-import it under Settings → Rules → Rulesets.\n"
        "If the live change was deliberate, run with --write and commit the result\n"
        "with a message saying what changed and why.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
