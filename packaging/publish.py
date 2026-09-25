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
import windows

REPOSITORY = "CritX-ai/LedAlert"
API = f"https://api.github.com/repos/{REPOSITORY}"
MARKER = re.compile(r"<!-- ledalert-release:(\{[^\n]+\}) -->")


def artifact_names(version: str) -> list[str]:
    return release.release_names(version, True) + windows.names(version)


def request(url: str, *, data: bytes | None = None, method: str = "GET",
            missing: bool = False, content_type: str = "application/json", timeout: int = 60) -> bytes | None:
    headers = {"User-Agent": "LedAlert-release", "Accept": "application/vnd.github+json",
               "X-GitHub-Api-Version": "2022-11-28", "Content-Type": content_type}
    if url.startswith((API + "/", f"https://uploads.github.com/repos/{REPOSITORY}/")):
        token = os.environ.get("GH_TOKEN")
        if token:
            headers["Authorization"] = "Bearer " + token
    try:
        with urlopen(Request(url, data=data, headers=headers, method=method), timeout=timeout) as response:
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

def release_record(version: str) -> dict | None:
    tag = f"v{version}"
    record = github(f"/releases/tags/{tag}", missing=True)
    if record is not None:
        return record
    # Drafts are hidden from the tag endpoint and may outlive newer releases.
    matches = []
    page = 1
    while True:
        records = github(f"/releases?per_page=100&page={page}")
        matches.extend(candidate for candidate in records if candidate.get("tag_name") == tag)
        verify.require(len(matches) <= 1, "Multiple GitHub releases have the same version tag")
        if len(records) < 100:
            break
        page += 1
    return matches[0] if matches else None


def artifact_recovery(version: str, run_id: str, kind: str) -> bool:
    """Reuse this run's original bytes, including incomplete external publications."""
    verify.require(re.fullmatch(r"[0-9]+", run_id) is not None, "Missing or invalid workflow run ID")
    verify.require(kind in {"windows", "publication"}, "Unknown recovery artifact kind")
    name = f"{kind}-release-{run_id}"
    result = github(f"/actions/runs/{run_id}/artifacts?per_page=100&name={name}")
    artifacts = result["artifacts"]
    verify.require(result["total_count"] == len(artifacts) and len(artifacts) <= 1 and
                   all(artifact.get("name") == name and artifact.get("expired") is False for artifact in artifacts),
                   "Original artifact is ambiguous or expired; recover original bytes, never rebuild a release")
    if artifacts:
        return True
    verify.require(release_record(version) is None,
                   "Original artifact is missing for an existing release; recover original bytes, never rebuild")
    return False


def release_notes(version: str) -> str:
    changelog = (release.ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    sections = list(re.finditer(r"^## ([^\n]+)$", changelog, re.MULTILINE))
    matches = [index for index, section in enumerate(sections)
               if section.group(1).split()[:1] == [version]]
    verify.require(len(matches) == 1, f"CHANGELOG.md must contain exactly one release section for {version}")
    index = matches[0]
    start = sections[index]
    end = sections[index + 1].start() if index + 1 < len(sections) else len(changelog)
    verify.require(bool(changelog[start.end():end].strip()), f"Missing release notes for {version}")
    return changelog[start.start():end].strip()


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
    verify.require(set(hashes) == set(artifact_names(version)) and
                   all(isinstance(checksum, str) and verify.SHA256.fullmatch(checksum) for checksum in hashes.values()),
                   "Existing release receipt has invalid artifact hashes")
    prerelease = release.version_info(version)[1]
    verify.require(record.get("target_commitish") == value["commit"] and
                   record.get("tag_name") == f"v{version}" and
                   record.get("prerelease") is prerelease and value.get("prerelease") is prerelease,
                   "Existing release target or prerelease status differs from its receipt")
    verify.require(value.get("windows_signing") in {"unsigned", "certificate-store"} and
                   (prerelease or value["windows_signing"] == "certificate-store"),
                   "Stable publication receipt must identify signed Windows packages")
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
    release.version_info(version)
    return version


def preflight(version: str, commit: str) -> bool:
    prerelease = release.version_info(version)[1]
    record = release_record(version)
    target = tag_commit(f"v{version}")
    entry = None if prerelease else registry_version(version)
    ready = True
    if record is not None:
        value = receipt(record, version)
        inspect_assets(record, value["sha256"], complete=not record["draft"])
        verify.require(target in {None, value["commit"]}, "Version tag points to another source commit")
        if not record["draft"]:
            verify.require(target == value["commit"], "Published release lacks its exact source tag")
        if prerelease and not record["draft"]:
            ready = False
        elif entry is not None:
            require_registry_match(entry, value["sha256"][f"ledalert-{version}.crate"])
            verify.require(not record["draft"], "Registry publication exists alongside an unfinished GitHub draft")
            ready = False
        else:
            verify.require(value["commit"] == commit,
                           "Partial publication belongs to an earlier SHA; rerun that original main-push workflow, not this checkout")
    else:
        verify.require(entry is None, "Registry version exists without this workflow's GitHub release; refusing to adopt it")
        verify.require(target in {None, commit}, "Version tag already belongs to a different source commit")
    if ready:
        release_notes(version)
    verify.report("publication_preflight", version=version, status="build" if ready else "already_published",
                  recovery="Rerun only the original main-push workflow for partial publication; no bytes are replaced")
    return ready


def local_receipt(output: Path, version: str, commit: str) -> dict:
    names = artifact_names(version)
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
    prerelease = release.version_info(version)[1]
    info = windows.inspect_artifacts(release.ROOT, output, commit=commit, require_signed=not prerelease)
    return {"version": version, "commit": commit, "prerelease": prerelease,
            "windows_signing": info["signing"], "sha256": hashes}


def assemble(linux: Path, windows_output: Path, output: Path, version: str, commit: str) -> None:
    """Combine independently verified builds without overwriting either checksum file."""
    names = artifact_names(version)
    release.prepare_output(output, names + [".builder-image"])
    with tempfile.TemporaryDirectory(prefix=".ledalert-assemble-", dir=output) as temporary:
        staging = Path(temporary)
        for name in names:
            if name == "SHA256SUMS":
                continue
            source = (windows_output if name in windows.names(version) else linux) / name
            verify.require(source.is_file() and not source.is_symlink(), f"Missing or unsafe build artifact: {name}")
            shutil.copyfile(source, staging / name)
        (staging / "SHA256SUMS").write_text("".join(
            f"{verify.file_digest(staging / name)}  {name}\n" for name in names if name != "SHA256SUMS"))
        builder = linux / ".builder-image"
        verify.require(builder.is_file() and not builder.is_symlink(), "Missing Linux baseline builder receipt")
        shutil.copyfile(builder, staging / ".builder-image")
        local_receipt(staging, version, commit)
        release.publish(staging, output, names + [".builder-image"])
    verify.report("publication_assembly", status="passed", version=version, commit=commit, artifacts=names)


def publish_github(output: Path, version: str, commit: str) -> None:
    value = local_receipt(output, version, commit)
    record = release_record(version)
    target = tag_commit(f"v{version}")
    verify.require(target in {None, commit}, "Release tag already belongs to another commit")
    prerelease = value["prerelease"]
    entry = None if prerelease else registry_version(version)
    if entry is not None:
        require_registry_match(entry, value["sha256"][f"ledalert-{version}.crate"])
        verify.require(record is not None and not record["draft"], "Unexpected partial registry publication")
    if record is None:
        signed = value["windows_signing"] == "certificate-store"
        windows_note = (
            "The MSIX is signed. Installation requires Windows to trust its signing certificate; "
            "explicitly grant notification access in LedAlert."
            if signed else
            "The MSIX is **unsigned and for development only**; it is not trusted or normally installable. "
            "The portable ZIP runs without notification access. For explicit Developer Mode loose-manifest "
            "registration and notification consent, follow docs/windows-verification.md; no certificate trust changes are automated."
        )
        registry_note = ("This is a GitHub-only prerelease; the .crate is a source download, not a crates.io publication."
                         if prerelease else "The crates.io publication follows this release.")
        body = (f"{release_notes(version)}\n\n"
                f"Linux x86_64 (glibc 2.36+), Windows 11 x86_64 portable ZIP and {'signed' if signed else 'unsigned development'} MSIX, "
                f"source and Cargo package. Source commit: `{commit}`.\n\n"
                f"{windows_note} Portable Windows execution cannot read notifications. "
                f"Verify downloads with SHA256SUMS. {registry_note}\n\n"
                f"<!-- ledalert-release:{json.dumps(value, sort_keys=True)} -->")
        record = github("/releases", method="POST", payload={"tag_name": f"v{version}", "target_commitish": commit,
                         "name": f"LedAlert {version}", "body": body, "draft": True, "prerelease": prerelease,
                         "make_latest": "false" if prerelease else "true"})
    verify.require(receipt(record, version) == value,
                   "Rerun artifacts differ from the original receipt; recover the original bytes, never replace the release")
    existing = inspect_assets(record, value["sha256"], complete=not record["draft"])
    if record["draft"]:
        for name in artifact_names(version):
            if name not in existing:
                request(f"https://uploads.github.com/repos/{REPOSITORY}/releases/{record['id']}/assets?name={name}",
                        method="POST", data=(output / name).read_bytes(), content_type="application/octet-stream",
                        timeout=900)
        record = github(f"/releases/{record['id']}")
        inspect_assets(record, value["sha256"], complete=True)
        record = github(f"/releases/{record['id']}", method="PATCH",
                        payload={"draft": False, "prerelease": prerelease,
                                 "make_latest": "false" if prerelease else "true"})
    verify.require(not record["draft"] and receipt(record, version) == value and tag_commit(f"v{version}") == commit,
                   "GitHub release publication is not confirmed at the triggering SHA")
    verify.report("github_publication", status="published", version=version, commit=commit,
                  next="complete; GitHub-only prerelease" if prerelease else
                  "crates.io; a failure there leaves an explicitly partial external publication")


def publish_crate_native(output: Path, commit: str, token: str | None) -> None:
    version = tomllib.loads((release.ROOT / "Cargo.toml").read_text())["package"]["version"]
    verify.require(not release.version_info(version)[1], "Prereleases are GitHub-only; crates.io publication is forbidden")
    release.native_environment()
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
    verify.require(not release.version_info(version)[1], "Prereleases are GitHub-only; crates.io publication is forbidden")
    local_receipt(output, version, commit)
    image = (output / ".builder-image").read_text().strip()
    verify.require(re.fullmatch(r"sha256:[0-9a-f]{64}", image) is not None, "Missing verified baseline builder image")
    engine, podman = release.container_engine()
    with tempfile.TemporaryDirectory(prefix="ledalert-publish-") as temporary:
        source = Path(temporary)
        # Hosted reruns do not retain Docker images. Recreate only the guarded
        # builder, never the release assets, when recovering an immutable bundle.
        present = subprocess.run([engine, "image", "inspect", image], capture_output=True)
        if present.returncode != 0:
            builder_context = source / "builder"
            builder_context.mkdir()
            image = release.build_image(engine, builder_context, (release.ROOT / "packaging/Containerfile").read_bytes())
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
    parser.add_argument("phase", choices=("preflight", "windows-recovery", "assemble", "github", "crate", "crate-native"))
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", type=Path, default=release.ROOT / "dist")
    parser.add_argument("--linux-output", type=Path)
    parser.add_argument("--windows-output", type=Path)
    args = parser.parse_args()
    token = os.environ.pop("CARGO_REGISTRY_TOKEN", None)
    try:
        if args.phase == "crate-native":
            publish_crate_native(args.output.resolve(), args.commit, token)
            return
        version = context(args.commit)
        if args.phase in {"preflight", "windows-recovery"}:
            if args.phase == "preflight":
                ready = preflight(version, args.commit)
                values = {"release": ready, "prerelease": release.version_info(version)[1],
                          "recover": ready and artifact_recovery(version, os.environ.get("GITHUB_RUN_ID", ""), "publication")}
            else:
                values = {"found": artifact_recovery(version, os.environ.get("GITHUB_RUN_ID", ""), "windows")}
            output = os.environ.get("GITHUB_OUTPUT")
            verify.require(bool(output), "Missing workflow output channel")
            with Path(output).open("a", encoding="utf-8") as file:
                for key, value in values.items():
                    file.write(f"{key}={'true' if value else 'false'}\n")
        elif args.phase == "assemble":
            verify.require(args.linux_output is not None and args.windows_output is not None,
                           "Assembly requires --linux-output and --windows-output")
            assemble(args.linux_output.resolve(), args.windows_output.resolve(), args.output.resolve(), version, args.commit)
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
