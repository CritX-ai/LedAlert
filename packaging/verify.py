#!/usr/bin/env python3
"""Verify local LedAlert artifacts; never publish or grant distribution rights."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib

import release

ROOT = Path(__file__).resolve().parents[1]
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_MEMBER_BYTES = 128 * 1024 * 1024
MAX_MEMBERS = 20000
SHA256 = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def report(stage: str, **details: object) -> None:
    print(json.dumps({"verification": stage, **details}, sort_keys=True), flush=True)


def file_digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def safe_name(name: str) -> bool:
    return bool(name) and not name.startswith("/") and "\\" not in name and all(
        part not in {"", ".", ".."} for part in name.split("/")
    )


def read_archive(path: Path, prefix: str) -> dict[str, tuple[bytes, int]]:
    """Read regular allowlisted members only; never invoke tar extraction."""
    require(path.is_file() and not path.is_symlink(), f"Missing or unsafe archive: {path.name}")
    require(path.stat().st_size <= MAX_ARCHIVE_BYTES, f"Archive too large: {path.name}")
    result = {}
    total = 0
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            require(len(result) < MAX_MEMBERS, "Archive member limit exceeded")
            require(safe_name(member.name), f"Unsafe archive path: {member.name!r}")
            require(member.name.startswith(prefix + "/"), f"Unexpected archive root: {member.name}")
            name = member.name[len(prefix) + 1:]
            require(member.isfile() and not member.sparse, f"Non-regular archive member: {name}")
            require(name not in result, f"Duplicate archive member: {name}")
            require(0 <= member.size <= MAX_MEMBER_BYTES, f"Oversized archive member: {name}")
            total += member.size
            require(total <= MAX_ARCHIVE_BYTES, "Expanded archive size limit exceeded")
            mode = 0o755 if name == "bin/ledalert" else 0o644
            require(member.mode == mode, f"Unexpected archive permissions: {name}")
            require(member.uid == member.gid == 0 and not member.uname and not member.gname,
                    f"Unexpected archive ownership: {name}")
            source = archive.extractfile(member)
            require(source is not None, f"Unreadable archive member: {name}")
            with source:
                data = source.read(MAX_MEMBER_BYTES + 1)
            require(len(data) == member.size, f"Truncated archive member: {name}")
            result[name] = (data, mode)
    return result


def read_json(data: bytes, label: str) -> dict:
    value = json.loads(data)
    require(isinstance(value, dict), f"Expected JSON object: {label}")
    return value


def first_party_license(root: Path) -> dict:
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    declared = package.get("license") or package.get("license-file")
    if not declared:
        return {"status": "not_selected"}
    names = [package["license-file"]] if package.get("license-file") else [
        path.name for path in root.iterdir() if re.match(r"^(?:licen[cs]e|copying)(?:$|[-_.])", path.name, re.I)
    ]
    require(bool(names), "First-party license metadata exists without selected license text")
    for name in names:
        require(safe_name(name), "Unsafe first-party license path")
        path = root / name
        require(path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root.resolve()) and bool(path.read_bytes().strip()),
                f"Missing first-party license text: {name}")
    return {"status": "declared", "files": sorted(names)}


def require_license(root: Path) -> None:
    status = first_party_license(root)
    report("first_party_license", **status)
    require(status["status"] == "declared",
            "REL-01 blocked: first-party license not_selected; owner-approved terms are required")


def require_distribution(root: Path) -> None:
    require_license(root)
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    require(package.get("publish") == ["crates-io"], "Distribution requires crates.io-only publication metadata")
    require(package.get("license") == "MIT OR Apache-2.0" and
            {"LICENSE-MIT", "LICENSE-APACHE"}.issubset(first_party_license(root)["files"]),
            "Distribution requires the owner-selected MIT OR Apache-2.0 expression and both license texts")
    require(package.get("homepage") == "https://alert.critx.ai/" and
            package.get("repository") == "https://github.com/CritX-ai/LedAlert" and
            bool(package.get("description")) and bool(package.get("categories")) and bool(package.get("keywords")),
            "Missing public crate metadata")
    includes = package.get("include", [])
    require(isinstance(includes, list) and bool(includes), "Cargo requires an explicit public include set")
    for name in first_party_license(root)["files"]:
        require("/" + name in includes, f"Add the selected license to Cargo include: /{name}")


def inspect_crate(root: Path, path: Path) -> None:
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    entries = read_archive(path, f"ledalert-{package['version']}")
    licenses = set(first_party_license(root).get("files", []))
    fixed = {"Cargo.toml", "Cargo.lock", "README.md", "assets/ledalert.png",
             "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt"} | licenses
    expected = {
        name: source for name, source in release.source_files(root).items()
        if name in fixed or
        (name.startswith(("src/", "tests/")) and name.endswith(".rs")) or
        name.startswith("packaging/licenses/")
    }
    require(fixed.issubset(expected), "Missing crate source, embedded asset or license")
    require(set(entries) == set(expected) | {"Cargo.toml.orig"},
            "Cargo archive differs from the public application-source allowlist")
    for name, source in expected.items():
        packaged = "Cargo.toml.orig" if name == "Cargo.toml" else name
        require(entries[packaged][0] == source.read_bytes(), f"Cargo source mismatch: {name}")
    normalized = tomllib.loads(entries["Cargo.toml"][0].decode())["package"]
    require(normalized["name"] == "ledalert" and normalized["version"] == package["version"],
            "Cargo archive package identity mismatch")
    report("crate_inventory", status="passed", version=package["version"], sha256=file_digest(path))


def check_crate(root: Path) -> Path:
    """Exercise Cargo's real package verification and registry dry run without credentials."""
    require_distribution(root)
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    with isolated_environment() as env:
        target = Path(env.get("CARGO_TARGET_DIR", str(root / "target")))
        path = target / "package" / f"ledalert-{version}.crate"
        run(["cargo", "package", "--locked"], root, env, 1800)
        inspect_crate(root, path)
        run(["cargo", "test", "--locked"], target / "package" / f"ledalert-{version}", env, 1800)
        checksum = file_digest(path)
        run(["cargo", "publish", "--dry-run", "--locked", "--registry", "crates-io"], root, env, 1800)
        require(file_digest(path) == checksum, "Cargo dry run changed the verified package bytes")
        inspect_crate(root, path)
    return path


def inspect_artifacts(root: Path, output: Path) -> tuple[dict, dict, dict]:
    """Validate the pair against this source tree before extracting or executing it."""
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?", version) is not None,
            "Unsafe package version")
    prefixes = [f"ledalert-{version}-linux-x86_64", f"ledalert-{version}-source"]
    names = [prefix + ".tar.gz" for prefix in prefixes]
    crate_name = f"ledalert-{version}.crate"
    if (output / crate_name).exists():
        names.append(crate_name)
    sums_path = output / "SHA256SUMS"
    require(sums_path.is_file() and not sums_path.is_symlink(), "Missing or unsafe SHA256SUMS")
    require(sums_path.stat().st_size <= 4096, "Oversized SHA256SUMS")
    checksums = {}
    for line in sums_path.read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9.+_-]+\.(?:tar\.gz|crate))", line)
        require(match is not None, "Malformed SHA256SUMS entry")
        checksum, name = match.groups()
        require(name in names and name not in checksums, f"Unexpected checksum entry: {name}")
        checksums[name] = checksum
    require(set(checksums) == set(names), "SHA256SUMS must cover exactly the release archives and optional crate")
    for name in names:
        path = output / name
        require(path.is_file() and not path.is_symlink(), f"Missing or unsafe archive: {name}")
        require(path.stat().st_size <= MAX_ARCHIVE_BYTES, f"Archive too large: {name}")
        require(file_digest(path) == checksums[name], f"Checksum mismatch: {name}")
    binary, source = [read_archive(output / name, prefix) for name, prefix in zip(names[:2], prefixes)]
    require("BUILD-INFO.json" in source and "BUILD-INFO.json" in binary, "Missing BUILD-INFO.json")
    require(source["BUILD-INFO.json"] == binary["BUILD-INFO.json"], "Archive BUILD-INFO mismatch")
    info = read_json(source["BUILD-INFO.json"][0], "BUILD-INFO.json")
    if crate_name in checksums:
        require_distribution(root)
        require("source_commit" in info and (info["source_commit"] is None or
                isinstance(info["source_commit"], str) and re.fullmatch(r"[0-9a-f]{40}", info["source_commit"]) is not None),
                "Cargo provenance requires a full source commit or explicit null for a local snapshot")
        require(info.get("crate_sha256") == checksums[crate_name], "Cargo package provenance mismatch")
        inspect_crate(root, output / crate_name)
    else:
        require("crate_sha256" not in info and "source_commit" not in info, "Missing recorded Cargo distribution")
    require(info.get("application") == "LedAlert" and info.get("version") == version, "Package version mismatch")
    require(info.get("target") == release.TARGET and info.get("target_cpu") == "x86-64", "Unexpected binary target")
    baseline = info.get("baseline", {})
    require(all(baseline.get(key) == value for key, value in {
        "distribution": "debian", "version": "12", "codename": "bookworm",
        "glibc_max": "2.36", "rust_toolchain": "1.95.0",
    }.items()), "Unexpected build baseline")
    require(baseline.get("container_image") == release.BASE_IMAGE, "Unrecognized baseline image")
    require(re.search(r"@sha256:[0-9a-f]{64}$", baseline["container_image"]) is not None,
            "Baseline image must be digest-pinned")
    require(str(info.get("rustc", "")).startswith("rustc 1.95.0 ") and
            str(info.get("cargo", "")).startswith("cargo 1.95.0 "), "Unexpected recorded toolchain")
    glibc = info.get("glibc_required", "")
    require(isinstance(glibc, str) and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", glibc) is not None and
            tuple(map(int, glibc.split("."))) <= (2, 36), "Binary exceeds glibc 2.36 ceiling")
    builder = info.get("builder", {})
    require(builder.get("os_release") == {"ID": "debian", "VERSION_ID": "12", "VERSION_CODENAME": "bookworm"} and
            builder.get("glibc") == "2.36", "Unexpected builder runtime")
    hashes = info.get("source_files")
    require(isinstance(hashes, dict) and set(release.FILES).issubset(hashes), "Missing required source inventory")
    require({"src/main.rs", "src/lib.rs", "packaging/verify.py", "packaging/test_release.py",
             "packaging/Containerfile", ".github/workflows/verify.yml"}.issubset(hashes),
            "Missing source/build/verification definitions")
    expected_sources = release.source_files(root)
    require(set(hashes) == set(expected_sources), "Source inventory differs from the allowlisted verification tree")
    legal_names = {path.name for path in root.iterdir() if release.FIRST_PARTY_NOTICE.match(path.name)}
    declared_license = tomllib.loads((root / "Cargo.toml").read_text())["package"].get("license-file")
    if declared_license:
        require(safe_name(declared_license) and not any(part.startswith(".") for part in declared_license.split("/")),
                "Unsafe declared first-party license path")
        legal_names.add(declared_license)
    for name, checksum in hashes.items():
        require(isinstance(checksum, str) and SHA256.fullmatch(checksum) is not None, f"Invalid source hash: {name}")
        require(name in source and release.digest(source[name][0]) == checksum, f"Source hash mismatch: {name}")
        require(file_digest(expected_sources[name]) == checksum, f"Source differs from verification tree: {name}")
    require(info.get("source_manifest_sha256") == release.digest(release.json_bytes(hashes)), "Source manifest hash mismatch")
    require(info.get("cargo_lock_sha256") == hashes["Cargo.lock"], "Cargo.lock hash mismatch")
    require(builder.get("containerfile_sha256") == hashes["packaging/Containerfile"], "Builder recipe hash mismatch")
    require("bin/ledalert" in binary and release.digest(binary["bin/ledalert"][0]) == info.get("binary_sha256"),
            "Executable hash mismatch")
    require(binary["bin/ledalert"][0].startswith(b"\x7fELF\x02\x01"), "Expected Linux x86_64 ELF executable")
    require("licenses/INVENTORY.json" in source and "THIRD-PARTY-NOTICES.txt" in source,
            "Missing third-party notices")
    inventory = read_json(source["licenses/INVENTORY.json"][0], "licenses/INVENTORY.json")
    require(inventory.get("target") == release.TARGET, "Notice inventory target mismatch")
    packages = inventory.get("packages")
    require(isinstance(packages, list) and bool(packages) and len(packages) == info.get("dependency_packages"),
            "Dependency notice count mismatch")
    locked = {(p["name"], p["version"]): p for p in tomllib.loads(source["Cargo.lock"][0].decode())["package"]}
    referenced = {"licenses/INVENTORY.json", "licenses/Silkscreen-OFL.txt", "licenses/Silkscreen-metadata.txt"}
    seen = set()
    for package in packages:
        key = (package["name"], package["version"])
        require(key not in seen and key in locked, f"Unexpected notice package: {key}")
        seen.add(key)
        require(bool(package.get("license_expression")) and bool(package.get("notices")), f"Missing dependency terms: {key}")
        require(package.get("crate_sha256") == locked[key].get("checksum") and
                package.get("registry_source") == locked[key].get("source"), f"Dependency provenance mismatch: {key}")
    metadata = release.metadata(root)
    expected_packages = {
        (package["name"], package["version"])
        for package in release.dependency_packages(metadata)
    }
    require(seen == expected_packages, "Dependency notices differ from the locked target dependency graph")
    # Archive hashes establish consistency, not the actual attribution obligation.
    # Reconstruct all required terms from locked crate sources, pinned overrides,
    # the application/documentation fonts and this toolchain rather than trusting the inventory.
    trusted_notices, _ = release.notice_inputs(metadata, root)
    for name, content in trusted_notices.items():
        require(source.get(name) == (content, 0o644),
                f"Notice differs from trusted dependency/font/runtime inputs: {name}")
    referenced.update(name for name in trusted_notices if name.startswith("licenses/"))
    runtime = inventory.get("rust_runtime", {})
    require(runtime.get("compiler") == info["rustc"].splitlines()[0] and bool(runtime.get("notices")),
            "Missing or inconsistent Rust runtime notices")
    for entry in [notice for package in packages for notice in package["notices"]] + runtime["notices"]:
        name = entry["path"]
        require(safe_name(name) and name.startswith("licenses/") and name in source,
                f"Missing or unsafe notice: {name}")
        require(bool(source[name][0].strip()) and release.digest(source[name][0]) == entry.get("sha256"),
                f"Notice hash mismatch: {name}")
        referenced.add(name)
    require(referenced.issubset(source), "Missing application font notices")
    require(source["licenses/Silkscreen-OFL.txt"][0] == source["assets/fonts/OFL.txt"][0], "Application font notice mismatch")
    require(bool(source["licenses/Silkscreen-metadata.txt"][0].strip()) and bool(source["THIRD-PARTY-NOTICES.txt"][0].strip()),
            "Empty third-party notices")
    common = referenced | {"BUILD-INFO.json", "THIRD-PARTY-NOTICES.txt"}
    require(set(source) == set(hashes) | common, "Unexpected source archive members")
    bundled = {name for name in hashes if name in {"README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md",
        "packaging/io.github.critx.LedAlert.desktop", "packaging/io.github.critx.LedAlert.svg"} or
        name.startswith(("docs/", "assets/"))}
    license_status = first_party_license(root)
    require(info.get("first_party_license", {}).get("status") == license_status["status"], "First-party license status mismatch")
    for name in license_status.get("files", []):
        require(name in hashes, f"First-party license missing from source: {name}")
        bundled.add(name)
    bundled.update(legal_names)
    require(set(binary) == common | bundled | {"bin/ledalert"}, "Unexpected or missing binary archive members")
    for name in common | bundled:
        require(binary[name] == source[name], f"Archive content mismatch: {name}")
    report("artifact_inventory", status="passed", version=version, source_files=len(hashes),
           dependency_packages=len(packages), first_party_license=license_status["status"],
           distribution_ready=False if license_status["status"] == "not_selected" else "not_assessed")
    return info, binary, source


@contextmanager
def isolated_environment():
    with tempfile.TemporaryDirectory(prefix="ledalert-verify-home-") as temporary:
        home = Path(temporary)
        env = {key: os.environ[key] for key in ("PATH", "CARGO_HOME", "RUSTUP_HOME", "SOURCE_DATE_EPOCH",
               "CARGO_TARGET_DIR", "CARGO_ENCODED_RUSTFLAGS") if key in os.environ}
        env.update(HOME=str(home), LC_ALL="C.UTF-8", LANG="C.UTF-8", PYTHONDONTWRITEBYTECODE="1",
                   LIBGL_ALWAYS_SOFTWARE="1", GALLIUM_DRIVER="llvmpipe", WINIT_UNIX_BACKEND="x11")
        for name, suffix in (("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                             ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state"), ("XDG_RUNTIME_DIR", "run")):
            directory = home / suffix
            directory.mkdir(mode=0o700)
            env[name] = str(directory)
        env["DBUS_SESSION_BUS_ADDRESS"] = f"unix:path={home}/absent-session-bus"
        env["DBUS_SYSTEM_BUS_ADDRESS"] = f"unix:path={home}/absent-system-bus"
        yield env


def run(args: list[str], cwd: Path, env: dict[str, str], timeout: int = 60, success: bool = True) -> str:
    """Bound command lifetime and output; kill descendants as well on timeout."""
    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen(args, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise ValueError(f"Command timed out after {timeout}s: {' '.join(args)}") from None
        log.seek(0, os.SEEK_END)
        length = log.tell()
        log.seek(max(0, length - 1024 * 1024))
        text = log.read().decode(errors="replace")
        require(length <= 1024 * 1024, f"Command output exceeded 1 MiB: {' '.join(args)}\n{text[-16000:]}")
    require((code == 0) if success else (code != 0),
            f"Unexpected exit {code}: {' '.join(args)}\n{text[-16000:]}")
    report("command", command=args, status="passed", expected_exit="zero" if success else "nonzero")
    return text


def check_source(root: Path) -> None:
    root = root.resolve()
    with isolated_environment() as env:
        for args, timeout in [
            (["cargo", "fmt", "--all", "--", "--check"], 120),
            (["cargo", "clippy", "--all-targets", "--locked", "--", "-D", "warnings"], 1800),
            (["cargo", "test", "--locked"], 1800),
            ([sys.executable, "-B", "-m", "unittest", "discover", "-s", "packaging", "-p", "test_release.py"], 120),
        ]:
            run(args, root, env, timeout)
    report("source", status="passed")


def extract(entries: dict[str, tuple[bytes, int]], destination: Path) -> None:
    destination.mkdir()
    for name, (content, mode) in entries.items():
        require(safe_name(name), f"Unsafe extraction path: {name}")
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as output:
            output.write(content)
        path.chmod(mode)


def synthetic_config() -> dict:
    return {
        "version": 2, "device": {"address": "127.0.0.1", "led_count": 60},
        "room": {"width": 5.0, "depth": 4.0, "height": 2.5,
                 "screens": [{"id": 1, "name": "Verification display", "position": {"x": 2.5, "y": 0.8, "z": 1.2},
                              "width": 0.7, "angle": 0.0, "connector": None, "aspect_ratio": 16.0 / 9.0}],
                 "strip": [{"x": 0.0, "y": 0.0, "z": 1.2}, {"x": 5.0, "y": 0.0, "z": 1.2}],
                 "reverse": False, "led_anchors": []},
        "rules": [{"application": "*", "screen_id": 1, "color": [80, 175, 220], "duration": 4.0,
                   "spread": 1.2, "enabled": True, "options": {"notifications": True, "media": True,
                   "minimum_urgency": 0, "intensity": 1.0, "fade": True, "critical_accent": True,
                   "effect": "glow", "range_unit": "room", "gradient": [], "mode": "one_off", "position": None}}],
        "brightness": 0.15, "reduced_motion": False, "notifications_enabled": True, "media_enabled": True,
    }


def window_smoke(binary: Path, config: Path) -> None:
    """Internal child: only called beneath a fresh Xvfb and dbus-run-session."""
    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen([str(binary), "--config", str(config), "gui"], stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline and process.poll() is None:
                search = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid), "--name", "LedAlert"],
                                        capture_output=True, text=True, timeout=3)
                if search.returncode == 0 and search.stdout.strip():
                    window = search.stdout.splitlines()[0]
                    break
                time.sleep(0.1)
            require(window is not None, "Native launch did not expose a visible LedAlert window")
            view = subprocess.check_output(["xwininfo", "-id", window], text=True, timeout=3)
            require("Map State: IsViewable" in view, "Native window is not viewable")
            time.sleep(2)
            require(process.poll() is None, "Native application exited during the launch smoke")
            report("native_launch", status="passed", display="private Xvfb", bus="private D-Bus",
                   rendering="software", observed="visible window remained alive for two seconds",
                   desktop_integration="not_assessed", hardware="not_accessed")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            log.seek(0, os.SEEK_END)
            length = log.tell()
            log.seek(max(0, length - 8000))
            diagnostics = log.read().decode(errors="replace")
            if diagnostics:
                print(diagnostics, file=sys.stderr)


def check_artifacts(root: Path, output: Path) -> None:
    root = root.resolve()
    info, binary_entries, source_entries = inspect_artifacts(root, output.resolve())
    with tempfile.TemporaryDirectory(prefix="ledalert-artifacts-") as temporary, isolated_environment() as env:
        work = Path(temporary)
        bundle, source = work / "binary", work / "source"
        extract(binary_entries, bundle)
        extract(source_entries, source)
        binary = bundle / "bin/ledalert"
        actual = run([str(binary), "--version"], bundle, env).strip()
        require(actual == f"LedAlert {info['version']}", "Extracted executable version mismatch")
        help_text = run([str(binary), "--help"], bundle, env)
        require("check-config" in help_text and "--config" in help_text, "Missing documented command surface")
        symbols = run(["readelf", "--version-info", str(binary)], bundle, env)
        versions = set(re.findall(r"GLIBC_(\d+(?:\.\d+)+)", symbols))
        require(bool(versions) and "GLIBC_PRIVATE" not in symbols, "Cannot measure public glibc requirements")
        measured = max(versions, key=lambda value: tuple(map(int, value.split("."))))
        require(measured == info["glibc_required"], "Recorded and measured glibc requirements differ")
        dynamic = run(["readelf", "-d", str(binary)], bundle, env)
        needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic)
        require(needed == info.get("direct_shared_libraries"), "Recorded shared-library requirements differ")
        config = work / "config.json"
        setup = synthetic_config()
        config.write_bytes(release.json_bytes(setup))
        original = config.read_bytes()
        run([str(binary), "--config", str(config), "check-config"], bundle, env)
        require(config.read_bytes() == original, "check-config modified the saved setup")
        old = synthetic_config()
        old["version"] = 1
        del old["room"]["led_anchors"]
        del old["room"]["screens"][0]["connector"]
        del old["room"]["screens"][0]["aspect_ratio"]
        del old["rules"][0]["options"]
        old_path = work / "old-v1.json"
        old_path.write_bytes(release.json_bytes(old))
        run([str(binary), "--config", str(old_path), "check-config"], bundle, env)
        require(old_path.read_bytes() == release.json_bytes(old), "Old setup validation modified its file")
        invalid = work / "invalid.json"
        setup["rules"][0]["screen_id"] = 999
        invalid.write_bytes(release.json_bytes(setup))
        run([str(binary), "--config", str(invalid), "check-config"], bundle, env, success=False)
        missing = work / "missing.json"
        run([str(binary), "--config", str(missing), "check-config"], bundle, env, success=False)
        require(not missing.exists(), "Missing-file validation unexpectedly created a setup")
        env["CARGO_TARGET_DIR"] = str(work / "target")
        run(["cargo", "build", "--locked", "--release", "--target", release.TARGET, "--bin", "ledalert"], source, env, 1800)
        rebuilt = work / "target" / release.TARGET / "release/ledalert"
        require(run([str(rebuilt), "--version"], source, env).strip() == f"LedAlert {info['version']}",
                "Released-source executable version mismatch")
        save_result = run(["cargo", "test", "--locked", "--test", "core", "invalid_save_preserves_previous_config_and_roundtrip_is_exact",
                           "--", "--exact"], source, env, 1800)
        require("1 passed; 0 failed" in save_result, "Released-source invalid-save regression did not execute")
        needed_tools = ("xvfb-run", "Xvfb", "dbus-run-session", "xdotool", "xwininfo")
        require(all(shutil.which(name, path=env.get("PATH")) for name in needed_tools),
                "Native artifact smoke requires xvfb, xauth, dbus, xdotool and x11-utils; use release.py --verify")
        smoke_result = run(["xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp", "dbus-run-session", "--",
                            sys.executable, "-B", str(root / "packaging/verify.py"), "--window-smoke", str(binary),
                            "--config", str(config)], bundle, env, 45)
        print(smoke_result, flush=True)
        require(config.read_bytes() == original, "Launch smoke modified the saved setup")
    report("artifacts", status="passed", first_party_license=first_party_license(root)["status"],
           scope="local technical checks; not release authorization or desktop/hardware acceptance")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Verify an existing artifact pair against this source tree, then build and launch it")
    parser.add_argument("--require-license", action="store_true", help="Fail unless selected first-party license metadata and text exist (REL-01)")
    parser.add_argument("--require-distribution", action="store_true", help="Require owner-selected license and crates.io publication metadata")
    parser.add_argument("--window-smoke", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--config", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        if args.window_smoke:
            require(args.config is not None and not args.output and not args.require_license and not args.require_distribution,
                    "Invalid internal smoke arguments")
            window_smoke(args.window_smoke, args.config)
            return
        require(args.output is not None or args.require_license or args.require_distribution,
                "Specify --output DIR, --require-license or --require-distribution")
        if args.require_license:
            require_license(ROOT)
        if args.require_distribution:
            require_distribution(ROOT)
        if args.output:
            check_artifacts(ROOT, args.output)
    except (ValueError, OSError, KeyError, TypeError, tarfile.TarError, subprocess.SubprocessError) as error:
        report("failed", error=str(error))
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
