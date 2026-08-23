#!/usr/bin/env python3
"""Structural checks for the published site.

The site is hand-written HTML with an inline stylesheet, which is the right
trade-off for a page with no build step — but it means an editing slip can
ship silently. These checks are the guard rail: well-formed markup, balanced
CSS, every link resolving, and the metadata the page claims to have.

    scripts/check-site.py [--root site] [--built _site]
"""

from __future__ import annotations

import argparse
import html.parser
import re
import sys
from pathlib import Path


class Structure(html.parser.HTMLParser):
    """Tracks tag nesting and collects ids, links and meta tags."""

    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input",
            "link", "meta", "source", "track", "wbr"}

    def __init__(self) -> None:
        super().__init__()
        self.stack: list[tuple[str, int]] = []
        self.errors: list[str] = []
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.metas: list[dict[str, str]] = []
        self.rel_links: list[dict[str, str]] = []
        self.has_title = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = {k: (v or "") for k, v in attrs}
        if "id" in values:
            self.ids.add(values["id"])
        if tag == "a" and "href" in values:
            self.links.append(values["href"])
        if tag == "meta":
            self.metas.append(values)
        if tag == "link":
            self.rel_links.append(values)
        if tag == "title":
            self.has_title = True
        if tag not in self.VOID:
            self.stack.append((tag, self.getpos()[0]))

    def handle_endtag(self, tag: str) -> None:
        if tag in self.VOID:
            return
        if not self.stack:
            self.errors.append(f"line {self.getpos()[0]}: stray </{tag}>")
        elif self.stack[-1][0] == tag:
            self.stack.pop()
        else:
            open_tag, line = self.stack[-1]
            self.errors.append(
                f"line {self.getpos()[0]}: </{tag}> closes <{open_tag}> opened on line {line}"
            )
            self.stack.pop()


def check_css(text: str, errors: list[str]) -> None:
    """Balance braces, and reject declarations sitting directly in an at-rule.

    A declaration is legal inside a style rule and illegal inside `@media` or
    `@supports`, where only rules may appear. Telling the two apart needs the
    kind of each open block, not just the nesting depth.
    """
    declaration = re.compile(r"^-{0,2}[a-z][a-z0-9-]*\s*:\s*[^;{}]+;$")

    for block in re.findall(r"<style[^>]*>(.*?)</style>", text, re.DOTALL):
        stripped = re.sub(r"/\*.*?\*/", "", block, flags=re.DOTALL)
        # Kind of every currently open block: True when opened by an at-rule.
        open_blocks: list[bool] = []
        pending = ""

        for line_number, raw in enumerate(stripped.split("\n"), 1):
            line = raw.strip()
            if not line:
                continue

            if open_blocks and open_blocks[-1] and "{" not in line and "}" not in line:
                if declaration.match(line):
                    errors.append(
                        f"css line {line_number}: `{line}` is a declaration "
                        "directly inside an at-rule, where only rules are legal"
                    )

            for char in raw:
                if char == "{":
                    open_blocks.append(pending.strip().startswith("@"))
                    pending = ""
                elif char == "}":
                    if not open_blocks:
                        errors.append(f"css line {line_number}: unbalanced closing brace")
                    else:
                        open_blocks.pop()
                    pending = ""
                elif char == ";":
                    pending = ""
                else:
                    pending += char

        if open_blocks:
            errors.append(f"css: {len(open_blocks)} unclosed block(s)")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("site"))
    parser.add_argument(
        "--external-prefix",
        action="append",
        default=[],
        metavar="PATH",
        help=(
            "an absolute path prefix served by another deployment on this "
            "domain, e.g. /dedalo/ — links under it are not resolved locally"
        ),
    )
    parser.add_argument(
        "--built",
        type=Path,
        default=None,
        help="assembled tree, if links should resolve against it",
    )
    parser.add_argument(
        "--base-path",
        default="/dedalo",
        help=(
            "path the site is served under. A project page lives at "
            "/<repo>/, and the 404 page has to use absolute links because it "
            "is served from any depth."
        ),
    )
    args = parser.parse_args()

    errors: list[str] = []
    pages = sorted(args.root.glob("*.html"))
    if not pages:
        print(f"{args.root}: no pages found", file=sys.stderr)
        return 1

    for page in pages:
        text = page.read_text()
        structure = Structure()
        structure.feed(text)
        structure.close()

        for error in structure.errors:
            errors.append(f"{page}: {error}")
        for tag, line in structure.stack:
            errors.append(f"{page}: <{tag}> opened on line {line} is never closed")
        if not structure.has_title:
            errors.append(f"{page}: no <title>")

        check_css(text, errors)

        names = {m.get("name", "") for m in structure.metas}
        equivs = {m.get("http-equiv", "") for m in structure.metas}
        if page.name == "index.html":
            for required in ("description", "theme-color"):
                if required not in names:
                    errors.append(f"{page}: missing <meta name={required}>")
            if "Content-Security-Policy" not in equivs:
                errors.append(f"{page}: missing a Content-Security-Policy")
            # The page promises it makes no external requests. Prove it.
            if re.search(r"<script", text, re.IGNORECASE):
                errors.append(f"{page}: contains a script tag, which the CSP forbids")
            if re.search(r'\ssrc="https?://', text, re.IGNORECASE):
                errors.append(f"{page}: loads a remote resource, which the CSP forbids")
            for link in structure.rel_links:
                # `canonical` and `alternate` are metadata, not fetches.
                if link.get("rel", "") in {"canonical", "alternate"}:
                    continue
                if link.get("href", "").startswith(("http://", "https://")):
                    errors.append(
                        f"{page}: <link rel={link.get('rel', '?')}> points at a remote host"
                    )

        for href in structure.links:
            if href.startswith(("http://", "https://", "mailto:", "data:")):
                continue
            if href.startswith("#"):
                if href[1:] and href[1:] not in structure.ids:
                    errors.append(f"{page}: `{href}` points at no element on the page")
                continue
            if any(href.startswith(prefix) for prefix in args.external_prefix):
                # Served by a different deployment of the same domain, so it
                # cannot be resolved here. Declared rather than guessed: a
                # typo in one of these is still a broken link, and silently
                # skipping every absolute path would hide it.
                continue
            if args.built is not None:
                relative = href
                if relative.startswith("/"):
                    base = args.base_path.strip("/")
                    if base and not relative.lstrip("/").startswith(base):
                        errors.append(
                            f"{page}: absolute link `{href}` is missing the "
                            f"/{base} base path the site is served under"
                        )
                        continue
                    relative = relative.lstrip("/")[len(base):]
                target = (args.built / relative.lstrip("/")).resolve()
                if not any(c.exists() for c in (target, target / "index.html")):
                    errors.append(f"{page}: `{href}` does not resolve in the built tree")

    if errors:
        print(f"{len(errors)} problem(s):", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    print(f"{len(pages)} page(s) checked, no problems")
    return 0


if __name__ == "__main__":
    sys.exit(main())
