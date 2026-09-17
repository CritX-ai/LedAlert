#!/usr/bin/env python3
"""Build local Linux release downloads in the pinned baseline; never publish."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import struct
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
RUST_TOOLCHAIN = "1.95.0"
GLIBC_MAX = "2.36"
BASE_IMAGE = "docker.io/library/rust:1.95.0-slim-bookworm@sha256:6f9e63259f12e1e599296f5ecfed2bae46de4af0ee0525dd8b89c046e236d5c5"
BASELINE = {"distribution": "debian", "version": "12", "codename": "bookworm",
            "glibc_max": GLIBC_MAX, "rust_toolchain": RUST_TOOLCHAIN, "container_image": BASE_IMAGE}
BUILDER_RECIPE = Path("/usr/local/share/ledalert/Containerfile")
FILES = (".gitignore", ".github/workflows/verify.yml", ".github/workflows/pages.yml",
         ".github/workflows/release.yml", "Cargo.toml", "Cargo.lock",
         "README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md", "packaging/Containerfile")
TREES = ("src", "tests", "assets", "packaging", "docs")
NOTICE_NAME = re.compile(r"(?:^|[-_.])(licen[cs]e|copying|notice|copyright|ofl|ufl)(?:$|[-_.])", re.I)
FIRST_PARTY_NOTICE = re.compile(r"^(?:licen[cs]e|copying|notice|copyright)(?:$|[-_.])", re.I)
FONT_NOTICES = ("fonts/Hack-Regular.txt", "fonts/OFL.txt", "fonts/UFL.txt", "fonts/emoji-icon-font-mit-license.txt")


def command(*args: str, env: dict[str, str] | None = None, cwd: Path | None = None) -> str:
    return subprocess.check_output(args, cwd=ROOT if cwd is None else cwd, env=env, text=True).strip()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def source_files(root: Path | None = None) -> dict[str, Path]:
    root = ROOT if root is None else root
    files = {name: root / name for name in FILES}
    for directory in TREES:
        tree = root / directory
        if tree.is_symlink():
            raise ValueError(f"Source archive refuses symbolic links: {directory}")
        for path in tree.rglob("*"):
            if path.is_symlink():
                raise ValueError(f"Source archive refuses symbolic links: {path.relative_to(root)}")
            if path.is_file():
                relative = path.relative_to(root)
                if "__pycache__" in relative.parts or path.suffix in {".pyc", ".tmp", ".log"}:
                    continue
                if any(part.startswith(".") for part in relative.parts):
                    raise ValueError(f"Unexpected hidden source input: {relative}")
                files[relative.as_posix()] = path
    for path in root.iterdir():
        if FIRST_PARTY_NOTICE.match(path.name):
            files[path.name] = path
    declared = tomllib.loads((root / "Cargo.toml").read_text())["package"].get("license-file")
    if declared:
        relative = Path(declared)
        if relative.is_absolute() or ".." in relative.parts or any(part.startswith(".") for part in relative.parts):
            raise ValueError("The application license-file must be a visible path inside the source tree")
        files[relative.as_posix()] = root / relative
    for name, path in files.items():
        parents = (path, *path.parents)
        if not path.is_file() or any(p.is_symlink() for p in parents if p.is_relative_to(root)):
            raise ValueError(f"Missing or unsafe release source input: {name}")
    return dict(sorted(files.items()))


def metadata(root: Path | None = None) -> dict:
    return json.loads(command("cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET, cwd=root))


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


def dependency_packages(meta: dict) -> list[dict]:
    """Conservative target closure: includes development and build-time crates."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    pending = [meta["resolve"]["root"]]
    selected: set[str] = set()
    while pending:
        identifier = pending.pop()
        if identifier not in selected:
            selected.add(identifier)
            pending.extend(dep["pkg"] for dep in nodes[identifier]["deps"])
    return sorted((package for package in meta["packages"]
                   if package["id"] in selected and package["source"] is not None),
                  key=lambda package: (package["name"], package["version"]))


def notice_inputs(meta: dict, root: Path | None = None) -> tuple[dict[str, bytes], list[dict]]:
    root = ROOT if root is None else root
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    checksums = {(p["name"], p["version"]): p.get("checksum") for p in lock["package"]}
    overrides_path = root / "packaging/licenses/index.json"
    overrides = json.loads(overrides_path.read_text())
    copied: dict[str, bytes] = {}
    inventory = []
    missing = []
    for package in dependency_packages(meta):
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
            path = (root / "packaging/licenses" / entry["file"]).resolve()
            if not path.is_relative_to((root / "packaging/licenses").resolve()):
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
    copied["licenses/Silkscreen-OFL.txt"] = (root / "assets/fonts/OFL.txt").read_bytes()
    copied["licenses/Silkscreen-metadata.txt"] = font_metadata(root / "assets/fonts/Silkscreen-Bold.ttf")
    copied["licenses/Saira-OFL.txt"] = (root / "docs/site/assets/fonts/saira-OFL.txt").read_bytes()
    runtime_files, runtime_inventory = rust_notices()
    copied.update(runtime_files)
    copied["licenses/INVENTORY.json"] = json_bytes({"scope": "Resolved Linux target closure, including development and build-time crates; not a claim that every listed crate is linked into the executable.",
                                                   "target": TARGET, "packages": inventory, "rust_runtime": runtime_inventory,
                                                   "application_font": {"file": "assets/fonts/Silkscreen-Bold.ttf", "notice": "licenses/Silkscreen-OFL.txt"},
                                                   "documentation_fonts": [{"name": "Saira", "file": "docs/site/assets/fonts/saira.woff2",
                                                                            "sha256": digest((root / "docs/site/assets/fonts/saira.woff2").read_bytes()),
                                                                            "license_expression": "OFL-1.1", "notice": "licenses/Saira-OFL.txt",
                                                                            "source": "https://github.com/google/fonts/tree/main/ofl/saira"}]})
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    lines = [f"LedAlert {version} — third-party notices", "",
             "No application license is granted by this notice. Third-party components retain their own terms.",
             "The licenses/ directory contains verbatim license and attribution inputs; INVENTORY.json maps them to versions and digests.",
             "This conservative inventory includes build-time and development dependencies. It is not a legal certification.", ""]
    lines.extend(f"{p['name']} {p['version']}: {p['license_expression']}" for p in inventory)
    lines.extend(["", "Bundled Silkscreen: SIL Open Font License 1.1; see licenses/Silkscreen-OFL.txt.",
                  "Documentation Saira: SIL Open Font License 1.1; see licenses/Saira-OFL.txt.",
                  "Embedded egui fonts include Hack, Noto Emoji, Ubuntu and emoji-icon-font; see the font notices in the epaint_default_fonts entry.",
                  "Additional upstream/Rust runtime notices and their provenance are recorded in licenses/INVENTORY.json.", ""])
    copied["THIRD-PARTY-NOTICES.txt"] = "\n".join(lines).encode()
    return copied, inventory


def archive(path: Path, prefix: str, entries: dict[str, tuple[bytes, int]], epoch: int) -> None:
    with path.open("xb") as destination:
        with gzip.GzipFile(filename="", mode="wb", fileobj=destination, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
                for relative, (content, mode) in sorted(entries.items()):
                    member = tarfile.TarInfo(f"{prefix}/{relative}")
                    member.size, member.mode, member.mtime = len(content), mode, epoch
                    member.uid = member.gid = 0
                    member.uname = member.gname = ""
                    tar.addfile(member, io.BytesIO(content))


def release_names(version: str, crate: bool = False) -> list[str]:
    names = [f"ledalert-{version}-linux-x86_64.tar.gz", f"ledalert-{version}-source.tar.gz", "SHA256SUMS"]
    if crate:
        names.append(f"ledalert-{version}.crate")
    return names


def prepare_output(output: Path, names: list[str]) -> None:
    output.mkdir(parents=True, exist_ok=True)
    if any((output / name).exists() or (output / name).is_symlink() for name in names):
        raise ValueError("Release filenames already exist; choose another empty --output directory rather than overwrite a release.")


def publish(staging: Path, output: Path, names: list[str]) -> None:
    # Staging is on the destination filesystem. Hard links fail rather than
    # replacing an existing file, including one created after the initial check.
    for name in names:
        os.link(staging / name, output / name)


def container_engine() -> tuple[str, bool]:
    podman = shutil.which("podman")
    if podman:
        result = subprocess.run([podman, "info", "--format", "{{.Host.Security.Rootless}}"],
                                capture_output=True, text=True)
        if result.returncode == 0 and result.stdout.strip() == "true":
            return podman, True
    docker = shutil.which("docker")
    if docker:
        result = subprocess.run([docker, "info"], capture_output=True, text=True)
        if result.returncode == 0:
            return docker, False
    raise ValueError("A working rootless Podman installation (preferred) or Docker daemon is required; no host-native fallback is allowed.")


def container_build(output: Path, names: list[str], epoch: int, verify: bool, commit: str | None = None) -> None:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("The controlled builder requires a Linux x86_64 host; emulated or cross builds are not supported")
    engine, podman = container_engine()
    with tempfile.TemporaryDirectory(prefix=".ledalert-build-", dir=output) as temporary:
        staging = Path(temporary)
        source, context, artifacts = (staging / name for name in ("source", "context", "artifacts"))
        for directory in (source, context, artifacts):
            directory.mkdir()
        # Only the selected public source enters the container. In particular,
        # no checkout, HOME, host Cargo cache/config or credentials are mounted.
        hashes = {}
        for name, path in source_files().items():
            content = path.read_bytes()
            hashes[name] = digest(content)
            destination = source / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
            destination.chmod(0o644)
        if hashes != {name: digest(path.read_bytes()) for name, path in source_files().items()}:
            raise ValueError("Source inputs changed during staging; rerun from a stable tree")
        recipe = (source / "packaging/Containerfile").read_bytes()
        if not recipe.decode().startswith(f"FROM {BASE_IMAGE}\n"):
            raise ValueError("Containerfile does not use the selected digest-pinned baseline")
        (context / "Containerfile").write_bytes(recipe)
        image_id = staging / "image-id"
        # The build context contains the recipe only, not even the source.
        subprocess.run([engine, "build", "--platform", "linux/amd64", "--file", str(context / "Containerfile"),
                        "--iidfile", str(image_id), str(context)], check=True)
        image = image_id.read_text().strip()
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
            raise ValueError("Container engine did not return a valid built image ID")
        if ":" in str(staging):
            raise ValueError("The output path cannot contain ':' because it is used in container bind mounts")
        run = [engine, "run", "--rm", "--platform", "linux/amd64", "--cap-drop=ALL",
               "--security-opt=no-new-privileges", "--user", f"{os.getuid()}:{os.getgid()}"]
        if podman:
            run.append("--userns=keep-id")
        run.extend(["--volume", f"{source}:/build:Z", "--volume", f"{artifacts}:/output:Z",
                    "--workdir", "/build", "--env", "HOME=/build/.home",
                    "--env", "CARGO_HOME=/build/.cargo-home", "--env", "CARGO_TARGET_DIR=/build/target",
                    "--env", f"SOURCE_DATE_EPOCH={epoch}", image,
                    "python3", "packaging/release.py", "--native", "--output", "/output"])
        if verify:
            run.append("--verify")
        if len(names) == 4:
            run.append("--crate")
            if commit is not None:
                run.extend(["--commit", commit])
        subprocess.run(run, check=True)
        for name in names:
            path = artifacts / name
            if not path.is_file() or path.is_symlink():
                raise ValueError(f"Builder did not produce a regular artifact: {name}")
        publish(artifacts, output, names)
        if len(names) == 4:
            (output / ".builder-image").write_text(image + "\n")
    print(json.dumps({"artifacts": [str(output / name) for name in names],
                      "verification": "passed" if verify else "not_requested",
                      "baseline": BASELINE}, indent=2))


def native_environment() -> tuple[str, str, dict]:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError(f"The native stage requires Linux {TARGET}")
    recipe = (ROOT / "packaging/Containerfile").read_bytes()
    if not BUILDER_RECIPE.is_file() or BUILDER_RECIPE.read_bytes() != recipe:
        raise ValueError("The internal --native stage requires the selected packaging/Containerfile builder")
    if not recipe.decode().startswith(f"FROM {BASE_IMAGE}\n"):
        raise ValueError("The native stage's Containerfile does not match the selected baseline image")
    system = platform.freedesktop_os_release()
    if any(system.get(key) != value for key, value in {
            "ID": "debian", "VERSION_ID": "12", "VERSION_CODENAME": "bookworm"}.items()):
        raise ValueError("The native stage requires Debian 12 (bookworm)")
    glibc = command("getconf", "GNU_LIBC_VERSION")
    if glibc != f"glibc {GLIBC_MAX}":
        raise ValueError(f"The native stage requires the Debian 12 glibc {GLIBC_MAX} baseline, found {glibc}")
    rustc, cargo = command("rustc", "-Vv"), command("cargo", "-V")
    if f"host: {TARGET}" not in rustc.splitlines() or f"release: {RUST_TOOLCHAIN}" not in rustc.splitlines():
        raise ValueError(f"The native stage requires Rust {RUST_TOOLCHAIN} for {TARGET}")
    if not cargo.startswith(f"cargo {RUST_TOOLCHAIN} "):
        raise ValueError(f"The native stage requires Cargo {RUST_TOOLCHAIN}")
    return rustc, cargo, {"os_release": {key: system[key] for key in ("ID", "VERSION_ID", "VERSION_CODENAME")},
                          "glibc": GLIBC_MAX, "containerfile_sha256": digest(recipe)}


def native_build(output: Path, names: list[str], epoch: int, verify: bool, commit: str | None = None) -> None:
    rustc, cargo, builder = native_environment()
    package = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
    version = package["version"]
    sources = source_files()
    source_hashes = {name: digest(path.read_bytes()) for name, path in sources.items()}
    Path.home().mkdir(parents=True, exist_ok=True)
    os.environ["CARGO_TARGET_DIR"] = str(ROOT / "target")
    os.environ.pop("RUSTFLAGS", None)
    os.environ["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
        "-C", "target-cpu=x86-64",
        f"--remap-path-prefix={Path.home()}=/usr/src/build",
        f"--remap-path-prefix={ROOT}=/usr/src/ledalert",
    ])
    if verify:
        from verify import check_artifacts, check_source

        check_source(ROOT)
    notices, dependencies = notice_inputs(metadata())
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", TARGET, "--bin", "ledalert"],
                   cwd=ROOT, check=True)
    crate = None
    if len(names) == 4:
        from verify import check_crate

        crate = check_crate(ROOT)
    binary = ROOT / "target" / TARGET / "release/ledalert"
    versions = command("readelf", "--version-info", str(binary))
    glibc = sorted(set(re.findall(r"GLIBC_(\d+(?:\.\d+)+)", versions)),
                   key=lambda value: tuple(map(int, value.split("."))))
    if not glibc or "GLIBC_PRIVATE" in versions:
        raise ValueError("The executable must have measurable public glibc ABI requirements")
    if tuple(map(int, glibc[-1].split("."))) > tuple(map(int, GLIBC_MAX.split("."))):
        raise ValueError(f"Executable requires glibc {glibc[-1]}, exceeding the {GLIBC_MAX} baseline")
    if command(str(binary), "--version") != f"LedAlert {version}":
        raise ValueError("Built executable version does not match Cargo.toml")
    needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", command("readelf", "-d", str(binary)))
    executable = binary.read_bytes()
    legal_files = [name for name in sources
                   if ("/" not in name and FIRST_PARTY_NOTICE.match(name)) or name == package.get("license-file")]
    legal = {"status": "declared" if legal_files and (package.get("license") or package.get("license-file")) else "not_selected",
             "expression": package.get("license"), "files": legal_files}
    info = {"application": "LedAlert", "version": version, "target": TARGET, "target_cpu": "x86-64",
            "rustc": rustc, "cargo": cargo, "glibc_required": glibc[-1],
            "baseline": BASELINE, "builder": builder, "first_party_license": legal,
            "direct_shared_libraries": needed, "binary_sha256": digest(executable), "source_files": source_hashes,
            "source_manifest_sha256": digest(json_bytes(source_hashes)), "cargo_lock_sha256": source_hashes["Cargo.lock"],
            "dependency_packages": len(dependencies), "archive_epoch": epoch,
            "provenance": "Source-only working-tree snapshot in the recorded baseline builder; no commit, tag, signature, publication or reproducible-build attestation is implied."}
    if crate is not None:
        info.update(source_commit=commit, crate_sha256=digest(crate.read_bytes()))
        if commit is not None:
            info["provenance"] = "Public source snapshot bound to the recorded workflow commit and Cargo checksum; not a signature or reproducible-build attestation."
    if source_hashes != {name: digest(path.read_bytes()) for name, path in source_files().items()}:
        raise ValueError("Source inputs changed during build; rerun from a stable tree")
    common = {name: (content, 0o644) for name, content in notices.items()}
    common["BUILD-INFO.json"] = (json_bytes(info), 0o644)
    source_entries = dict(common)
    source_entries.update({name: (path.read_bytes(), 0o644) for name, path in sources.items()})
    binary_entries = dict(common)
    binary_entries["bin/ledalert"] = (executable, 0o755)
    for name, path in sources.items():
        if name in {"README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md"} or name.startswith(("docs/", "assets/")) or name in legal_files or name in {
                "packaging/io.github.critx.LedAlert.desktop", "packaging/io.github.critx.LedAlert.svg"}:
            binary_entries[name] = (path.read_bytes(), 0o644)
    with tempfile.TemporaryDirectory(prefix=".ledalert-package-", dir=output) as temporary:
        staging = Path(temporary)
        archive(staging / names[0], f"ledalert-{version}-linux-x86_64", binary_entries, epoch)
        archive(staging / names[1], f"ledalert-{version}-source", source_entries, epoch)
        if crate is not None:
            shutil.copyfile(crate, staging / names[3])
        (staging / names[2]).write_text("".join(
            f"{digest((staging / name).read_bytes())}  {name}\n" for name in names if name != "SHA256SUMS"))
        if verify:
            check_artifacts(ROOT, staging)
        publish(staging, output, names)
    print(json.dumps({"artifacts": [str(output / name) for name in names], "glibc_required": info["glibc_required"],
                      "source_files": len(sources), "dependency_packages": len(dependencies),
                      "first_party_license": legal}, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist", help="Destination without existing release filenames (default: dist/)")
    parser.add_argument("--verify", action="store_true", help="Run source gates and final artifact verification inside the baseline container")
    parser.add_argument("--crate", action="store_true", help="Also verify a publishable Cargo package; requires --verify")
    parser.add_argument("--commit", help="Exact Git commit for automated distribution; omit for an uncommitted local snapshot")
    parser.add_argument("--native", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
        if args.crate:
            from verify import require_distribution

            require_distribution(ROOT)
            if not args.verify:
                raise ValueError("--crate requires --verify")
            if args.commit is not None and not re.fullmatch(r"[0-9a-f]{40}", args.commit):
                raise ValueError("--commit must be a full lowercase SHA when supplied")
        elif args.commit:
            raise ValueError("--commit is only valid with --crate")
        names = release_names(version, args.crate)
        output = args.output.resolve()
        prepare_output(output, names)
        epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
        if epoch < 0:
            raise ValueError("SOURCE_DATE_EPOCH must be nonnegative")
        if args.native:
            native_build(output, names, epoch, args.verify, args.commit)
        else:
            container_build(output, names, epoch, args.verify, args.commit)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release.py: {error}\n")


if __name__ == "__main__":
    main()
