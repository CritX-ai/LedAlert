#!/usr/bin/env python3
"""Publish verified main-push artifacts; reruns never replace a release or a crate."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import tomllib
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import release
import verify

REPOSITORY = "CritX-ai/LedAlert"
API = f"https://api.github.com/repos/{REPOSITORY}"
MARKER = re.compile(r"<!-- ledalert-release:(\{[^\n]+\}) -->")


def request(url: str, *, data: bytes | None = None, method: str = "GET",
            missing: bool = False, content_type: str = "application/json") -> bytes | None:
    headers = {"User-Agent": "LedAlert-release", "Accept": "application/vnd.github+json",
               "X-GitHub-Api-Version": "2022-11-28", "Content-Type": content_type}
    if url.startswith((API + "/", f"https://uploads.github.com/repos/{REPOSITORY}/")):
        token = os.environ.get("GH_TOKEN")
        if token:
            headers["Authorization"] = "Bearer " + token
    try:
        with urlopen(Request(url, data=data, headers=headers, method=method), timeout=60) as response:
            payload = response.read(8 * 1024 * 1024 + 1)
            verify.require(len(payload) <= 8 * 1024 * 1024, "Remote metadata exceeds 8 MiB")
            return payload
    except HTTPError as error:
        if missing and error.code == 404:
            return None
        # Do not log request headers, authentication data or arbitrary server bodies.
        raise ValueError(f"Remote request failed (HTTP {error.code}): {method} {url}") from None


def github(path: str, *, payload: dict | None = None, method: str = "GET", missing: bool = False):
    body = request(API + path, data=release.json_bytes(payload) if payload is not None else None,
                   method=method, missing=missing)
    return None if body is None else json.loads(body)


def registry_version(version: str) -> dict | None:
    data = request("https://index.crates.io/le/da/ledalert", missing=True)
    if data is None:
        return None
    versions = [json.loads(line) for line in data.splitlines() if line]
    matches = [entry for entry in versions if entry.get("vers") == version]
    verify.require(len(matches) <= 1, "Registry returned duplicate versions")
    return matches[0] if matches else None


def tag_commit(tag: str) -> str | None:
    ref = github(f"/git/ref/tags/{tag}", missing=True)
    if ref is None:
        return None
    target = ref["object"]
    for _ in range(4):
        if target["type"] == "commit":
            return target["sha"]
        verify.require(target["type"] == "tag", "Release tag does not resolve to a commit")
        target = github(f"/git/tags/{target['sha']}")["object"]
    raise ValueError("Release tag nesting exceeds the recovery limit")


def receipt(record: dict, version: str) -> dict:
    matches = MARKER.findall(record.get("body") or "")
    verify.require(len(matches) == 1, "Existing release lacks one recognized publication receipt; refusing to adopt it")
    value = json.loads(matches[0])
    verify.require(value.get("version") == version and isinstance(value.get("commit"), str) and
                   re.fullmatch(r"[0-9a-f]{40}", value["commit"]) is not None,
                   "Existing release identity is invalid")
    hashes = value.get("sha256", {})
    verify.require(set(hashes) == set(release.release_names(version, True)) and
                   all(isinstance(checksum, str) and verify.SHA256.fullmatch(checksum) for checksum in hashes.values()),
                   "Existing release receipt has invalid artifact hashes")
    verify.require(record.get("target_commitish") == value["commit"] and not record.get("prerelease"),
                   "Existing release target differs from its receipt")
    return value


def inspect_assets(record: dict, expected: dict, *, complete: bool) -> set[str]:
    assets = record.get("assets", [])
    names = [asset["name"] for asset in assets]
    verify.require(len(names) == len(set(names)) and set(names).issubset(expected),
                   "Existing release has duplicate or unexpected assets")
    for asset in assets:
        verify.require(asset.get("state") == "uploaded" and asset.get("digest") == "sha256:" + expected[asset["name"]],
                       f"Existing asset differs or is incomplete: {asset['name']}; never replacing released bytes")
    if complete:
        verify.require(set(names) == set(expected), "Published release is missing verified assets")
    return set(names)


def require_registry_match(entry: dict, checksum: str) -> None:
    verify.require(entry.get("name") == "ledalert" and entry.get("cksum") == checksum and entry.get("yanked") is False,
                   "Existing crates.io version differs from the verified package or is yanked; it cannot be republished")


def context(commit: str) -> str:
    verify.require(os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main" and
                   os.environ.get("GITHUB_REPOSITORY") == REPOSITORY and os.environ.get("GITHUB_SHA") == commit,
                   "Publication only accepts this repository's exact main-push SHA")
    verify.require(re.fullmatch(r"[0-9a-f]{40}", commit) is not None, "Expected a full triggering commit SHA")
    verify.require(release.command("git", "rev-parse", "HEAD") == commit, "Checkout does not match the triggering commit")
    verify.require(not release.command("git", "status", "--porcelain", "--untracked-files=no"),
                   "Tracked source changed after checkout")
    verify.require_distribution(release.ROOT)
    version = tomllib.loads((release.ROOT / "Cargo.toml").read_text())["package"]["version"]
    verify.require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is not None, "Automated releases require a stable version")
    return version


def preflight(version: str, commit: str) -> bool:
    record = github(f"/releases/tags/v{version}", missing=True)
    target = tag_commit(f"v{version}")
    entry = registry_version(version)
    ready = True
    if record is not None:
        value = receipt(record, version)
        inspect_assets(record, value["sha256"], complete=not record["draft"])
        verify.require(target in {None, value["commit"]}, "Version tag points to another source commit")
        if not record["draft"]:
            verify.require(target == value["commit"], "Published release lacks its exact source tag")
        if entry is not None:
            require_registry_match(entry, value["sha256"][f"ledalert-{version}.crate"])
            verify.require(not record["draft"], "Registry publication exists alongside an unfinished GitHub draft")
            ready = False
        else:
            verify.require(value["commit"] == commit,
                           "Partial publication belongs to an earlier SHA; rerun that original main-push workflow, not this checkout")
    else:
        verify.require(entry is None, "Registry version exists without this workflow's GitHub release; refusing to adopt it")
        verify.require(target in {None, commit}, "Version tag already belongs to a different source commit")
    verify.report("publication_preflight", version=version, status="build" if ready else "already_published",
                  recovery="Rerun only the original main-push workflow for partial publication; no bytes are replaced")
    return ready


def local_receipt(output: Path, version: str, commit: str) -> dict:
    names = release.release_names(version, True)
    hashes = {}
    for name in names:
        path = output / name
        verify.require(path.is_file() and not path.is_symlink() and path.stat().st_size <= verify.MAX_ARCHIVE_BYTES,
                       f"Missing or unsafe verified artifact: {name}")
        hashes[name] = verify.file_digest(path)
    sums = "".join(f"{hashes[name]}  {name}\n" for name in names if name != "SHA256SUMS")
    verify.require((output / "SHA256SUMS").read_text() == sums, "Release checksums do not match the verified artifacts")
    source = verify.read_archive(output / names[1], f"ledalert-{version}-source")
    binary = verify.read_archive(output / names[0], f"ledalert-{version}-linux-x86_64")
    info = verify.read_json(source["BUILD-INFO.json"][0], "BUILD-INFO.json")
    verify.require(source["BUILD-INFO.json"] == binary["BUILD-INFO.json"] and
                   info.get("source_commit") == commit and info.get("version") == version and
                   info.get("crate_sha256") == hashes[names[3]], "Built artifact identity differs from this publication")
    sources = {name: release.digest(path.read_bytes()) for name, path in release.source_files().items()}
    verify.require(info.get("source_files") == sources and
                   all(source[name][0] == path.read_bytes() for name, path in release.source_files().items()),
                   "Built source differs from the triggering checkout")
    verify.inspect_crate(release.ROOT, output / names[3])
    return {"version": version, "commit": commit, "sha256": hashes}


def publish_github(output: Path, version: str, commit: str) -> None:
    value = local_receipt(output, version, commit)
    record = github(f"/releases/tags/v{version}", missing=True)
    target = tag_commit(f"v{version}")
    verify.require(target in {None, commit}, "Release tag already belongs to another commit")
    entry = registry_version(version)
    if entry is not None:
        require_registry_match(entry, value["sha256"][f"ledalert-{version}.crate"])
        verify.require(record is not None and not record["draft"], "Unexpected partial registry publication")
    if record is None:
        body = (f"Linux x86_64 (glibc 2.36+), source and Cargo package. Source commit: `{commit}`.\n\n"
                "Verify downloads with SHA256SUMS. The crates.io publication follows this release.\n\n"
                f"<!-- ledalert-release:{json.dumps(value, sort_keys=True)} -->")
        record = github("/releases", method="POST", payload={"tag_name": f"v{version}", "target_commitish": commit,
                         "name": f"LedAlert {version}", "body": body, "draft": True, "prerelease": False})
    verify.require(receipt(record, version) == value,
                   "Rerun artifacts differ from the original receipt; recover the original bytes, never replace the release")
    existing = inspect_assets(record, value["sha256"], complete=not record["draft"])
    if record["draft"]:
        for name in release.release_names(version, True):
            if name not in existing:
                request(f"https://uploads.github.com/repos/{REPOSITORY}/releases/{record['id']}/assets?name={name}",
                        method="POST", data=(output / name).read_bytes(), content_type="application/octet-stream")
        record = github(f"/releases/{record['id']}")
        inspect_assets(record, value["sha256"], complete=True)
        record = github(f"/releases/{record['id']}", method="PATCH", payload={"draft": False})
    verify.require(not record["draft"] and tag_commit(f"v{version}") == commit,
                   "GitHub release publication is not confirmed at the triggering SHA")
    verify.report("github_publication", status="published", version=version, commit=commit,
                  next="crates.io; a failure there leaves an explicitly partial external publication")


def publish_crate_native(output: Path, commit: str, token: str | None) -> None:
    release.native_environment()
    version = tomllib.loads((release.ROOT / "Cargo.toml").read_text())["package"]["version"]
    value = local_receipt(output, version, commit)
    expected = value["sha256"][f"ledalert-{version}.crate"]
    record = github(f"/releases/tags/v{version}")
    verify.require(not record["draft"] and receipt(record, version) == value and tag_commit(f"v{version}") == commit,
                   "The exact GitHub release must be public before crates.io publication")
    inspect_assets(record, value["sha256"], complete=True)
    entry = registry_version(version)
    if entry is not None:
        require_registry_match(entry, expected)
        verify.report("registry_publication", status="already_published", version=version)
        return
    package = verify.check_crate(release.ROOT)
    verify.require(verify.file_digest(package) == expected, "Repackaged Cargo bytes differ from the verified release")
    verify.require(bool(token), "Missing CARGO_REGISTRY_TOKEN for cargo publish")
    with verify.isolated_environment() as env:
        # No builds, tests or build scripts run with the publishing credential.
        env["CARGO_REGISTRY_TOKEN"] = token
        result = subprocess.run(["cargo", "publish", "--locked", "--no-verify", "--registry", "crates-io"],
                                cwd=release.ROOT, env=env, timeout=600)
    verify.require(result.returncode == 0,
                   "cargo publish failed; GitHub is already public and crates.io may have accepted the upload. Rerun the original workflow")
    verify.require(verify.file_digest(package) == expected, "Cargo changed the uploaded bytes; external publication requires owner review")
    for attempt in range(7):
        entry = registry_version(version)
        if entry is not None:
            require_registry_match(entry, expected)
            verify.report("registry_publication", status="published", version=version, sha256=expected)
            return
        if attempt < 6:
            time.sleep(10)
    raise ValueError("Registry acknowledgement is not visible after 60 seconds; GitHub is public and the upload may be accepted. Rerun the original workflow")


def publish_crate(output: Path, version: str, commit: str, token: str | None) -> None:
    local_receipt(output, version, commit)
    image = (output / ".builder-image").read_text().strip()
    verify.require(re.fullmatch(r"sha256:[0-9a-f]{64}", image) is not None, "Missing verified baseline builder image")
    engine, podman = release.container_engine()
    with tempfile.TemporaryDirectory(prefix="ledalert-publish-") as temporary:
        source = Path(temporary)
        for name, path in release.source_files().items():
            destination = source / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            destination.chmod(0o644)
        args = [engine, "run", "--rm", "--platform", "linux/amd64", "--cap-drop=ALL",
                "--security-opt=no-new-privileges", "--user", f"{os.getuid()}:{os.getgid()}"]
        if podman:
            args.append("--userns=keep-id")
        args.extend(["--volume", f"{source}:/build:Z", "--volume", f"{output}:/output:ro,Z", "--workdir", "/build",
                     "--env", "HOME=/build/.home", "--env", "CARGO_HOME=/build/.cargo-home",
                     "--env", "CARGO_TARGET_DIR=/build/target", "--env", "CARGO_REGISTRY_TOKEN", image,
                     "python3", "packaging/publish.py", "crate-native", "--output", "/output", "--commit", commit])
        env = dict(os.environ)
        if token:
            env["CARGO_REGISTRY_TOKEN"] = token
        subprocess.run(args, check=True, timeout=7200, env=env)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("preflight", "github", "crate", "crate-native"))
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", type=Path, default=release.ROOT / "dist")
    args = parser.parse_args()
    token = os.environ.pop("CARGO_REGISTRY_TOKEN", None)
    try:
        if args.phase == "crate-native":
            publish_crate_native(args.output.resolve(), args.commit, token)
            return
        version = context(args.commit)
        if args.phase == "preflight":
            ready = preflight(version, args.commit)
            output = os.environ.get("GITHUB_OUTPUT")
            verify.require(bool(output), "Missing workflow output channel")
            with Path(output).open("a") as file:
                file.write(f"release={'true' if ready else 'false'}\n")
        elif args.phase == "github":
            publish_github(args.output.resolve(), version, args.commit)
        else:
            publish_crate(args.output.resolve(), version, args.commit, token)
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        verify.report("publication_failed", phase=args.phase, error=str(error),
                      recovery="Inspect the version's GitHub release and crates.io state, then rerun the original main-push workflow. No automatic delete, overwrite, retag or second upload attempt")
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
