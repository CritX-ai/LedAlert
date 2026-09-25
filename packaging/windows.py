#!/usr/bin/env python3
"""Build Windows 11 x64 portable/MSIX packages; never install, trust, or publish."""
from __future__ import annotations

import argparse
import binascii
import os
from pathlib import Path
import platform
import re
import stat
import struct
import subprocess
import sys
import tempfile
import tomllib
import xml.etree.ElementTree as ET
import zipfile
import zlib

import release
import verify

TARGET = "x86_64-pc-windows-msvc"
MIN_WINDOWS = "10.0.22000.0"
IDENTITY = "CritX.LedAlert"
NS = {
    "": "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
    "uap": "http://schemas.microsoft.com/appx/manifest/uap/windows10",
    "uap3": "http://schemas.microsoft.com/appx/manifest/uap/windows10/3",
    "rescap": "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities",
}
for prefix, uri in NS.items():
    ET.register_namespace(prefix, uri)
LOGOS = {"assets/windows/StoreLogo.png": 50, "assets/windows/Square44x44Logo.png": 44,
         "assets/windows/Square150x150Logo.png": 150}
PACKAGE_FILES = {"AppxBlockMap.xml", "[Content_Types].xml", "AppxSignature.p7x", "AppxMetadata/CodeIntegrity.cat"}
PORTABLE_NOTICE = (
    "LedAlert Windows 11 x64 portable package\n\n"
    "Run ledalert.exe. Portable execution has no package identity and cannot read Windows notifications.\n"
    "Notification access requires registered MSIX identity and explicit permission in LedAlert.\n"
    "Unsigned development MSIX files are not trusted or normally installable. No certificate is generated or installed.\n"
    "For local development only, enable Windows Developer Mode, extract this ZIP to a permanent directory,\n"
    "then run Add-AppxPackage -Register <full-path-to-AppxManifest.xml> and launch LedAlert from Start.\n"
    "Registration is per-user; remove with Get-AppxPackage CritX.LedAlert | Remove-AppxPackage.\n"
    "For normal installation use a properly signed MSIX whose Publisher matches its trusted signing certificate.\n"
    "See RELEASE.md for signing prerequisites and package limitations.\n"
).encode()


def names(version: str) -> list[str]:
    package_version(version)
    return [f"ledalert-{version}-windows-x86_64.zip", f"ledalert-{version}-windows-x86_64.msix", "WINDOWS-SHA256SUMS"]


def package_version(version: str) -> str:
    return release.version_info(version)[0]


def manifest(version: str, publisher: str, root: Path = release.ROOT) -> bytes:
    verify.require(bool(publisher) and publisher.startswith("CN=") and not any(ord(c) < 32 for c in publisher),
                   "Publisher must be a certificate subject beginning CN=")
    tree = ET.fromstring((root / "packaging/windows/AppxManifest.xml").read_bytes())
    identity = tree.find("Identity", NS)
    verify.require(identity is not None, "Manifest lacks Identity")
    identity.set("Version", package_version(version))
    identity.set("Publisher", publisher)
    data = ET.tostring(tree, encoding="utf-8", xml_declaration=True)
    validate_manifest(data, version, publisher)
    return data


def validate_manifest(data: bytes, version: str, publisher: str) -> None:
    verify.require(len(data) <= 64 * 1024 and b"<!DOCTYPE" not in data.upper() and b"<!ENTITY" not in data.upper(),
                   "Unsafe package manifest")
    tree = ET.fromstring(data)
    identity = tree.findall("Identity", NS)
    verify.require(len(identity) == 1 and identity[0].attrib == {
        "Name": IDENTITY, "Publisher": publisher, "Version": package_version(version), "ProcessorArchitecture": "x64"},
        "MSIX identity, publisher, version or architecture mismatch")
    families = tree.findall("Dependencies/TargetDeviceFamily", NS)
    verify.require(len(families) == 1 and families[0].get("Name") == "Windows.Desktop" and
                   families[0].get("MinVersion") == MIN_WINDOWS, "MSIX must target Windows 11 desktop")
    applications = tree.findall("Applications/Application", NS)
    verify.require(len(applications) == 1 and applications[0].attrib == {
        "Id": "LedAlert", "Executable": "ledalert.exe", "EntryPoint": "Windows.FullTrustApplication"},
        "Unexpected MSIX activation contract")
    capabilities = tree.find("Capabilities", NS)
    expected = {(f"{{{NS['uap3']}}}Capability", "userNotificationListener"),
                (f"{{{NS['rescap']}}}Capability", "runFullTrust"),
                (f"{{{NS['rescap']}}}Capability", "globalMediaControl")}
    verify.require(capabilities is not None and len(capabilities) == len(expected) and
                   {(node.tag, node.get("Name")) for node in capabilities} == expected,
                   "Missing or unexpected package capabilities")
    visuals = applications[0].find("uap:VisualElements", NS)
    verify.require(visuals is not None and visuals.get("Square150x150Logo") == "assets\\windows\\Square150x150Logo.png" and
                   visuals.get("Square44x44Logo") == "assets\\windows\\Square44x44Logo.png" and
                   tree.findtext("Properties/Logo", namespaces=NS) == "assets\\windows\\StoreLogo.png", "Invalid package logos")


def logo(size: int) -> bytes:
    """Generate a deterministic opaque LED tile with no image-library dependency."""
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            lit = (x - size // 2) ** 2 + (y - size // 2) ** 2 <= (size // 3) ** 2
            rows.extend((71, 224, 170) if lit else (19, 26, 35))
    def chunk(kind: bytes, value: bytes) -> bytes:
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", binascii.crc32(kind + value))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b""))


def safe_zip_name(name: str) -> bool:
    reserved = {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$",
                *(f"{prefix}{digit}" for prefix in ("COM", "LPT") for digit in "123456789¹²³")}
    return verify.safe_name(name) and all(
        not any(ord(c) < 32 or c in ':<>"|?*' for c in part) and not part.endswith((".", " ")) and
        part.split(".")[0].rstrip(" ").upper() not in reserved for part in name.split("/"))


def read_zip(path: Path) -> dict[str, bytes]:
    verify.require(path.is_file() and not path.is_symlink() and path.stat().st_size <= verify.MAX_ARCHIVE_BYTES,
                   "Missing, unsafe or oversized Windows archive")
    result = {}
    folded = set()
    total = 0
    with zipfile.ZipFile(path) as archive:
        members = archive.infolist()
        verify.require(len(members) <= verify.MAX_MEMBERS, "Too many ZIP members")
        for member in members:
            name = member.filename
            mode = member.external_attr >> 16
            verify.require(member.orig_filename == name and safe_zip_name(name) and name.casefold() not in folded and not member.is_dir() and
                           stat.S_IFMT(mode) in {0, stat.S_IFREG} and not member.flag_bits & 1,
                           f"Unsafe, duplicate or unsupported ZIP member: {name}")
            total += member.file_size
            verify.require(member.file_size <= verify.MAX_MEMBER_BYTES and total <= verify.MAX_ARCHIVE_BYTES,
                           "Oversized ZIP contents")
            folded.add(name.casefold())
            result[name] = archive.read(member)
    return result


def write_zip(path: Path, entries: dict[str, bytes]) -> None:
    with path.open("xb") as destination, zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in sorted(entries.items()):
            verify.require(safe_zip_name(name), f"Unsafe ZIP destination: {name}")
            member = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            member.create_system = 3
            member.external_attr = (stat.S_IFREG | 0o644) << 16
            member.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(member, data)


def validate_pe(data: bytes) -> None:
    verify.require(len(data) >= 64 and data[:2] == b"MZ", "Expected Windows PE executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    verify.require(offset >= 64 and offset + 26 <= len(data) and data[offset:offset + 4] == b"PE\0\0" and
                   struct.unpack_from("<H", data, offset + 4)[0] == 0x8664 and
                   struct.unpack_from("<H", data, offset + 24)[0] == 0x20B, "Expected x64 PE32+ executable")


def inspect_artifacts(root: Path, output: Path, *, commit: str | None = None, require_signed: bool = False) -> dict:
    version = tomllib.loads((root / "Cargo.toml").read_text())['package']['version']
    artifacts = names(version)
    sum_path = output / artifacts[2]
    verify.require(sum_path.is_file() and not sum_path.is_symlink() and sum_path.stat().st_size <= 4096,
                   "Missing or unsafe Windows checksums")
    expected = "".join(f"{verify.file_digest(output / name)}  {name}\n" for name in artifacts[:2])
    verify.require(sum_path.read_text() == expected, "Windows checksums differ from artifacts")
    portable, msix = (read_zip(output / name) for name in artifacts[:2])
    required = {"ledalert.exe", "AppxManifest.xml", "BUILD-INFO.json", "PORTABLE.txt", "THIRD-PARTY-NOTICES.txt", "licenses/INVENTORY.json", *LOGOS}
    verify.require(required.issubset(portable), "Missing Windows package members")
    info = verify.read_json(portable["BUILD-INFO.json"], "Windows BUILD-INFO.json")
    verify.require(info.get("application") == "LedAlert" and info.get("version") == version and
                   info.get("target") == TARGET and info.get("minimum_windows") == MIN_WINDOWS and
                   info.get("rust_toolchain") == release.RUST_TOOLCHAIN, "Windows build identity mismatch")
    verify.require(info.get("source_commit") == commit, "Windows package source commit mismatch")
    publisher = info.get("publisher")
    verify.require(isinstance(publisher, str) and publisher.startswith("CN="), "Missing Windows publisher")
    validate_manifest(portable["AppxManifest.xml"], version, publisher)
    verify.require(portable["AppxManifest.xml"] == manifest(version, publisher, root), "Manifest differs from source contract")
    validate_pe(portable["ledalert.exe"])
    verify.require(release.digest(portable["ledalert.exe"]) == info.get("binary_sha256"), "Windows executable hash mismatch")
    source_hashes = {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()}
    verify.require(info.get("source_files") == source_hashes, "Windows package sources differ from checkout")
    payload = info.get("payload_sha256")
    verify.require(isinstance(payload, dict) and set(payload) == set(portable) - {"BUILD-INFO.json"} and
                   all(release.digest(portable[name]) == checksum for name, checksum in payload.items()), "Windows payload inventory mismatch")
    for name, size in LOGOS.items():
        verify.require(portable[name] == logo(size), f"Package logo mismatch: {name}")
    verify.require(portable["PORTABLE.txt"] == PORTABLE_NOTICE, "Missing portable notification limitations")
    inventory = verify.read_json(portable["licenses/INVENTORY.json"], "Windows notice inventory")
    verify.require(inventory.get("target") == TARGET and bool(inventory.get("packages")), "Windows notice target mismatch")
    locked = {(p["name"], p["version"]): p for p in tomllib.loads((root / "Cargo.lock").read_text())["package"]}
    referenced = {"licenses/INVENTORY.json", "licenses/Silkscreen-OFL.txt", "licenses/Silkscreen-metadata.txt", "licenses/Saira-OFL.txt"}
    seen = set()
    for package in inventory["packages"]:
        key = (package["name"], package["version"])
        verify.require(key not in seen and key in locked and package.get("crate_sha256") == locked[key].get("checksum") and
                       package.get("registry_source") == locked[key].get("source") and
                       bool(package.get("license_expression")) and bool(package.get("notices")), "Invalid Windows dependency notice provenance")
        seen.add(key)
    notices = [notice for package in inventory["packages"] for notice in package["notices"]]
    runtime = inventory.get("rust_runtime", {})
    verify.require(bool(runtime.get("notices")), "Missing Windows Rust runtime notices")
    for notice in notices + runtime["notices"]:
        name = notice["path"]
        verify.require(name.startswith("licenses/") and name in portable and bool(portable[name].strip()) and
                       release.digest(portable[name]) == notice.get("sha256"), f"Missing or changed Windows notice: {name}")
        referenced.add(name)
    legal = verify.first_party_license(root)
    docs = {name for name in release.source_files(root)
            if name.startswith(("docs/", "assets/")) or name in
            {"README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md", *legal.get("files", [])}}
    verify.require(info.get("first_party_license") == legal and docs.issubset(portable) and
                   all(portable[name] == (root / name).read_bytes() for name in docs), "Windows documentation or first-party license mismatch")
    verify.require(set(portable) == required | referenced | docs and referenced.issubset(portable),
                   "Unexpected or missing Windows payload files")
    verify.require(set(msix) - set(portable) <= PACKAGE_FILES and set(portable).issubset(msix) and
                   all(msix[name] == data for name, data in portable.items()) and
                   {"AppxBlockMap.xml", "[Content_Types].xml"}.issubset(msix), "MSIX and portable payloads differ")
    signed = bool(msix.get("AppxSignature.p7x"))
    verify.require(info.get("signing") in {"unsigned", "certificate-store"} and
                   signed == (info["signing"] == "certificate-store"), "MSIX signing envelope mismatch")
    verify.require(not require_signed or signed, "Stable public releases require signed MSIX; unsigned packages are development-only")
    # Signature presence is not trust. Native build additionally runs signtool verify /pa.
    return info


def sdk_tool(name: str, sdk_bin: Path | None = None) -> str:
    if sdk_bin:
        candidates = [sdk_bin / name]
    else:
        kits = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Windows Kits/10/bin"
        versions = sorted((p for p in kits.glob("10.*") if re.fullmatch(r"10\.\d+\.\d+\.\d+", p.name)),
                          key=lambda p: tuple(map(int, p.name.split("."))), reverse=True)
        candidates = [p / "x64" / name for p in versions]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    raise ValueError(f"Windows SDK {name} is missing; install Windows SDK or supply --sdk-bin (x64 tools directory)")


def smoke(binary: Path, version: str) -> None:
    verify.require(release.command(str(binary), "--version") == f"LedAlert {version}", "Windows executable version mismatch")
    with tempfile.TemporaryDirectory(prefix="ledalert-windows-smoke-") as temporary:
        config = Path(temporary) / "config.json"
        data = release.json_bytes(verify.synthetic_config())
        config.write_bytes(data)
        subprocess.run([str(binary), "--config", str(config), "check-config"], check=True, timeout=30)
        verify.require(config.read_bytes() == data, "Windows check-config changed the saved setup")
        missing = Path(temporary) / "missing.json"
        result = subprocess.run([str(binary), "--config", str(missing), "check-config"], capture_output=True, timeout=30)
        verify.require(result.returncode != 0 and not missing.exists(), "Windows missing-config validation changed state")


def build(args: argparse.Namespace) -> None:
    root = release.ROOT
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    artifacts = names(version)
    output = args.output.resolve()
    release.prepare_output(output, artifacts)
    verify.require(platform.system() == "Windows" and platform.machine().lower() in {"amd64", "x86_64"},
                   "Windows packaging requires native Windows x64 with MSVC build tools")
    verify.require(args.commit is None or re.fullmatch(r"[0-9a-f]{40}", args.commit), "Expected full lowercase commit SHA")
    makeappx, signtool = package_tools(args)
    os.environ["RUSTUP_TOOLCHAIN"] = release.RUST_TOOLCHAIN
    os.environ["CARGO_TARGET_DIR"] = str(root / "target")
    os.environ.pop("RUSTFLAGS", None)
    os.environ["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
        "-C", "target-cpu=x86-64", "-C", "target-feature=+crt-static",
        f"--remap-path-prefix={Path.home()}=/usr/src/build", f"--remap-path-prefix={root}=/usr/src/ledalert"])
    verify.require(release.command("rustc", "-V").startswith("rustc " + release.RUST_TOOLCHAIN + " "), "Unexpected Windows Rust toolchain")
    if args.verify:
        for command in (["cargo", "test", "--locked", "--target", TARGET],
                        [sys.executable, "-B", "-m", "unittest", "discover", "-s", "packaging", "-p", "test_*.py"]):
            subprocess.run(command, cwd=root, check=True)
    sources = {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()}
    notices, _ = release.notice_inputs(release.metadata(root, TARGET), root, TARGET)
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", TARGET, "--bin", "ledalert"], cwd=root, check=True)
    binary = root / "target" / TARGET / "release/ledalert.exe"
    smoke(binary, version)
    executable = binary.read_bytes()
    validate_pe(executable)
    entries = {**notices, "ledalert.exe": executable, "AppxManifest.xml": manifest(version, args.publisher),
               "PORTABLE.txt": PORTABLE_NOTICE, **{name: logo(size) for name, size in LOGOS.items()}}
    legal = verify.first_party_license(root)
    for name, path in release.source_files(root).items():
        if name.startswith(("docs/", "assets/")) or name in {"README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md", *legal.get("files", [])}:
            entries[name] = path.read_bytes()
    info = {"application": "LedAlert", "version": version, "target": TARGET, "minimum_windows": MIN_WINDOWS,
            "rust_toolchain": release.RUST_TOOLCHAIN, "binary_sha256": release.digest(executable),
            "source_commit": args.commit, "source_files": sources, "publisher": args.publisher,
            "signing": "certificate-store" if signtool else "unsigned", "first_party_license": legal,
            "payload_sha256": {name: release.digest(data) for name, data in sorted(entries.items())}}
    entries["BUILD-INFO.json"] = release.json_bytes(info)
    verify.require(sources == {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()},
                   "Windows source inputs changed during build")
    package_entries(args, entries, info, makeappx, signtool)


def package_tools(args: argparse.Namespace) -> tuple[str, str | None]:
    verify.require(not args.require_signed or args.certificate_thumbprint, "--require-signed requires --certificate-thumbprint")
    if args.certificate_thumbprint:
        verify.require(re.fullmatch(r"[0-9a-fA-F]{40}", args.certificate_thumbprint) is not None,
                       "Signing certificate thumbprint must contain 40 hexadecimal digits")
        verify.require(args.timestamp_url and args.timestamp_url.startswith("https://"), "Signing requires an HTTPS RFC3161 timestamp URL")
    return sdk_tool("makeappx.exe", args.sdk_bin), sdk_tool("signtool.exe", args.sdk_bin) if args.certificate_thumbprint else None


def package_entries(args: argparse.Namespace, entries: dict[str, bytes], info: dict, makeappx: str, signtool: str | None) -> None:
    output = args.output.resolve()
    artifacts = names(info["version"])
    with tempfile.TemporaryDirectory(prefix=".ledalert-windows-", dir=output) as temporary:
        staging = Path(temporary)
        layout = staging / "layout"
        for name, data in entries.items():
            destination = layout / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        write_zip(staging / artifacts[0], entries)
        msix = staging / artifacts[1]
        subprocess.run([makeappx, "pack", "/d", str(layout), "/p", str(msix), "/h", "SHA256"], check=True)
        if signtool:
            subprocess.run([signtool, "sign", "/fd", "SHA256", "/s", "My", "/sha1", args.certificate_thumbprint,
                            "/tr", args.timestamp_url, "/td", "SHA256", str(msix)], check=True)
            subprocess.run([signtool, "verify", "/pa", "/all", str(msix)], check=True)
        (staging / artifacts[2]).write_text("".join(f"{verify.file_digest(staging / name)}  {name}\n" for name in artifacts[:2]))
        inspect_artifacts(release.ROOT, staging, commit=args.commit, require_signed=args.require_signed)
        release.publish(staging, output, artifacts)
    verify.report("windows_packages", status="passed", artifacts=artifacts, signing=info["signing"],
                  scope="Package inventory; CLI smoke runs during builds only. No installation, notification permission or GUI acceptance implied")


def sign_existing(args: argparse.Namespace) -> None:
    """Repackage verified bytes without executing Cargo, tests, or the executable with signing credentials."""
    verify.require(platform.system() == "Windows", "MSIX signing requires native Windows")
    verify.require(bool(args.certificate_thumbprint), "--sign-existing requires --certificate-thumbprint")
    source = args.sign_existing.resolve()
    info = inspect_artifacts(release.ROOT, source, commit=args.commit)
    verify.require(info["signing"] == "unsigned", "Refusing to re-sign an existing signed release")
    artifacts = names(info["version"])
    release.prepare_output(args.output.resolve(), artifacts)
    makeappx, signtool = package_tools(args)
    entries = read_zip(source / artifacts[0])
    del entries["BUILD-INFO.json"]
    entries["AppxManifest.xml"] = manifest(info["version"], args.publisher)
    info.update(publisher=args.publisher, signing="certificate-store",
                payload_sha256={name: release.digest(data) for name, data in sorted(entries.items())})
    entries["BUILD-INFO.json"] = release.json_bytes(info)
    package_entries(args, entries, info, makeappx, signtool)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=release.ROOT / "dist/windows")
    parser.add_argument("--verify", action="store_true", help="Run native Rust and portable Python regression tests before building")
    parser.add_argument("--inspect", action="store_true", help="Only inspect existing Windows artifacts (also works on Linux)")
    parser.add_argument("--sign-existing", type=Path, help="Sign verified unsigned packages without rebuilding or executing payloads")
    parser.add_argument("--publisher", default="CN=LedAlert", help="Exact signing certificate subject; local unsigned default: CN=LedAlert")
    parser.add_argument("--certificate-thumbprint", help="Existing code-signing certificate in CurrentUser\\My; never imports trust")
    parser.add_argument("--timestamp-url", help="HTTPS RFC3161 timestamp service (required for signing)")
    parser.add_argument("--require-signed", action="store_true", help="Reject local unsigned packages for public distribution")
    parser.add_argument("--sdk-bin", type=Path, help="Windows SDK x64 tools directory")
    parser.add_argument("--commit", help="Exact source commit for CI provenance")
    args = parser.parse_args()
    try:
        verify.require(not (args.inspect and args.sign_existing), "--inspect and --sign-existing are mutually exclusive")
        if args.inspect:
            inspect_artifacts(release.ROOT, args.output.resolve(), commit=args.commit, require_signed=args.require_signed)
            verify.report("windows_inventory", status="passed", signature_trust="not checked; use signtool verify /pa on Windows")
        elif args.sign_existing:
            sign_existing(args)
        else:
            build(args)
    except (ValueError, OSError, KeyError, TypeError, ET.ParseError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        parser.exit(1, f"windows.py: {error}\n")


if __name__ == "__main__":
    main()
