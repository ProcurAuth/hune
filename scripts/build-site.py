#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Build the static website in docs/ from the Markdown files. Use only the Python standard library.

Use:
    python3 -I scripts/build-site.py            write the pages
    python3 -I scripts/build-site.py --check    make no change; exit 1 when a page differs from its source

The Markdown files in docs/ and the file CHANGELOG.md are the source of the pages. The Overview page and the page 404 are
in this script. The pages need no build step to be read: they are plain HTML with relative links, and they work from
file:// and from GitHub Pages.
"""

from __future__ import annotations

import html
import posixpath
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
SITE_URL = "https://procurauth.github.io/hune/"
REPO_URL = "https://github.com/ProcurAuth/hune"
BLOB = REPO_URL + "/blob/main/"
TREE = REPO_URL + "/tree/main/"


def version() -> str:
    """Return the version of the crate."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.M)
    if not match:
        raise SystemExit("Cargo.toml has no version")
    return match.group(1)


VERSION = version()

# name of the page, title, short title (navigation), source files (relative to the root), eyebrow, lede, in the navigation
PAGES = [
    ("index.html", "Hune", "Overview", None,
     "Dashboards over ClickHouse, PostgreSQL and MongoDB", "", True),
    ("getting-started.html", "Getting started", "Getting started", "docs/GETTING-STARTED.md",
     "A test in five minutes",
     "Run a local ClickHouse, load the demo data, and open the demo dashboards in Hune.", True),
    ("configuration.html", "Configuration", "Configuration", "docs/CONFIGURATION.md",
     "One HCL file, a closed schema",
     "The blocks, the rules and the commands of the configuration. A mistake stops the start, and the message names the key.", True),
    ("architecture.html", "Architecture", "Architecture", "docs/ARCHITECTURE.md",
     "How the single binary works",
     "The server, the query flow, the stores, the data sources and the limits.", True),
    ("access.html", "Access and ProcurAuth", "Access", "docs/ACCESS.md",
     "The mode authzen",
     "How Hune checks the identity token, asks the PDP and applies the decision: the resource tree, the actions, the ceilings and the errors.", True),
    ("security.html", "Security model", "Security", "docs/SECURITY-MODEL.md",
     "What Hune protects, and how",
     "The protections of the query engine, the stores, the page and the data source connections. To report a vulnerability, read the security policy.", True),
    ("operations.html", "Operations", "Operations", "docs/OPERATIONS.md",
     "Run it in production",
     "The service, the stores, the logs and metrics, the limits, the upgrade and the steps to find a problem.", True),
    ("specification.html", "Specification", "Specification", "docs/SPECIFICATION.md",
     "The public specification",
     "The eight parts of the specification of Hune: the format of a document, the API, the query engine, the page, the access rules, the configuration and the tests.", True),
    ("reference.html", "Configuration reference", "Reference", "docs/configuration-reference.md",
     "Generated from the code",
     "Each key with its type, its range and its forms. The program makes this page from its table of keys.", False),
    ("changelog.html", "Changelog", "Changelog", "CHANGELOG.md",
     "The changes of each version", "The versions of Hune, with the changes and the upgrade steps.", False),
    ("status.html", "Implementation status", "Status", "docs/implementation-status.md",
     "What exists, and what does not",
     "The state of each milestone, the differences from the specification and the known limits.", False),
    ("development.html", "Development", "Development", "docs/DEVELOPMENT.md",
     "Build, test and release",
     "The checks, the tests, the dependencies and the steps of a release.", False),
    ("conformance.html", "Conformance", "Conformance", "docs/CONFORMANCE.md",
     "The test of the ProcurAuth runner",
     "How to run the conformance cases of ProcurAuth against Hune, and what the result is.", False),
    ("engine.html", "Chart engine edits", "Chart engine", "docs/engine-patches.md",
     "The copied chart engine",
     "The edits that Hune makes in its copy of the chart engine of ClickHouse Dash.", False),
    ("spec-01.html", "Specification, part 1: Overview", "Part 1", "docs/01-overview.md",
     "Part 1 of the specification", "Purpose, scope, decisions and architecture.", False),
    ("spec-02.html", "Specification, part 2: Document format and storage", "Part 2", "docs/02-format-and-storage.md",
     "Part 2 of the specification", "The JSON format of a dashboard and the two stores.", False),
    ("spec-03.html", "Specification, part 3: API, query engine and data sources", "Part 3", "docs/03-api-query-sources.md",
     "Part 3 of the specification", "The HTTP API, the query engine and the three kinds of data source.", False),
    ("spec-04.html", "Specification, part 4: Front end and editor", "Part 4", "docs/04-frontend-editor.md",
     "Part 4 of the specification", "The page, the panels, the editor, the history and the conflicts.", False),
    ("spec-05.html", "Specification, part 5: Access and security", "Part 5", "docs/05-access-security.md",
     "Part 5 of the specification", "The access rules, ProcurAuth and the security rules.", False),
    ("spec-06.html", "Specification, part 6: Configuration and operation", "Part 6", "docs/06-configuration-operation.md",
     "Part 6 of the specification", "The configuration, the observability and the operation.", False),
    ("spec-07.html", "Specification, part 7: Migration, tests and delivery", "Part 7", "docs/07-migration-tests-delivery.md",
     "Part 7 of the specification", "The import from Grafana, the tests and the delivery.", False),
    ("spec-08.html", "Specification, part 8: Roadmap and open questions", "Part 8", "docs/08-roadmap-open-items.md",
     "Part 8 of the specification", "The roadmap, the open questions, the glossary and the references.", False),
]
PAGE_OF_SOURCE = {p[3]: p[0] for p in PAGES if p[3]}


# ------------------------------------------------------------------ Markdown

def github_slug(heading: str) -> str:
    """Make the anchor that GitHub gives to a Markdown heading."""
    text = re.sub(r"`([^`]*)`", r"\1", heading)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]+>", "", text).strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def map_link(url: str, source_dir: str) -> str:
    """Map the target of a link in a Markdown file to the target in the website."""
    if re.match(r"^[a-z][a-z0-9+.-]*:", url) or url.startswith("//"):
        return url
    if url.startswith("#"):
        return url
    path, _, fragment = url.partition("#")
    target = posixpath.normpath(posixpath.join(source_dir, path))
    suffix = "#" + fragment if fragment else ""
    if target in PAGE_OF_SOURCE:
        return PAGE_OF_SOURCE[target] + suffix
    if target.startswith("docs/") and target.endswith(".html"):
        return target[len("docs/"):] + suffix
    if (ROOT / target).is_dir():
        return TREE + target.rstrip("/") + suffix
    return BLOB + target + suffix


class Inline:
    """Render the inline Markdown of the documents: code, links, images, emphasis."""

    def __init__(self, source_dir: str) -> None:
        self.source_dir = source_dir

    def render(self, text: str) -> str:
        holders: list[str] = []

        def keep_code(match: re.Match[str]) -> str:
            holders.append("<code>" + html.escape(match.group(2), quote=False) + "</code>")
            return f"\x00{len(holders) - 1}\x00"

        text = re.sub(r"(`+)(.+?)\1", keep_code, text)
        text = html.escape(text, quote=False)
        text = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)\)", lambda m: f'<img src="{html.escape(m.group(2))}" alt="{m.group(1)}" loading="lazy">', text)
        text = re.sub(
            r"\[([^\]]+)\]\(([^)\s]+)(?:\s+&quot;[^&]*&quot;)?\)",
            lambda m: f'<a href="{html.escape(map_link(html.unescape(m.group(2)), self.source_dir))}">{m.group(1)}</a>',
            text,
        )
        text = re.sub(r"&lt;(https?://[^\s&]+)&gt;", r'<a href="\1">\1</a>', text)
        text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
        text = re.sub(r"(?<![\w*])\*(?![\s*])(.+?)(?<![\s*])\*(?![\w*])", r"<em>\1</em>", text)
        return re.sub(r"\x00(\d+)\x00", lambda m: holders[int(m.group(1))], text)


def split_row(line: str) -> list[str]:
    """Split a table row at the pipes that are not in a code span."""
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    cells: list[str] = []
    current = ""
    in_code = False
    for char in line:
        if char == "`":
            in_code = not in_code
        if char == "|" and not in_code:
            cells.append(current.strip())
            current = ""
        else:
            current += char
    cells.append(current.strip())
    return cells


class Markdown:
    """A small Markdown renderer for the documents of this repository."""

    def __init__(self, source_dir: str) -> None:
        self.inline = Inline(source_dir)
        self.headings: list[tuple[int, str, str]] = []
        self.slugs: dict[str, int] = {}
        self.title = ""

    def heading_id(self, text: str) -> str:
        slug = github_slug(text)
        count = self.slugs.get(slug, 0)
        self.slugs[slug] = count + 1
        return slug if count == 0 else f"{slug}-{count}"

    def render(self, text: str) -> str:
        lines = text.split("\n")
        out: list[str] = []
        i = 0
        paragraph: list[str] = []

        def flush() -> None:
            if paragraph:
                out.append("<p>" + self.inline.render(" ".join(s.strip() for s in paragraph)) + "</p>")
                paragraph.clear()

        while i < len(lines):
            line = lines[i]
            stripped = line.strip()
            if not stripped:
                flush()
                i += 1
                continue
            fence = re.match(r"^```(\S*)\s*$", stripped)
            if fence:
                flush()
                language = fence.group(1)
                body: list[str] = []
                i += 1
                while i < len(lines) and not lines[i].strip().startswith("```"):
                    body.append(lines[i])
                    i += 1
                i += 1
                out.append(self.code_block(language, "\n".join(body)))
                continue
            heading = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
            if heading:
                flush()
                level = len(heading.group(1))
                content = heading.group(2)
                if level == 1 and not self.title:
                    self.title = re.sub(r"`", "", content)
                    i += 1
                    continue
                identifier = self.heading_id(content)
                self.headings.append((level, re.sub(r"`", "", content), identifier))
                rendered = self.inline.render(content)
                if level == 2:
                    out.append(f'<h2 id="{identifier}"><a class="self" href="#{identifier}">{rendered}</a></h2>')
                else:
                    out.append(f'<h{level} id="{identifier}">{rendered}</h{level}>')
                i += 1
                continue
            if stripped.startswith("|") and i + 1 < len(lines) and re.match(r"^\s*\|?\s*:?-{3,}", lines[i + 1]):
                flush()
                rows: list[str] = []
                while i < len(lines) and lines[i].strip().startswith("|"):
                    rows.append(lines[i])
                    i += 1
                out.append(self.table(rows))
                continue
            if re.match(r"^\s*([-*]|\d+\.)\s+", line):
                flush()
                block, i = self.list_block(lines, i)
                out.append(block)
                continue
            if re.match(r"^-{3,}$", stripped):
                flush()
                out.append("<hr>")
                i += 1
                continue
            paragraph.append(line)
            i += 1
        flush()
        return "\n".join(out)

    def code_block(self, language: str, body: str) -> str:
        label = language or "text"
        escaped = html.escape(body, quote=False)
        return (
            f'<div class="code-wrap"><span class="label">{html.escape(label)}</span>'
            f'<pre data-copy tabindex="0" aria-label="Code example: {html.escape(label)}"><code>{escaped}</code></pre></div>'
        )

    def table(self, rows: list[str]) -> str:
        head = split_row(rows[0])
        body = [split_row(r) for r in rows[2:]]
        labels = [re.sub(r"<[^>]+>", "", self.inline.render(h)) for h in head]
        parts = ['<div class="tablewrap"><table><thead><tr>']
        for cell in head:
            parts.append(f'<th scope="col">{self.inline.render(cell)}</th>')
        parts.append("</tr></thead><tbody>")
        for cells in body:
            parts.append("<tr>")
            for index, cell in enumerate(cells):
                rendered = self.inline.render(cell)
                if index == 0:
                    parts.append(f'<th scope="row">{rendered}</th>')
                else:
                    label = html.escape(labels[index] if index < len(labels) else "", quote=True)
                    parts.append(f'<td data-label="{label}">{rendered}</td>')
            parts.append("</tr>")
        parts.append("</tbody></table></div>")
        return "".join(parts)

    def list_block(self, lines: list[str], i: int) -> tuple[str, int]:
        marker = re.match(r"^(\s*)([-*]|\d+\.)\s+", lines[i])
        assert marker
        indent = len(marker.group(1))
        ordered = marker.group(2)[0].isdigit()
        tag = "ol" if ordered else "ul"
        items: list[str] = []
        while i < len(lines):
            m = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)$", lines[i])
            if not m or len(m.group(1)) != indent:
                break
            text_lines = [m.group(3)]
            children: list[str] = []
            i += 1
            while i < len(lines):
                nxt = lines[i]
                if not nxt.strip():
                    # A blank line ends the item unless an indented line continues it.
                    j = i + 1
                    while j < len(lines) and not lines[j].strip():
                        j += 1
                    if j < len(lines) and len(lines[j]) - len(lines[j].lstrip()) > indent and not re.match(r"^\s*([-*]|\d+\.)\s+", lines[j]):
                        i = j
                        text_lines.append("")
                        continue
                    break
                nm = re.match(r"^(\s*)([-*]|\d+\.)\s+", nxt)
                lead = len(nxt) - len(nxt.lstrip())
                if nm and len(nm.group(1)) > indent:
                    block, i = self.list_block(lines, i)
                    children.append(block)
                    continue
                if nm and len(nm.group(1)) <= indent:
                    break
                if lead > indent:
                    text_lines.append(nxt.strip())
                    i += 1
                    continue
                break
            paragraphs: list[list[str]] = [[]]
            for text_line in text_lines:
                if text_line == "":
                    paragraphs.append([])
                else:
                    paragraphs[-1].append(text_line)
            texts = [" ".join(p).strip() for p in paragraphs if p]
            if len(texts) > 1:
                body = "".join("<p>" + self.inline.render(t) + "</p>" for t in texts)
            else:
                body = self.inline.render(texts[0] if texts else "")
            items.append("<li>" + body + "".join(children) + "</li>")
            # a blank line between two items keeps the same list
            j = i
            while j < len(lines) and not lines[j].strip():
                j += 1
            if j < len(lines) and j != i:
                nm2 = re.match(r"^(\s*)([-*]|\d+\.)\s+", lines[j])
                if nm2 and len(nm2.group(1)) == indent:
                    i = j
        return f"<{tag}>" + "".join(items) + f"</{tag}>", i


# ----------------------------------------------------------------- templates

MARK = (
    '<svg class="mark" viewBox="0 0 32 32" aria-hidden="true" focusable="false"><rect width="32" height="32" rx="8" fill="var(--accent)"/>'
    '<path d="M6 21l5-6 4 3 5-8 6 7" fill="none" stroke="var(--accent-ink)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>'
    '<circle cx="26" cy="24" r="2" fill="#34d399"/></svg>'
)
GITHUB_ICON = (
    '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path fill="currentColor" d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>'
)
THEME_ICON = (
    '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" stroke-width="2"/>'
    '<path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor"/></svg>'
)
DEFAULT_DESCRIPTION = (
    "Hune is a dashboard server for ClickHouse, PostgreSQL and MongoDB. It is one static binary with one HCL file, "
    "and a viewer never sends SQL."
)


def head(name: str, title: str, description: str) -> str:
    page_title = "Hune: dashboards over ClickHouse, PostgreSQL and MongoDB" if name == "index.html" else f"{title} | Hune"
    canonical = SITE_URL if name == "index.html" else SITE_URL + name
    e = html.escape
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="description" content="{e(description)}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Hune">
<meta property="og:title" content="{e(page_title)}">
<meta property="og:description" content="{e(description)}">
<meta property="og:url" content="{canonical}">
<meta property="og:image" content="{SITE_URL}assets/og.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="Hune: dashboards over ClickHouse, PostgreSQL and MongoDB. One static binary.">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{e(page_title)}">
<meta name="twitter:description" content="{e(description)}">
<meta name="twitter:image" content="{SITE_URL}assets/og.png">
<meta name="twitter:image:alt" content="Hune: dashboards over ClickHouse, PostgreSQL and MongoDB.">
<link rel="canonical" href="{canonical}">
<title>{e(page_title)}</title>
<link rel="icon" href="assets/favicon.svg" type="image/svg+xml">
<link rel="icon" href="assets/favicon-32.png" sizes="32x32" type="image/png">
<link rel="icon" href="assets/favicon.ico" sizes="16x16 32x32 48x48">
<link rel="apple-touch-icon" href="assets/apple-touch-icon.png">
<link rel="manifest" href="site.webmanifest">
<meta name="theme-color" content="#fafafd" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#0c0e15" media="(prefers-color-scheme: dark)">
<link rel="stylesheet" href="assets/site.css">
<script src="assets/site.js"></script>
</head>
<body>
"""


def header(current: str) -> str:
    items = []
    for name, _title, short, _src, _eyebrow, _lede, nav in PAGES:
        if not nav:
            continue
        aria = ' aria-current="page"' if name == current else ""
        items.append(f'<li><a href="{name}"{aria}>{short}</a></li>')
    items.append(f'<li class="nav-gh"><a href="{REPO_URL}">GitHub</a></li>')
    return f"""<a class="skip" href="#main">Skip to the content</a>
<header class="site-header">
  <div class="wrap bar">
    <a class="brand" href="index.html" aria-label="Hune, home page">{MARK}<span>Hun<b>e</b></span></a>
    <button class="menu-btn" id="menu-btn" type="button" aria-expanded="false" aria-controls="site-nav">Menu</button>
    <nav id="site-nav" aria-label="Main"><ul>{"".join(items)}</ul></nav>
    <a class="gh-link" href="{REPO_URL}" aria-label="Hune on GitHub" title="Hune on GitHub">{GITHUB_ICON}<span>GitHub</span></a>
    <button class="icon-btn" id="theme-btn" type="button" aria-label="Switch colour theme" title="Switch colour theme">{THEME_ICON}</button>
  </div>
</header>
<main id="main">
"""


def footer() -> str:
    return f"""</main>
<footer class="site-footer">
  <div class="wrap">
    <div class="foot-grid">
      <div>
        <h2>Hune</h2>
        <p>Hune is a dashboard server for ClickHouse, PostgreSQL and MongoDB. Version {VERSION}.</p>
      </div>
      <div>
        <h2>Learn</h2>
        <ul>
          <li><a href="getting-started.html">Getting started</a></li>
          <li><a href="configuration.html">Configuration</a></li>
          <li><a href="architecture.html">Architecture</a></li>
          <li><a href="specification.html">Specification</a></li>
        </ul>
      </div>
      <div>
        <h2>Run</h2>
        <ul>
          <li><a href="access.html">Access and ProcurAuth</a></li>
          <li><a href="security.html">Security model</a></li>
          <li><a href="operations.html">Operations</a></li>
          <li><a href="reference.html">Configuration reference</a></li>
        </ul>
      </div>
      <div>
        <h2>Project</h2>
        <ul>
          <li><a href="{REPO_URL}">Repository</a></li>
          <li><a href="{REPO_URL}/releases/latest">Downloads</a></li>
          <li><a href="changelog.html">Changelog</a></li>
          <li><a href="status.html">Status</a></li>
          <li><a href="development.html">Development</a></li>
          <li><a href="conformance.html">Conformance</a></li>
          <li><a href="https://procurauth.github.io/procurauth/">ProcurAuth</a></li>
        </ul>
      </div>
    </div>
    <div class="foot-legal">
      <p>Hune, version {VERSION}.</p>
      <p>Licence: <a href="{BLOB}LICENSE-MIT">MIT</a> or <a href="{BLOB}LICENSE-APACHE">Apache-2.0</a>.</p>
      <p><a href="{BLOB}SECURITY.md">Security policy</a></p>
    </div>
  </div>
</footer>
</body>
</html>
"""


def doc_page(name: str, title: str, eyebrow: str, lede: str, source: str) -> str:
    path = ROOT / source
    text = path.read_text(encoding="utf-8")
    md = Markdown(posixpath.dirname(source))
    body = md.render(text)
    toc = "".join(f'<li><a href="#{ident}">{html.escape(label)}</a></li>' for level, label, ident in md.headings if level == 2)
    description = lede
    h1_title = title
    lede_html = f'<p class="lede">{html.escape(lede)}</p>' if lede else ""
    edit = f'<p class="src">This page comes from <a href="{BLOB}{source}"><code>{source}</code></a> in the repository.</p>'
    toc_html = (
        f'<nav class="toc" aria-label="On this page"><details class="toc-details" open><summary>On this page</summary><ol>{toc}</ol></details></nav>'
        if toc else ""
    )
    layout = "wrap doc-layout" if toc else "wrap narrow doc-layout"
    return (
        head(name, title, description)
        + header(name)
        + f'<div class="page-head"><div class="wrap"><p class="eyebrow"><span class="badge">Version {VERSION}</span> {html.escape(eyebrow)}</p>'
        + f"<h1>{html.escape(h1_title)}</h1>{lede_html}</div></div>"
        + f'<div class="{layout}">{toc_html}<article class="doc">{edit}{body}</article></div>\n'
        + footer()
    )


def figure(src: str, alt: str, caption: str, extra: str = "") -> str:
    return (
        f'<figure class="shot{extra}"><img src="assets/img/{src}" alt="{html.escape(alt)}" loading="lazy" decoding="async">'
        f"<figcaption>{caption}</figcaption></figure>"
    )


def panel(src: str, alt: str, caption: str) -> str:
    return (
        f'<figure class="panel"><img src="assets/{src}" alt="{html.escape(alt)}" loading="lazy" decoding="async">'
        f"<figcaption>{caption}</figcaption></figure>"
    )


QUICKSTART = """docker run -d --rm --name hune-demo-ch -p 127.0.0.1:8123:8123 clickhouse/clickhouse-server:26.8.34.13
curl -fsSLO https://github.com/ProcurAuth/hune/releases/download/v%(v)s/hune_%(v)s_linux_amd64.tar.gz
tar -xzf hune_%(v)s_linux_amd64.tar.gz
cd hune_%(v)s_linux_amd64
./hune serve -c hune.hcl"""


def overview() -> str:
    name = "index.html"
    description = DEFAULT_DESCRIPTION
    cards = [
        ("M4 19V9M10 19V5M16 19v-8M22 19V3", "Dashboards as documents",
         "A dashboard is one JSON document with a closed schema. The server checks it, keeps its history and gives it a version tag.",
         "architecture.html#the-document-and-the-stores"),
        ("M12 3v18M5 8l7-5 7 5M5 16l7 5 7-5", "SQL stays on the server",
         "The server builds each query from the saved document. A viewer sends no SQL. Each source has limits, a time budget and a cancel.",
         "architecture.html#the-query-flow"),
        ("M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z", "Access with ProcurAuth",
         "In the mode authzen, Hune checks an identity token and asks a PDP before each operation. A grant on a folder covers its dashboards.",
         "access.html"),
    ]
    card_html = "".join(
        f'<a class="card" href="{href}"><div class="ico"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" '
        f'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="{path}"/></svg></div><h3>{title}</h3><p>{text}</p></a>'
        for path, title, text, href in cards
    )
    features = [
        ("Data sources", "ClickHouse, PostgreSQL and MongoDB. Each kind has a closed check of the query text, limits, a cancel and a probe."),
        ("Panels", "Table, time series, stat, bar chart, pie chart, XY chart, bar gauge, gauge, text, state timeline, status history, heatmap, histogram, logs and map."),
        ("Variables", "Query, custom, text box, interval, constant and ad hoc filters. A variable can repeat a panel or a row."),
        ("Editor", "Forms for each panel type, a query editor with a syntax tree, a history of versions, a conflict screen and restore."),
        ("Stores", "A folder of JSON files (a volume) or a PostgreSQL database. Commands move documents from one store to the other."),
        ("Grafana", "The command import-grafana reads the dashboards of a Grafana export, with a report, an acceptance file and an nginx map."),
        ("Access", "The mode authzen checks an identity token and asks a PDP. The mode open needs a proxy that signs users in."),
        ("Configuration", "One HCL file with a closed schema. A mistake stops the start. The command print-config shows each value and its source."),
        ("Operation", "Logs, metrics, audit lines, an OTLP exporter of traces and a second port for the operation routes."),
        ("Page", "Plain JavaScript, HTML and CSS with no framework and no build step. A light theme and a dark theme. No request goes to another origin."),
    ]
    rows = "".join(f'<tr><th scope="row">{a}</th><td data-label="What Hune does">{b}</td></tr>' for a, b in features)
    shots = "".join([
        figure("hero.webp", "A dashboard in the light theme: stat panels, a time series, a table and a bar chart", "<strong>Dashboards.</strong> Stat panels, time series, tables and charts over a ClickHouse source."),
        figure("dashboard-dark.webp", "The same dashboard in the dark theme", "<strong>Two themes.</strong> The page has a light theme and a dark theme."),
        figure("charts-light.webp", "A dashboard of charts in the light theme", "<strong>Charts.</strong> Time series with thresholds, bar charts and more."),
        figure("charts-dark.webp", "A dashboard of charts in the dark theme", "<strong>Dark charts.</strong> The charts follow the theme of the page."),
        figure("table.webp", "A table panel with coloured cells", "<strong>Tables.</strong> Cell colours by thresholds, links and sorting."),
        figure("list.webp", "The list of dashboards with folders", "<strong>Folders.</strong> A dashboard list with one level of folders."),
        figure("variables.webp", "Variable controls and the open time picker", "<strong>Variables and time.</strong> Variables, the time picker and the refresh."),
        figure("editor.webp", "The panel editor with the query editor and the preview", "<strong>Editor.</strong> The query editor, the options and a live preview."),
        figure("history.webp", "The history of versions of a dashboard", "<strong>History.</strong> Each save is a version. You can compare and restore."),
        figure("inspect.webp", "The inspect view of a panel query", "<strong>Inspect.</strong> The SQL that the server sent, the time and the rows."),
        figure("loader.webp", "A panel that shows three pulsing dots while it loads", "<strong>Loader.</strong> Three small dots show that a panel loads."),
        figure("latency.webp", "A dashboard with gauges, a heatmap, a histogram, a state timeline and a logs panel", "<strong>More panel types.</strong> Gauges, heatmaps, histograms, state timelines and logs."),
        figure("diff.webp", "The comparison of two versions of a dashboard", "<strong>Compare.</strong> The editor shows the change between two versions of a document."),
        figure("access-readonly.webp", "A dashboard in the read-only view, without the Edit switch", "<strong>Read only.</strong> A person without the write grant sees no Edit switch."),
        figure("access-refused.webp", "The refusal that the page shows when the PDP says no", "<strong>Refusal.</strong> The PDP refuses a dashboard. Hune shows no data of it."),
        figure("mobile.webp", "A dashboard at the width of a phone", "<strong>Narrow screens.</strong> The grid becomes one column."),
    ])
    return (
        head(name, "Hune", description)
        + header(name)
        + f"""<section class="hero" aria-labelledby="hero-title">
  <div class="wrap hero-grid">
    <div>
      <p class="eyebrow"><span class="badge">Version {VERSION}</span> Dashboards over ClickHouse, PostgreSQL and MongoDB</p>
      <h1 id="hero-title">Hune</h1>
      <p class="promise">Dashboards as JSON, one static binary and one HCL file.</p>
      <p class="sub">The server builds every query from the saved document, so a viewer never sends SQL. It is for teams that keep their data in SQL and want a small, strict dashboard server.</p>
      <div class="btn-row">
        <a class="btn primary" href="getting-started.html">Get started</a>
        <a class="btn" href="{REPO_URL}/releases/latest">Download v{VERSION}</a>
      </div>
      <p class="more"><a href="architecture.html">See how it works</a></p>
    </div>
    <div class="hero-shot">{figure("hero.webp", "A dashboard in the light theme: stat panels, a time series, a table and a bar chart", "A demo dashboard over synthetic data in ClickHouse.")}</div>
  </div>
</section>
<section class="section" aria-labelledby="why-h">
  <div class="wrap">
    <div class="section-head"><h2 id="why-h">Dashboards that stay under control</h2>
    <p>Hune shows dashboards of SQL and document panels. It keeps the rules of the server in one place, and it keeps them strict.</p></div>
    <div class="grid c3">{card_html}</div>
  </div>
</section>
<section class="section alt" aria-labelledby="how-h">
  <div class="wrap">
    <div class="section-head"><h2 id="how-h">How it works</h2>
    <p>Hune is one static binary. The page is inside. The query engine checks each text and asks the data source.</p></div>
    {panel("architecture.svg", "Architecture of Hune: the browser, the edge, the single binary with the HTTP server, the access guard, the query engine and the store, the data sources, and the PDP", "<strong>The single binary.</strong> The HTTP server, the access guard, the query engine, the store and the three source clients are in one program.")}
    {panel("query-flow.svg", "The query flow: the dashboard document, the expander, the query engine with its limits, and the data source", "<strong>One query.</strong> The server expands the variables and the macros of the saved SQL, checks the text, applies the limits and asks the source.")}
    {panel("access-model.svg", "The resource tree of the access model: service, folder, dashboard and source, with the three actions and the two ceilings", "<strong>The access model.</strong> A grant on a folder covers its dashboards. <a href='access.html'>Read the rules</a>.")}
    {panel("storage.svg", "The two stores: a volume with one JSON file for each dashboard, and PostgreSQL with one row for each dashboard", "<strong>Two stores.</strong> Both keep the same canonical bytes. A command copies a store to the other kind.")}
  </div>
</section>
<section class="section" aria-labelledby="see-h">
  <div class="wrap">
    <div class="section-head"><h2 id="see-h">See it</h2>
    <p>Each picture comes from a live copy of the product, with synthetic demo data. Nothing is mocked.</p></div>
    <div class="shots">{shots}</div>
  </div>
</section>
<section class="section alt" aria-labelledby="features-h">
  <div class="wrap">
    <div class="section-head"><h2 id="features-h">What Hune does</h2></div>
    <div class="tablewrap"><table class="feature-table"><thead><tr><th scope="col">Area</th><th scope="col">What Hune does</th></tr></thead><tbody>{rows}</tbody></table></div>
  </div>
</section>
<section class="section" aria-labelledby="run-h">
  <div class="wrap narrow">
    <div class="section-head"><h2 id="run-h">Run it in five minutes</h2>
    <p>This test starts a local ClickHouse in Docker, on 127.0.0.1. The <a href="getting-started.html">guide</a> has each step, with the demo data.</p></div>
    <div class="code-wrap"><span class="label">sh</span><pre data-copy tabindex="0" aria-label="Code example: quick start"><code>{html.escape(QUICKSTART % {"v": VERSION}, quote=False)}</code></pre></div>
    <div class="btn-row"><a class="btn primary" href="getting-started.html">Open the guide</a><a class="btn" href="configuration.html">Read the configuration</a></div>
  </div>
</section>
<section class="section alt" aria-labelledby="family-h">
  <div class="wrap narrow">
    <div class="section-head"><h2 id="family-h">The ProcurAuth family</h2>
    <p>Hune is a program of the <a href="https://github.com/ProcurAuth">ProcurAuth</a> organisation. The organisation holds the open specification and the programs that follow it. The <a href="https://procurauth.github.io/procurauth/implementations.html">implementations page</a> lists them, with the results of the conformance runs.</p></div>
  </div>
</section>
<section class="section" aria-labelledby="go-h">
  <div class="wrap">
    <div class="section-head"><h2 id="go-h">Where to go next</h2></div>
    <div class="btn-row">
      <a class="btn primary" href="getting-started.html">Getting started</a>
      <a class="btn" href="configuration.html">Configuration</a>
      <a class="btn" href="access.html">Access and ProcurAuth</a>
      <a class="btn" href="specification.html">Specification</a>
      <a class="btn" href="changelog.html">Changelog</a>
    </div>
  </div>
</section>
"""
        + footer()
    )


def not_found() -> str:
    name = "404.html"
    description = "This page does not exist. Go to the home page or to the guide of Hune."
    page = head(name, "Page not found", description)
    # The page 404 can be served at any depth: it loads its files from the public address.
    page = page.replace('<meta name="color-scheme"', '<meta name="robots" content="noindex">\n<meta name="color-scheme"', 1)
    page = page.replace('<link rel="canonical" href="' + SITE_URL + '404.html">\n', "")
    for rel in ("assets/favicon.svg", "assets/favicon-32.png", "assets/favicon.ico", "assets/apple-touch-icon.png", "assets/site.css", "assets/site.js", "site.webmanifest"):
        page = page.replace(f'"{rel}"', f'"{SITE_URL}{rel}"')
    body = header(name)
    body = body.replace('href="index.html"', f'href="{SITE_URL}index.html"')
    body = re.sub(r'<a href="([a-z-]+\.html)"', lambda m: f'<a href="{SITE_URL}{m.group(1)}"', body)
    main = (
        '<div class="page-head"><div class="wrap"><p class="eyebrow"><span class="badge">Error 404</span> Page not found</p><h1>This page does not exist</h1>'
        '<p class="lede">The address is wrong, or the page moved. Go to the home page or to the guide.</p></div></div>'
        f'<div class="page-body"><div class="wrap"><div class="btn-row"><a class="btn primary" href="{SITE_URL}index.html">Home page</a>'
        f'<a class="btn" href="{SITE_URL}getting-started.html">Getting started</a></div></div></div>\n'
    )
    foot = footer()
    foot = re.sub(r'href="([a-z-]+\.html)"', lambda m: f'href="{SITE_URL}{m.group(1)}"', foot)
    return page + body + main + foot


def small_files() -> dict[str, str]:
    urls = [SITE_URL] + [SITE_URL + p[0] for p in PAGES if p[0] not in ("index.html",)]
    sitemap = '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    sitemap += "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls) + "</urlset>\n"
    manifest = """{
  "name": "Hune",
  "short_name": "Hune",
  "description": "A dashboard server for ClickHouse, PostgreSQL and MongoDB.",
  "start_url": "index.html",
  "scope": "./",
  "display": "browser",
  "background_color": "#fafafd",
  "theme_color": "#4338ca",
  "icons": [
    { "src": "assets/icon-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "assets/icon-512.png", "sizes": "512x512", "type": "image/png" }
  ]
}
"""
    robots = f"User-agent: *\nAllow: /\n\nSitemap: {SITE_URL}sitemap.xml\n"
    return {"sitemap.xml": sitemap, "site.webmanifest": manifest, "robots.txt": robots}


def build() -> dict[str, str]:
    out: dict[str, str] = {}
    for name, title, _short, source, eyebrow, lede, _nav in PAGES:
        out[name] = overview() if name == "index.html" else doc_page(name, title, eyebrow, lede, source)
    out["404.html"] = not_found()
    out.update(small_files())
    return out


def main(argv: list[str]) -> int:
    check = "--check" in argv
    pages = build()
    stale: list[str] = []
    for name, text in sorted(pages.items()):
        path = DOCS / name
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current != text:
            stale.append(name)
            if not check:
                path.write_text(text, encoding="utf-8")
    if check:
        if stale:
            sys.stdout.write("site: these pages differ from their source: " + ", ".join(stale) + "\nRun: python3 -I scripts/build-site.py\n")
            return 1
        sys.stdout.write(f"site: {len(pages)} files are current\n")
        return 0
    sys.stdout.write(f"site: wrote {len(stale)} of {len(pages)} files\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
