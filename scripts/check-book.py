#!/usr/bin/env python3
"""Structural checks for the handbook.

mdBook renders a broken link exactly as happily as a working one, so the book
would rot silently between releases. These checks are the guard rail:

  * every page listed in SUMMARY.md exists and was rendered;
  * every relative link resolves to a file in the built output;
  * every `#anchor` resolves to a heading that actually exists;
  * no page links to the old `/dedalo/api/` rustdoc tree, which moved to
    docs.rs and is now only a redirect.

It also writes the sitemap, with `--sitemap`. Generating it from the rendered
tree is the only way it stays true: a hand-written one lists the pages somebody
remembered to add.

    scripts/check-book.py [--src book/src] [--built target/book] [--sitemap PATH]
"""

from __future__ import annotations

import argparse
import html.parser
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urldefrag


class Page(html.parser.HTMLParser):
    """Collects the ids and hrefs of one rendered page."""

    def __init__(self) -> None:
        super().__init__()
        self.ids: set[str] = set()
        self.links: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = {k: (v or "") for k, v in attrs}
        if "id" in values:
            self.ids.add(values["id"])
        if tag == "a" and "href" in values:
            self.links.append(values["href"])


def summary_pages(src: Path) -> list[str]:
    """Every markdown path SUMMARY.md points at."""
    text = (src / "SUMMARY.md").read_text(encoding="utf-8")
    return re.findall(r"\]\(([^)]+\.md)\)", text)


def check(src: Path, built: Path, base_path: str = "/dedalo") -> list[str]:
    errors: list[str] = []

    for rel in summary_pages(src):
        if not (src / rel).is_file():
            errors.append(f"SUMMARY.md lists {rel}, which does not exist")
        # mdBook renders `foo/bar.md` to `foo/bar.html`.
        if not (built / rel).with_suffix(".html").is_file():
            errors.append(f"{rel} was not rendered")

    pages = {p.relative_to(built): p for p in built.rglob("*.html")}
    # The print page inlines every chapter, so its duplicate ids and its
    # rewritten links are noise rather than findings.
    pages.pop(Path("print.html"), None)

    anchors: dict[Path, set[str]] = {}
    for rel, path in pages.items():
        parser = Page()
        parser.feed(path.read_text(encoding="utf-8", errors="replace"))
        anchors[rel] = parser.ids
        pages[rel] = parser  # type: ignore[assignment]

    for rel, parser in pages.items():
        for href in parser.links:  # type: ignore[attr-defined]
            if href.startswith(("http://", "https://", "mailto:", "#", "data:")):
                if href.startswith("#"):
                    anchor = unquote(href[1:])
                    if anchor and anchor not in anchors[rel]:
                        errors.append(f"{rel}: #{anchor} matches no heading")
                continue

            target, fragment = urldefrag(unquote(href))
            if not target:
                continue

            if target.startswith("/"):
                # A project page is served under /<repo>/, and the pages that
                # can be reached from any depth — the 404, the redirect — have
                # to link absolutely. Map the prefix back onto the built root.
                prefix = base_path.rstrip("/") + "/"
                if not target.startswith(prefix):
                    errors.append(f"{rel}: {href} leaves {prefix}")
                    continue
                resolved = (built / target[len(prefix):]).resolve()
            else:
                resolved = (built / rel).parent.joinpath(target).resolve()
            try:
                key = resolved.relative_to(built.resolve())
            except ValueError:
                errors.append(f"{rel}: {href} escapes the book")
                continue

            if resolved.is_dir():
                key = key / "index.html"
                resolved = resolved / "index.html"

            if not resolved.is_file():
                errors.append(f"{rel}: {href} does not resolve")
                continue

            if fragment and key in anchors and fragment not in anchors[key]:
                errors.append(f"{rel}: {href} — no such heading in {key}")

    # The API reference lives on docs.rs now. A relative link into `api/`
    # would resolve against a tree this book does not build.
    for rel, parser in pages.items():
        for href in parser.links:  # type: ignore[attr-defined]
            if re.match(r"^(\.\./)*api/", href):
                errors.append(f"{rel}: {href} — the API reference is on docs.rs")

    return errors


def write_sitemap(built: Path, base: str, out: Path) -> int:
    """Emit a sitemap naming every rendered chapter."""
    # The print page is every chapter concatenated, and the 404 is not a page
    # anybody should be sent to from a search result.
    skip = {"print.html", "404.html"}
    urls = sorted(
        p.relative_to(built).as_posix()
        for p in built.rglob("*.html")
        if p.name not in skip
    )

    lines = ['<?xml version="1.0" encoding="UTF-8"?>',
             '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">']
    for url in urls:
        loc = base.rstrip("/") + "/" + ("" if url == "index.html" else url)
        priority = "1.0" if url == "index.html" else "0.6"
        lines += ["  <url>",
                  f"    <loc>{loc}</loc>",
                  "    <changefreq>weekly</changefreq>",
                  f"    <priority>{priority}</priority>",
                  "  </url>"]
    lines.append("</urlset>")

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return len(urls)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=Path, default=Path("book/src"))
    parser.add_argument("--built", type=Path, default=Path("target/book"))
    parser.add_argument(
        "--sitemap",
        type=Path,
        default=None,
        help="write a sitemap of the rendered pages here",
    )
    parser.add_argument(
        "--base-url",
        default="https://dedalo-org.github.io/dedalo/",
        help="URL the book is served under, for the sitemap",
    )
    parser.add_argument(
        "--base-path",
        default="/dedalo",
        help=(
            "path the book is served under. A project page lives at "
            "/<repo>/, and pages reachable from any depth link absolutely."
        ),
    )
    args = parser.parse_args()

    if not args.built.is_dir():
        print(f"{args.built} does not exist — run `mdbook build book` first", file=sys.stderr)
        return 2

    errors = check(args.src, args.built, args.base_path)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)

    pages = len(list(args.built.rglob("*.html")))
    if errors:
        print(f"\n{len(errors)} problem(s) across {pages} pages", file=sys.stderr)
        return 1

    print(f"book checked: {pages} pages, every link and anchor resolves")

    if args.sitemap:
        count = write_sitemap(args.built, args.base_url, args.sitemap)
        print(f"sitemap written: {count} urls -> {args.sitemap}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
