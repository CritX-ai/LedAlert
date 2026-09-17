#!/usr/bin/env python3
"""Prepare allowlisted Markdown with Pandoc, then build it with ReGen 1.1.0.

Python 3.11+, installed pandoc and exactly regen 1.1.0 are required. Use the
pinned public container with --container podman/docker, or an installed CLI.
No Python packages or JavaScript toolchain are used. Relative paths use cwd.
Only pages.json inputs enter staging. Local links must name listed pages/media;
use external repository URLs for source files that are not public site content.

Existing output must be empty or owned by this command. Replacement is staged on
the same filesystem and rolled back on promotion failure. Keep source/output
parents stable while building; this is not protection against hostile concurrent
filesystem mutation. Interrupted .NAME-docs-stage/.NAME-docs-previous directories
require manual inspection/recovery, never automatic deletion on the next run.
"""
from __future__ import annotations

import argparse
import html
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
from urllib.parse import quote, unquote, urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "docs/site"
PRODUCTION = "https://alert.critx.ai/"
VERSION = "regen 1.1.0"
# Published ReGen 1.1.0 multi-platform index (Linux amd64 and arm64).
IMAGE = "ghcr.io/critx-ai/regen-ssg@sha256:4f435cfdafc67e761107ff225490f3b3bb7e21fa82d0a9c2ce2433d15516f354"
MARKER = "ledalert-docs.json"
OWNERSHIP = {"generator": "LedAlert documentation", "format": 1}
PRIVATE_NAMES = {"product.md", "design.md", "agents.md", "important.md", "purpose.md",
                 "plan.md", "planning.md", "verification.md"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def relative_name(value: str) -> str:
    require(isinstance(value, str) and bool(value) and "\\" not in value,
            f"Unsafe source/path name: {value!r}")
    require(all(part not in {"", ".", ".."} and not part.startswith(".") and
                part.casefold() not in PRIVATE_NAMES for part in value.split("/")),
            f"Private or unsafe source/path name: {value!r}")
    return value


def inspect_path(path: Path) -> None:
    """Reject symlinks in all components before any read, copy or replacement."""
    for part in reversed((path, *path.parents)):
        try:
            mode = part.lstat().st_mode
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(mode), f"Symbolic links are not accepted: {part}")
        require(stat.S_ISDIR(mode) or stat.S_ISREG(mode), f"Special file is not accepted: {part}")


def source_file(name: str) -> Path:
    path = ROOT / relative_name(name)
    inspect_path(path)
    require(path.is_file(), f"Missing documentation input: {name}")
    return path


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def run(*args: str, content: str | None = None) -> str:
    result = subprocess.run(args, input=content, text=True, encoding="utf-8", capture_output=True)
    require(result.returncode == 0, f"{args[0]} failed ({result.returncode}):\n{result.stderr.strip()}")
    if result.stderr.strip():
        print(result.stderr.strip(), file=sys.stderr)
    return result.stdout


def base_url(value: str) -> str:
    require(not any(character.isspace() or ord(character) < 32 for character in value),
            "--base-url cannot contain whitespace or controls")
    parsed = urlsplit(value)
    require(parsed.scheme in {"http", "https"} and bool(parsed.hostname) and
            parsed.username is None and parsed.password is None and not parsed.query and
            not parsed.fragment and "\\" not in value, "--base-url must be an absolute HTTP(S) URL without credentials, query or fragment")
    # Match ReGen's deliberately portable, unencoded hosting-prefix policy.
    prefix = parsed.path.rstrip("/")
    require(not prefix or (prefix.startswith("/") and all(
        re.fullmatch(r"[A-Za-z0-9_.-]+", part) and part not in {".", ".."}
        for part in prefix[1:].split("/"))), "Unsafe --base-url hosting prefix")
    parsed.port  # Reject malformed/out-of-range ports before staging.
    return urlunsplit((parsed.scheme, parsed.netloc, prefix + "/", "", ""))


class ArticleHTML(HTMLParser):
    """Inspect HTML, preserving markup while collecting rendered headings and prose."""
    TEXT_BREAKS = {"address", "article", "aside", "blockquote", "br", "dd", "div", "dl", "dt",
                   "figcaption", "figure", "footer", "header", "hr", "li", "main", "nav", "ol",
                   "p", "pre", "section", "table", "tbody", "td", "tfoot", "th", "thead", "tr", "ul"}

    def __init__(self, rewrite=None, collect_sections: bool = False):
        super().__init__(convert_charrefs=False)
        self.rewrite = rewrite
        self.parts: list[str] = []
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.headings: list[dict] = []
        self.heading = None
        # Each heading owns only the prose before the next heading, at any level.
        self.sections: list[tuple[dict | None, list[str]]] = [(None, [])] if collect_sections else []
        self.hidden_text_depth = 0

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        identifier = attributes.get("id") or (attributes.get("name") if tag == "a" else None)
        if identifier:
            require(identifier not in self.ids, f"Duplicate HTML anchor: {identifier}")
            self.ids.add(identifier)
        if tag in {"h1", "h2", "h3", "h4", "h5", "h6"}:
            self.heading = {"id": attributes.get("id", ""), "title": "", "level": int(tag[1])}
            self.headings.append(self.heading)
            if self.sections:
                self.sections.append((self.heading, []))
        if tag in {"script", "style"}:
            self.hidden_text_depth += 1
        if tag in self.TEXT_BREAKS:
            self.collect_text(" ")
        elif tag == "img":
            self.collect_text(attributes.get("alt") or "")
        changed = False
        rewritten = []
        for key, value in attrs:
            if key in {"href", "src", "poster"} and value is not None:
                self.links.append(value)
                if self.rewrite is not None:
                    replacement = self.rewrite(value)
                    changed = changed or replacement != value
                    value = replacement
            rewritten.append((key, value))
        if changed:
            self.parts.append("<" + tag + "".join(" " + key + ("" if value is None else
                              '="' + html.escape(value, quote=True) + '"') for key, value in rewritten) +
                              (" />" if self.get_starttag_text().endswith("/>") else ">"))
        else:
            self.parts.append(self.get_starttag_text())

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        if self.heading is not None and tag == "h" + str(self.heading["level"]):
            self.heading["title"] = " ".join(self.heading["title"].split())
            self.heading = None
        if tag in {"script", "style"}:
            self.hidden_text_depth = max(0, self.hidden_text_depth - 1)
        if tag in self.TEXT_BREAKS:
            self.collect_text(" ")
        self.parts.append(f"</{tag}>")

    def handle_data(self, data):
        self.parts.append(data)
        self.collect_text(data)

    def collect_text(self, data):
        if self.hidden_text_depth:
            return
        if self.heading is not None:
            self.heading["title"] += data
        elif self.sections:
            self.sections[-1][1].append(data)

    def handle_entityref(self, name):
        entity = "&" + name + ";"
        self.parts.append(entity)
        self.collect_text(html.unescape(entity))

    def handle_charref(self, name):
        self.handle_entityref("#" + name)

    def handle_comment(self, data):
        self.parts.append("<!--" + data + "-->")

    def handle_decl(self, decl):
        self.parts.append("<!" + decl + ">")

    def handle_pi(self, data):
        self.parts.append("<?" + data + ">")


def inspect_html(content: str, rewrite=None, collect_sections: bool = False) -> ArticleHTML:
    document = ArticleHTML(rewrite, collect_sections)
    document.feed(content)
    document.close()
    return document


def walk(value):
    if isinstance(value, dict):
        if "t" in value:
            yield value
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def remove_first_h1(value) -> str | None:
    """Remove only the first Markdown H1, including one nested in a block."""
    children = value.values() if isinstance(value, dict) else value if isinstance(value, list) else ()
    for index, child in enumerate(children):
        if isinstance(value, list) and isinstance(child, dict) and child.get("t") == "Header" and child["c"][0] == 1:
            identifier = child["c"][1][0]
            del value[index]
            return identifier
        identifier = remove_first_h1(child)
        if identifier is not None:
            return identifier
    return None


def manifest_inputs() -> dict:
    manifest = json.loads(source_file("docs/site/pages.json").read_text(encoding="utf-8"))
    require(set(manifest) == {"release_label", "templates", "pages", "assets", "media"},
            "Unexpected pages.json fields")
    require(isinstance(manifest["release_label"], str), "Missing release label")
    sources, ids, paths = set(), set(), set()
    for page in manifest["pages"]:
        require(set(page) == {"source", "id", "path", "title", "description", "group", "kicker"} and
                all(isinstance(value, str) and value for value in page.values()), "Invalid page manifest entry")
        source_file(page["source"])
        require(page["source"].endswith(".md"), "Pages must be canonical Markdown files")
        relative_name(page["id"])
        require(re.fullmatch(r"[a-z0-9_-]+(?:/[a-z0-9_-]+)*", page["id"]) is not None,
                f"Invalid ReGen page ID: {page['id']}")
        require(page["path"] == "/" or re.fullmatch(r"/(?:[a-z0-9_-]+/)+", page["path"]) is not None,
                f"Invalid documentation route: {page['path']}")
        require(page["source"] not in sources and page["id"] not in ids and page["path"] not in paths,
                "Duplicate page source, ID or route")
        sources.add(page["source"])
        ids.add(page["id"])
        paths.add(page["path"])
    require(any(page["id"] == "index" and page["source"] == "README.md" and page["path"] == "/"
                for page in manifest["pages"]), "README.md must own the index home page")
    require(manifest["templates"] and len(set(manifest["templates"])) == len(manifest["templates"]),
            "Templates must be a nonempty unique list")
    require("page.html" in manifest["templates"], "Missing page.html template")
    for template in manifest["templates"]:
        source_file("docs/site/templates/" + relative_name(template))
    for category in ("assets", "media"):
        destinations, listed = set(), set()
        for item in manifest[category]:
            require(set(item) == {"source", "path"}, f"Invalid {category} entry")
            source_file(item["source"])
            relative_name(item["path"])
            require(item["path"] not in destinations and item["source"] not in listed,
                    f"Duplicate {category} source or destination")
            destinations.add(item["path"])
            listed.add(item["source"])
    return manifest


def prepare(stage: Path, manifest: dict, url: str) -> None:
    site_root = urlsplit(url).path
    preview = url != PRODUCTION
    pages = {page["source"]: page for page in manifest["pages"]}
    routes = {page["path"]: page["source"] for page in pages.values()}
    media = {item["source"]: site_root + "media/" + item["path"] for item in manifest["media"]}
    documents, anchors = {}, {}
    for source in pages:
        ast = json.loads(run("pandoc", "--from=gfm+gfm_auto_identifiers", "--to=json",
                             content=source_file(source).read_text(encoding="utf-8")))
        encoded = json.dumps(ast)
        rendered = inspect_html(run("pandoc", "--from=json", "--to=html5", "--wrap=none", content=encoded))
        documents[source], anchors[source] = ast, rendered.ids

    def rewrite_url(value: str, source: str) -> str:
        parsed = urlsplit(value)
        require(not any(ord(character) < 32 for character in value) and "\\" not in value,
                f"Unsafe link in {source}: {value}")
        if parsed.scheme or parsed.netloc:
            require(parsed.scheme in {"https", "http", "mailto"}, f"Unsupported link scheme in {source}: {value}")
            # Absolute links to this site's known pages follow preview origin/prefix too.
            if parsed.netloc not in {urlsplit(PRODUCTION).netloc, urlsplit(url).netloc}:
                return value
        path = unquote(parsed.path)
        route = path
        if site_root != "/" and route.startswith(site_root):
            route = "/" + route[len(site_root):]
        if route in routes:
            target = routes[route]
        elif path in media.values():
            return value
        elif route.startswith("/media/") and site_root + route.lstrip("/") in media.values():
            return urlunsplit(("", "", site_root + route.lstrip("/"), parsed.query, parsed.fragment))
        elif parsed.scheme or parsed.netloc:
            raise ValueError(f"Unlisted same-site link in {source}: {value}")
        else:
            target = source if not path else posixpath.normpath(
                path.lstrip("/") if path.startswith("/") else posixpath.join(posixpath.dirname(source), path))
        require(target in pages or target in media, f"Unlisted local link in {source}: {value}")
        if target in pages:
            require(not parsed.fragment or unquote(parsed.fragment) in anchors[target],
                    f"Missing anchor in {source}: {value}")
            destination = site_root + pages[target]["path"].lstrip("/")
        else:
            require(not parsed.fragment, f"Media fragments are not supported in {source}: {value}")
            destination = media[target]
        return urlunsplit(("", "", destination, parsed.query, parsed.fragment))

    groups = {}
    search = []
    for source, page in pages.items():
        ast = documents[source]
        for node in walk(ast):
            if node["t"] in {"Link", "Image"}:
                node["c"][-1][0] = rewrite_url(node["c"][-1][0], source)
        heading_id = remove_first_h1(ast["blocks"])
        require(bool(heading_id), f"A Markdown H1 with a generated ID is required: {source}")
        rendered = run("pandoc", "--from=json", "--to=html5", "--wrap=none", content=json.dumps(ast))
        article = inspect_html(rendered, lambda value: rewrite_url(value, source), collect_sections=True)
        data = {"body_html": "".join(article.parts), "heading_id": heading_id,
                "toc": [heading for heading in article.headings if heading["id"]],
                "group": page["group"], "kicker": page["kicker"], "is_home": page["id"] == "index"}
        write_json(stage / "content/en/pages" / (page["id"] + ".yaml"),
                   {"title": page["title"], "description": page["description"], "template": "page.html",
                    "slug": page["path"].strip("/"), "data": data})
        path = site_root + page["path"].lstrip("/")
        groups.setdefault(page["group"], []).append({key: page[key] for key in ("id", "title", "description")} | {"path": path})
        for heading, text_parts in article.sections:
            text = " ".join("".join(text_parts).split())
            if heading is None:
                if not text:
                    continue
                title, destination = page["title"], path
            else:
                require(bool(heading["id"]), f"Search heading requires an HTML ID in {source}: {heading['title']}")
                title, destination = heading["title"], path + "#" + quote(heading["id"], safe="")
            search.append({"title": title, "page_title": page["title"], "path": destination, "text": text})
    cargo = tomllib.loads(source_file("Cargo.toml").read_text(encoding="utf-8"))
    require(cargo["package"]["name"] == "ledalert" and cargo["package"]["version"], "Missing Cargo package version")
    write_json(stage / "content/en/site.yaml", {
        "canonical_domain": "alert.critx.ai", "release_label": manifest["release_label"],
        "version": cargo["package"]["version"],
        "preview": preview, "search_index_path": site_root + "search-index.json",
        "groups": [{"title": title, "items": items} for title, items in groups.items()]})
    config = source_file("docs/site/regen.toml").read_text(encoding="utf-8")
    require(tomllib.loads(config)["site"]["base_url"] == PRODUCTION and config.count(json.dumps(PRODUCTION)) == 1,
            "Committed regen.toml must use the single canonical production base_url")
    (stage / "regen.toml").write_text(config.replace(json.dumps(PRODUCTION), json.dumps(url)), encoding="utf-8")
    for template in manifest["templates"]:
        destination = stage / "templates" / template
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_file("docs/site/templates/" + template), destination)
    for category, destination_root in (("assets", stage / "assets"), ("media", stage / "public/media")):
        for item in manifest[category]:
            destination = destination_root / item["path"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_file(item["source"]), destination)
    write_json(stage / "public/search-index.json", search)
    write_json(stage / "public" / MARKER, OWNERSHIP)
    error_page = source_file("docs/site/404.html").read_text(encoding="utf-8")
    error_page = error_page.replace("{{site_root}}", html.escape(site_root, quote=True))
    require("{{" not in error_page and "{%" not in error_page, "404.html supports only the literal {{site_root}} placeholder")
    (stage / "public/404.html").write_text(error_page, encoding="utf-8")
    (stage / "public/robots.txt").write_text("User-agent: *\n" + ("Disallow: /\n" if preview else
        "Allow: /\nSitemap: " + url + "sitemap.xml\n"), encoding="utf-8")


def validate_site(output: Path, url: str) -> None:
    """Check rendered HTML and search links before promoting the generated site."""
    files = {}
    for path in output.rglob("*"):
        inspect_path(path)
        if path.is_file():
            files[path.relative_to(output).as_posix()] = path
    require("index.html" in files and "404.html" in files, "ReGen did not produce the home and 404 pages")
    documents = {name: inspect_html(path.read_text(encoding="utf-8")) for name, path in files.items() if name.endswith(".html")}
    search_index = json.loads(files["search-index.json"].read_text(encoding="utf-8"))
    link_groups = {name: document.links for name, document in documents.items()}
    link_groups["search-index.json"] = [entry["path"] for entry in search_index]
    prefix = urlsplit(url).path
    for name, links in link_groups.items():
        for value in links:
            link = urlsplit(value)
            if link.scheme or link.netloc:
                require(link.scheme in {"http", "https", "mailto"}, f"Unsafe rendered link in {name}: {value}")
                continue
            path = unquote(link.path)
            if path.startswith("/"):
                require(path.startswith(prefix), f"Link escapes hosting prefix in {name}: {value}")
                target = path[len(prefix):]
            else:
                target = posixpath.normpath(posixpath.join(posixpath.dirname(name), path)) if path else name
            if path.endswith("/") or not target or target == ".":
                target = target.rstrip("/") + "/index.html" if target not in {"", "."} else "index.html"
            require(target in files, f"Missing rendered link target in {name}: {value}")
            if link.fragment:
                require(target in documents and unquote(link.fragment) in documents[target].ids,
                        f"Missing rendered anchor in {name}: {value}")


def output_path(value: str) -> Path:
    requested = Path(value).expanduser()
    require(".." not in requested.parts, "--output cannot contain parent traversal")
    path = Path(os.path.abspath(requested))
    inspect_path(path)
    require(path != Path(path.anchor) and path != Path.home() and not ROOT.is_relative_to(path),
            "--output cannot replace the filesystem root, home, repository or its ancestors")
    require(not path.is_relative_to(ROOT) or (path.is_relative_to(ROOT / "dist") and path != ROOT / "dist"),
            "Outputs inside the repository must be beneath dist/, not source directories or dist/ itself")
    if path.exists():
        require(path.is_dir(), "--output must be a dedicated directory")
        for child in path.rglob("*"):
            inspect_path(child)
        if any(path.iterdir()):
            marker = path / MARKER
            require(marker.is_file() and json.loads(marker.read_text(encoding="utf-8")) == OWNERSHIP,
                    "Refusing to replace nonempty output not owned by packaging/docs.py")
    return path


def generator(regen: str | None, container: str | None, stage: Path) -> tuple[list[str], str]:
    executable = shutil.which(container or regen or "regen")
    require(executable is not None, f"Executable not found: {container or regen or 'regen'}")
    if container is None:
        return [executable], str(stage)
    require(hasattr(os, "getuid"), "Container builds require a Unix host; use --regen on other hosts")
    require(":" not in str(stage), "Container staging paths cannot contain ':'")
    available = subprocess.run([executable, "image", "inspect", IMAGE],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if available.returncode != 0:
        # The release image is public. Never consult personal registry credentials.
        with tempfile.TemporaryDirectory(prefix="ledalert-regen-auth-") as temporary:
            auth = Path(temporary) / "config.json"
            auth.write_text('{"auths":{}}\n', encoding="utf-8")
            if container == "podman":
                run(executable, "pull", "--authfile", str(auth), IMAGE)
            else:
                run(executable, "--config", temporary, "pull", IMAGE)
    command = [executable, "run", "--rm", "--pull=never", "--network=none", "--read-only",
               "--cap-drop=ALL", "--security-opt=no-new-privileges",
               "--user", f"{os.getuid()}:{os.getgid()}"]
    if container == "podman":
        command.extend(["--userns=keep-id", "--read-only-tmpfs=false"])
    command.extend(["--volume", f"{stage}:/site:rw" + (",Z" if container == "podman" else ""),
                    "--workdir", "/site", IMAGE])
    return command, "/site"


def build(regen: str | None, output: Path, url: str, container: str | None = None) -> None:
    require(shutil.which("pandoc") is not None, "Install Pandoc before preparing documentation")
    manifest = manifest_inputs()
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = output.parent / ("." + output.name + "-docs-stage")
    previous = output.parent / ("." + output.name + "-docs-previous")
    inspect_path(stage)
    inspect_path(previous)
    require(not previous.exists(), f"Recover the interrupted output before building: {previous}")
    stage.mkdir()  # Exclusive reservation also prevents concurrent builds to this output.
    try:
        runner, site_path = generator(regen, container, stage)
        require(run(*runner, "--version").strip() == VERSION,
                f"Exactly {VERSION} is required; no version fallback is allowed")
        prepare(stage, manifest, url)
        print(run(*runner, "build", "--site", site_path, "--profile", "release").strip())
        generated = stage / "dist"
        if url == PRODUCTION:
            # GitHub's fixed filename is outside ReGen's portable route namespace.
            (generated / "CNAME").write_text("alert.critx.ai\n", encoding="utf-8")
        validate_site(generated, url)
        output_path(str(output))
        require(not previous.exists(), f"Recovery destination appeared during build: {previous}")
        had_output = output.exists()
        if had_output:
            output.rename(previous)
        try:
            generated.rename(output)
        except BaseException:
            if had_output:
                previous.rename(output)
            raise
        if had_output:
            try:
                shutil.rmtree(previous)
            except OSError as error:
                print(f"Site installed; old output cleanup requires manual attention: {previous}: {error}", file=sys.stderr)
        print(f"Built LedAlert documentation into {output} ({'preview' if url != PRODUCTION else 'production'}: {url})")
    finally:
        try:
            shutil.rmtree(stage)
        except OSError as error:
            print(f"Staging cleanup requires manual attention: {stage}: {error}", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    runtime = parser.add_mutually_exclusive_group()
    runtime.add_argument("--regen", metavar="PATH", help="ReGen 1.1.0 executable (default: installed regen)")
    runtime.add_argument("--container", choices=("podman", "docker"), help="use the digest-pinned public ReGen 1.1.0 image")
    parser.add_argument("--output", default="dist/docs", metavar="DIR", help="dedicated generated site directory (default: dist/docs)")
    parser.add_argument("--base-url", default=PRODUCTION, metavar="URL", help="effective site URL, including hosting prefix (default: %(default)s)")
    args = parser.parse_args()
    try:
        build(args.regen, output_path(args.output), base_url(args.base_url), args.container)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"docs.py: {error}\n")


if __name__ == "__main__":
    main()
