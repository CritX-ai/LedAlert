#!/usr/bin/env python3
"""Build local Linux release downloads; never stage, commit, tag, or publish."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import struct
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
FILES = (".gitignore", "Cargo.toml", "Cargo.lock", "README.md", "CHANGELOG.md", "RELEASE.md")
TREES = ("src", "tests", "assets", "packaging", "docs")
NOTICE_NAME = re.compile(r"(?:^|[-_.])(licen[cs]e|copying|notice|copyright|ofl|ufl)(?:$|[-_.])", re.I)
FONT_NOTICES = ("fonts/Hack-Regular.txt", "fonts/OFL.txt", "fonts/UFL.txt", "fonts/emoji-icon-font-mit-license.txt")


def command(*args: str, env: dict[str, str] | None = None) -> str:
    return subprocess.check_output(args, cwd=ROOT, env=env, text=True).strip()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def source_files() -> dict[str, Path]:
    files = {name: ROOT / name for name in FILES}
    for directory in TREES:
        for path in (ROOT / directory).rglob("*"):
            if path.is_symlink():
                raise ValueError(f"Source archive refuses symbolic links: {path.relative_to(ROOT)}")
            if path.is_file():
                relative = path.relative_to(ROOT)
                if "__pycache__" in relative.parts or path.suffix in {".pyc", ".tmp", ".log"}:
                    continue
                if any(part.startswith(".") for part in relative.parts):
                    raise ValueError(f"Unexpected hidden source input: {relative}")
                files[relative.as_posix()] = path
    for name, path in files.items():
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Missing or unsafe release source input: {name}")
    return dict(sorted(files.items()))


def metadata() -> dict:
    return json.loads(command("cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET))


def font_metadata(path: Path) -> bytes:
    """Retain copyright/licensing name records embedded in the shipped font."""
    data = path.read_bytes()
    count = struct.unpack_from(">H", data, 4)[0]
    records = set()
    for index in range(count):
        tag, _, offset, size = struct.unpack_from(">4sIII", data, 12 + index * 16)
        if tag != b"name":
            continue
        if offset + size > len(data):
            raise ValueError(f"Invalid font name table: {path.name}")
        _, entries, strings = struct.unpack_from(">HHH", data, offset)
        for entry in range(entries):
            platform, _, _, name_id, length, start = struct.unpack_from(">HHHHHH", data, offset + 6 + entry * 12)
            if name_id not in {0, 1, 5, 13, 14}:
                continue
            raw = data[offset + strings + start:offset + strings + start + length]
            if len(raw) != length:
                raise ValueError(f"Invalid font name record: {path.name}")
            text = raw.decode("utf-16-be" if platform in {0, 3} else "mac_roman")
            records.add((name_id, text))
    if not records:
        raise ValueError(f"Font has no readable name records: {path.name}")
    lines = [f"Embedded name-table records from {path.name}", f"Font SHA-256: {digest(data)}", ""]
    for name_id, text in sorted(records):
        lines.extend([f"Name ID {name_id}:", text, ""])
    return "\n".join(lines).encode()


def rust_notices() -> tuple[dict[str, bytes], dict]:
    sysroot = Path(command("rustc", "--print", "sysroot"))
    base = sysroot / "share/doc/rust"
    paths = [base / "COPYRIGHT-library.html", *sorted((base / "licenses").glob("*.txt"))]
    if len(paths) < 3 or not paths[0].is_file():
        raise ValueError("Install this toolchain's rust-docs component to supply Rust runtime notices")
    copied = {}
    inventory = []
    for path in paths:
        content = path.read_bytes()
        relative = path.relative_to(base).as_posix()
        destination = f"licenses/rust/{relative}"
        copied[destination] = content
        inventory.append({"path": destination, "sha256": digest(content), "toolchain_source": f"share/doc/rust/{relative}"})
    return copied, {"compiler": command("rustc", "-V"), "notices": inventory}


def notice_inputs(meta: dict) -> tuple[dict[str, bytes], list[dict]]:
    """Conservative target closure: includes development and build-time crates."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    pending = [meta["resolve"]["root"]]
    selected: set[str] = set()
    while pending:
        identifier = pending.pop()
        if identifier not in selected:
            selected.add(identifier)
            pending.extend(dep["pkg"] for dep in nodes[identifier]["deps"])
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    checksums = {(p["name"], p["version"]): p.get("checksum") for p in lock["package"]}
    overrides_path = ROOT / "packaging/licenses/index.json"
    overrides = json.loads(overrides_path.read_text())
    copied: dict[str, bytes] = {}
    inventory = []
    missing = []
    for package in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
        if package["id"] not in selected or package["source"] is None:
            continue
        name, version = package["name"], package["version"]
        key = f"{name}-{version}"
        base = Path(package["manifest_path"]).parent
        paths = set()
        for path in base.rglob("*"):
            if path.is_file() and not path.is_symlink() and NOTICE_NAME.search(path.name):
                if path.suffix.lower() not in {".rs", ".toml", ".json", ".svg", ".png", ".ttf"}:
                    paths.add(path.relative_to(base).as_posix())
        declared = package.get("license_file")
        if declared:
            resolved = (base / declared).resolve()
            if not resolved.is_relative_to(base.resolve()) or not resolved.is_file():
                raise ValueError(f"Unsafe or missing declared license file for {key}")
            paths.add(resolved.relative_to(base.resolve()).as_posix())
        if name == "epaint_default_fonts":
            paths.update(FONT_NOTICES)
        notices = []
        for relative in sorted(paths):
            path = base / relative
            if not path.is_file() or path.is_symlink():
                raise ValueError(f"Missing notice: {key}/{relative}")
            content = path.read_bytes()
            destination = f"licenses/crates/{key}/{relative}"
            copied[destination] = content
            notices.append({"path": destination, "sha256": digest(content), "source": f"crate:{relative}"})
        for entry in overrides.get("packages", {}).get(key, []):
            path = (ROOT / "packaging/licenses" / entry["file"]).resolve()
            if not path.is_relative_to((ROOT / "packaging/licenses").resolve()):
                raise ValueError(f"Unsafe notice override for {key}")
            content = path.read_bytes()
            if digest(content) != entry["sha256"]:
                raise ValueError(f"Notice digest mismatch for {key}: {entry['file']}")
            destination = "licenses/upstream/" + entry["file"]
            copied[destination] = content
            notices.append({"path": destination, "sha256": entry["sha256"], "source": entry["source"]})
        if not notices or not package.get("license"):
            missing.append(key)
        inventory.append({"name": name, "version": version, "license_expression": package.get("license"),
                          "registry_source": package["source"], "crate_sha256": checksums[(name, version)], "notices": notices})
        if name == "epaint_default_fonts":
            for font in sorted((base / "fonts").glob("*.ttf")):
                destination = f"licenses/crates/{key}/fonts/{font.stem}-metadata.txt"
                content = font_metadata(font)
                copied[destination] = content
                notices.append({"path": destination, "sha256": digest(content), "source": f"font-name-table:fonts/{font.name}"})
    if missing:
        raise ValueError("Unresolved dependency notices: " + ", ".join(missing))
    copied["licenses/Silkscreen-OFL.txt"] = (ROOT / "assets/fonts/OFL.txt").read_bytes()
    copied["licenses/Silkscreen-metadata.txt"] = font_metadata(ROOT / "assets/fonts/Silkscreen-Bold.ttf")
    runtime_files, runtime_inventory = rust_notices()
    copied.update(runtime_files)
    copied["licenses/INVENTORY.json"] = json_bytes({"scope": "Resolved Linux target closure, including development and build-time crates; not a claim that every listed crate is linked into the executable.",
                                                   "target": TARGET, "packages": inventory, "rust_runtime": runtime_inventory,
                                                   "application_font": {"file": "assets/fonts/Silkscreen-Bold.ttf", "notice": "licenses/Silkscreen-OFL.txt"}})
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    lines = [f"LedAlert {version} — third-party notices", "",
             "No application license is granted by this notice. Third-party components retain their own terms.",
             "The licenses/ directory contains verbatim license and attribution inputs; INVENTORY.json maps them to versions and digests.",
             "This conservative inventory includes build-time and development dependencies. It is not a legal certification.", ""]
    lines.extend(f"{p['name']} {p['version']}: {p['license_expression']}" for p in inventory)
    lines.extend(["", "Bundled Silkscreen: SIL Open Font License 1.1; see licenses/Silkscreen-OFL.txt.",
                  "Embedded egui fonts include Hack, Noto Emoji, Ubuntu and emoji-icon-font; see the font notices in the epaint_default_fonts entry.",
                  "Additional upstream/Rust runtime notices and their provenance are recorded in licenses/INVENTORY.json.", ""])
    copied["THIRD-PARTY-NOTICES.txt"] = "\n".join(lines).encode()
    return copied, inventory


def archive(path: Path, prefix: str, entries: dict[str, tuple[bytes, int]], epoch: int) -> None:
    with path.open("wb") as destination:
        with gzip.GzipFile(filename="", mode="wb", fileobj=destination, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
                for relative, (content, mode) in sorted(entries.items()):
                    member = tarfile.TarInfo(f"{prefix}/{relative}")
                    member.size, member.mode, member.mtime = len(content), mode, epoch
                    member.uid = member.gid = 0
                    member.uname = member.gname = ""
                    tar.addfile(member, io.BytesIO(content))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist", help="Empty destination (default: dist/)")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    names = [f"ledalert-{version}-linux-x86_64.tar.gz", f"ledalert-{version}-source.tar.gz", "SHA256SUMS"]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any((output / name).exists() for name in names):
        parser.error("Release filenames already exist; choose another empty --output directory rather than overwrite a release.")
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    if epoch < 0:
        parser.error("SOURCE_DATE_EPOCH must be nonnegative")
    rustc = command("rustc", "-Vv")
    if f"host: {TARGET}" not in rustc.splitlines():
        parser.error(f"This release helper supports a native {TARGET} build only")
    sources = source_files()
    source_hashes = {name: digest(path.read_bytes()) for name, path in sources.items()}
    notices, dependencies = notice_inputs(metadata())
    env = os.environ.copy()
    env.pop("RUSTFLAGS", None)
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
        "-C", "target-cpu=x86-64",
        f"--remap-path-prefix={Path.home()}=/usr/src/build",
        f"--remap-path-prefix={ROOT}=/usr/src/ledalert",
    ])
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", TARGET, "--bin", "ledalert"], cwd=ROOT, env=env, check=True)
    binary = ROOT / "target" / TARGET / "release/ledalert"
    if command(str(binary), "--version") != f"LedAlert {version}":
        raise ValueError("Built executable version does not match Cargo.toml")
    versions = command("readelf", "--version-info", str(binary))
    glibc = sorted(set(re.findall(r"GLIBC_(\d+(?:\.\d+)+)", versions)), key=lambda value: tuple(map(int, value.split("."))))
    needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", command("readelf", "-d", str(binary)))
    executable = binary.read_bytes()
    info = {"application": "LedAlert", "version": version, "target": TARGET, "target_cpu": "x86-64",
            "rustc": rustc, "cargo": command("cargo", "-V"), "glibc_required": glibc[-1] if glibc else None,
            "direct_shared_libraries": needed, "binary_sha256": digest(executable), "source_files": source_hashes,
            "source_manifest_sha256": digest(json_bytes(source_hashes)), "cargo_lock_sha256": source_hashes["Cargo.lock"],
            "dependency_packages": len(dependencies), "archive_epoch": epoch,
            "provenance": "Working-tree source inventory; no commit, tag, signature, publication or reproducible-build attestation is implied."}
    if source_hashes != {name: digest(path.read_bytes()) for name, path in source_files().items()}:
        raise ValueError("Source inputs changed during build; rerun from a stable tree")
    common = {name: (content, 0o644) for name, content in notices.items()}
    common["BUILD-INFO.json"] = (json_bytes(info), 0o644)
    source_entries = dict(common)
    source_entries.update({name: (path.read_bytes(), 0o644) for name, path in sources.items()})
    binary_entries = dict(common)
    binary_entries["bin/ledalert"] = (executable, 0o755)
    for name, path in sources.items():
        if name in {"README.md", "CHANGELOG.md", "RELEASE.md"} or name.startswith(("docs/", "assets/")) or name in {
                "packaging/io.github.critx.LedAlert.desktop", "packaging/io.github.critx.LedAlert.svg"}:
            binary_entries[name] = (path.read_bytes(), 0o644)
    with tempfile.TemporaryDirectory(prefix=".ledalert-package-", dir=output) as temporary:
        staging = Path(temporary)
        archive(staging / names[0], f"ledalert-{version}-linux-x86_64", binary_entries, epoch)
        archive(staging / names[1], f"ledalert-{version}-source", source_entries, epoch)
        (staging / names[2]).write_text("".join(f"{digest((staging / name).read_bytes())}  {name}\n" for name in names[:2]))
        for name in names:
            shutil.move(staging / name, output / name)
    print(json.dumps({"artifacts": [str(output / name) for name in names], "glibc_required": info["glibc_required"],
                      "source_files": len(sources), "dependency_packages": len(dependencies)}, indent=2))


if __name__ == "__main__":
    main()
