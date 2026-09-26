#!/usr/bin/env python3
"""Build/inspect an ad-hoc Apple Silicon .app archive; never install, grant access or publish."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import platform
import plistlib
import re
import struct
import subprocess
import sys
import tempfile
import tomllib
import unicodedata

import release
import verify

TARGET = "aarch64-apple-darwin"
MIN_MACOS = "14.2"
BUNDLE_ID = "io.github.critx.LedAlert"
APP = "LedAlert.app"
BINARY = APP + "/Contents/MacOS/ledalert"
RESOURCES = APP + "/Contents/Resources/"
PLIST = APP + "/Contents/Info.plist"
SIGNATURE = APP + "/Contents/_CodeSignature/CodeResources"
INSTALL = (
    "LedAlert for Apple Silicon macOS\n\n"
    "Copy LedAlert.app to a location you control and open it. No installer or administrator access is needed.\n"
    "This package is locally ad-hoc signed, UNNOTARIZED, and has NO Developer ID identity or Apple trust claim.\n"
    "Gatekeeper may block a downloaded copy. Review the source/checksums and use macOS's explicit per-app\n"
    "Open Anyway flow only if you choose to trust it; never disable Gatekeeper or strip quarantine automatically.\n"
    "Lighting starts off. Notification identities optionally require Full Disk Access, a broad permission.\n"
    "Grant it only to LedAlert in System Settings if wanted, then relaunch; the app never grants access itself.\n"
    "Notification database and Sidebar preference schemas are private and may change. Failures pause the affected\n"
    "integration. Sidebar pins are read only while Sidebar runs; otherwise conventional Dock pins are used.\n"
    "Audio activity includes calls and silent output streams; it is not a media-title or video-playback feed.\n"
    "The current validation scope is macOS 27 on Apple Silicon. The 14.2 loader minimum is NOT older-OS acceptance.\n"
    "See the bundled README.md and RELEASE.md under Contents/Resources for verification and limitations.\n"
).encode()


def names(version: str) -> list[str]:
    release.version_info(version)
    return [f"ledalert-{version}-macos-aarch64.tar.gz", "MACOS-SHA256SUMS"]


def info_plist(version: str) -> bytes:
    release.version_info(version)
    base = version.split("-", 1)[0]
    return plistlib.dumps({
        "CFBundleDevelopmentRegion": "en", "CFBundleDisplayName": "LedAlert",
        "CFBundleExecutable": "ledalert", "CFBundleIconFile": "LedAlert.icns",
        "CFBundleIdentifier": BUNDLE_ID, "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "LedAlert", "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": base, "CFBundleVersion": base,
        "LedAlertSourceVersion": version, "LSMinimumSystemVersion": MIN_MACOS,
        "LSApplicationCategoryType": "public.app-category.utilities", "NSHighResolutionCapable": True,
    }, sort_keys=True)


def icon(root: Path) -> bytes:
    png = (root / "assets/ledalert.png").read_bytes()
    verify.require(len(png) >= 24 and png[:8] == b"\x89PNG\r\n\x1a\n", "Missing PNG icon")
    width, height = struct.unpack_from(">II", png, 16)
    slots = {128: b"ic07", 256: b"ic08", 512: b"ic09", 1024: b"ic10"}
    verify.require(width == height and width in slots, "Native icon requires a square 128–1024 PNG")
    chunk = slots[width] + struct.pack(">I", len(png) + 8) + png
    return b"icns" + struct.pack(">I", len(chunk) + 8) + chunk


def validate_macho(data: bytes) -> None:
    """Portable structural gate; native codesign verification is a separate build/smoke gate."""
    verify.require(len(data) >= 32, "Truncated Mach-O")
    magic, cpu, _, kind, count, size, _, _ = struct.unpack_from("<8I", data)
    verify.require(magic == 0xFEEDFACF and cpu == 0x0100000C and kind == 2,
                   "Expected a thin Apple Silicon Mach-O executable")
    verify.require(count <= 4096 and 32 + size <= len(data), "Invalid Mach-O load commands")
    cursor, minimum, signature = 32, None, None
    for _ in range(count):
        verify.require(cursor + 8 <= 32 + size, "Truncated Mach-O load command")
        command, length = struct.unpack_from("<II", data, cursor)
        verify.require(length >= 8 and length % 8 == 0 and cursor + length <= 32 + size,
                       "Invalid Mach-O load-command length")
        if command == 0x32:
            verify.require(length >= 24 and minimum is None, "Invalid or duplicate Mach-O platform")
            target, minimum = struct.unpack_from("<II", data, cursor + 8)
            verify.require(target == 1, "Mach-O must target macOS, not iOS or a simulator")
        if command == 0x1D:
            verify.require(length == 16 and signature is None, "Invalid Mach-O signature command")
            signature = struct.unpack_from("<II", data, cursor + 8)
        cursor += length
    verify.require(cursor == 32 + size and minimum == (14 << 16 | 2 << 8),
                   "Mach-O deployment target differs from the 14.2 loader contract")
    verify.require(signature is not None, "Mach-O has no embedded code signature")
    offset, length = signature
    verify.require(offset >= cursor and length >= 12 and offset + length <= len(data),
                   "Mach-O signature is out of bounds")
    blob = data[offset:offset + length]
    magic, total, count = struct.unpack_from(">III", blob)
    verify.require(magic == 0xFADE0CC0 and 12 <= total <= len(blob) and count <= 64 and
                   12 + count * 8 <= total, "Malformed embedded signature")
    directories = 0
    for index in range(count):
        slot, start = struct.unpack_from(">II", blob, 12 + index * 8)
        verify.require(start >= 12 + count * 8 and start + 8 <= total, "Invalid signature slot")
        kind, size = struct.unpack_from(">II", blob, start)
        verify.require(size >= 8 and start + size <= total, "Truncated signature slot")
        verify.require(slot != 0x10000 or (kind == 0xFADE0B01 and size == 8),
                       "Expected ad-hoc signing, not a certificate/CMS identity")
        if kind == 0xFADE0C02:
            verify.require(size >= 44, "Truncated CodeDirectory")
            flags = struct.unpack_from(">I", blob, start + 12)[0]
            identity = struct.unpack_from(">I", blob, start + 20)[0]
            verify.require(flags & 2 and 44 <= identity < size, "CodeDirectory is not ad-hoc")
            end = blob.find(b"\0", start + identity, start + size)
            verify.require(end != -1 and blob[start + identity:end] == BUNDLE_ID.encode(),
                           "CodeDirectory bundle identity mismatch")
            directories += 1
    verify.require(directories > 0, "Missing ad-hoc CodeDirectory")


def documentation(root: Path) -> dict[str, bytes]:
    legal = verify.first_party_license(root)
    return {name: path.read_bytes() for name, path in release.source_files(root).items()
            if name.startswith(("docs/", "assets/")) or name in
            {"README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md", *legal.get("files", [])}}


def inspect_artifacts(root: Path, output: Path, *, commit: str | None = None) -> dict:
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    archive_name, checksum_name = names(version)
    sums = output / checksum_name
    verify.require(sums.is_file() and not sums.is_symlink() and sums.stat().st_size <= 4096,
                   "Missing or unsafe macOS checksums")
    archive = output / archive_name
    verify.require(archive.is_file() and not archive.is_symlink() and
                   archive.stat().st_size <= verify.MAX_ARCHIVE_BYTES, "Missing or unsafe macOS archive")
    verify.require(sums.read_text() == f"{verify.file_digest(archive)}  {archive_name}\n",
                   "macOS checksum mismatch")
    entries = verify.read_archive(archive, archive_name.removesuffix(".tar.gz"), executables=(BINARY,))
    files = {name: data for name, (data, _) in entries.items()}
    folded = {unicodedata.normalize("NFC", name).casefold() for name in files}
    verify.require(len(folded) == len(files), "macOS archive paths collide on a normal volume")
    required = {BINARY, PLIST, SIGNATURE, RESOURCES + "LedAlert.icns", RESOURCES + "licenses/INVENTORY.json",
                RESOURCES + "THIRD-PARTY-NOTICES.txt", "BUILD-INFO.json", "INSTALL.txt"}
    verify.require(required.issubset(files), "Missing macOS bundle members")
    info = verify.read_json(files["BUILD-INFO.json"], "macOS BUILD-INFO.json")
    verify.require(info.get("application") == "LedAlert" and info.get("version") == version and
                   info.get("target") == TARGET and info.get("minimum_macos") == MIN_MACOS and
                   info.get("rust_toolchain") == release.RUST_TOOLCHAIN and info.get("bundle_identifier") == BUNDLE_ID,
                   "macOS build identity mismatch")
    verify.require(info.get("source_commit") == commit, "macOS source commit mismatch")
    verify.require(info.get("signing") == "ad-hoc" and info.get("notarized") is False and
                   info.get("developer_id") is False, "macOS must make only an ad-hoc, unnotarized claim")
    verify.require(files[PLIST] == info_plist(version) and files[RESOURCES + "LedAlert.icns"] == icon(root) and
                   files["INSTALL.txt"] == INSTALL, "macOS launch manifest, icon or permission notice differs")
    validate_macho(files[BINARY])
    verify.require(info.get("binary_sha256") == release.digest(files[BINARY]), "macOS executable hash mismatch")
    sources = {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()}
    verify.require(info.get("source_files") == sources, "macOS package sources differ from checkout")
    payload = info.get("payload_sha256")
    verify.require(isinstance(payload, dict) and set(payload) == set(files) - {"BUILD-INFO.json"} and
                   all(release.digest(files[name]) == checksum for name, checksum in payload.items()),
                   "macOS payload inventory mismatch")
    resources = {name[len(RESOURCES):]: data for name, data in files.items() if name.startswith(RESOURCES)}
    notices = verify.inspect_target_notices(root, resources, TARGET)
    docs = documentation(root)
    verify.require(info.get("first_party_license") == verify.first_party_license(root) and
                   all(resources.get(name) == data for name, data in docs.items()),
                   "macOS documentation or first-party license mismatch")
    expected = required | {RESOURCES + name for name in notices | docs.keys()}
    verify.require(set(files) == expected and bool(files[SIGNATURE]), "Unexpected or missing macOS payload files")
    return info


def smoke(binary: Path, version: str) -> None:
    verify.require(release.command(str(binary), "--version") == f"LedAlert {version}", "macOS executable version mismatch")
    with tempfile.TemporaryDirectory(prefix="ledalert-macos-cli-") as temporary:
        config = Path(temporary) / "config.json"
        data = release.json_bytes(verify.synthetic_config())
        config.write_bytes(data)
        subprocess.run([str(binary), "--config", str(config), "check-config"], check=True, timeout=30)
        verify.require(config.read_bytes() == data, "macOS check-config changed the saved setup")
        missing = Path(temporary) / "missing.json"
        result = subprocess.run([str(binary), "--config", str(missing), "check-config"], capture_output=True, timeout=30)
        verify.require(result.returncode != 0 and not missing.exists(), "macOS missing-config validation changed state")


WINDOW_PROBE = r'''import AppKit
import CoreGraphics
let target = URL(fileURLWithPath: CommandLine.arguments[1]).standardizedFileURL.path
let stop = Date().addingTimeInterval(12)
while Date() < stop {
    if let app = NSWorkspace.shared.runningApplications.first(where: {
        $0.bundleURL?.standardizedFileURL.path == target
    }), let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
        for window in windows where (window[kCGWindowOwnerPID as String] as? Int32) == app.processIdentifier {
            if let bounds = window[kCGWindowBounds as String] as? [String: Any],
               let width = bounds["Width"] as? Double, let height = bounds["Height"] as? Double,
               width >= 100, height >= 100, (window[kCGWindowLayer as String] as? Int) == 0 {
                print(app.processIdentifier)
                exit(0)
            }
        }
    }
    Thread.sleep(forTimeInterval: 0.1)
}
exit(1)
'''


def gui_smoke(app: Path) -> None:
    """Launch only this extracted .app with an isolated, initially missing setup; never enable output."""
    with tempfile.TemporaryDirectory(prefix="ledalert-macos-gui-") as temporary:
        config = Path(temporary) / "new-setup.json"
        process = subprocess.Popen([str(app / "Contents/MacOS/ledalert"), "--config", str(config)],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            result = subprocess.run(["/usr/bin/swift", "-e", WINDOW_PROBE, str(app.resolve())],
                                    capture_output=True, text=True, timeout=45)
            verify.require(result.returncode == 0 and re.fullmatch(r"[1-9][0-9]*\n?", result.stdout) is not None,
                           "Extracted macOS application did not expose a native window")
            verify.require(int(result.stdout.strip()) == process.pid, "Observed a different application's window")
            verify.require(not config.exists(), "GUI smoke unexpectedly saved a setup")
            verify.report("macos_gui_smoke", status="passed", window="observed",
                          scope="Native window only; no permission grant, notification or lighting acceptance")
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


def native_verify(output: Path, version: str, gui: bool = False) -> None:
    verify.require(platform.system() == "Darwin" and platform.machine() == "arm64",
                   "Native macOS smoke requires Apple Silicon macOS")
    archive = output / names(version)[0]
    entries = verify.read_archive(archive, archive.name.removesuffix(".tar.gz"), executables=(BINARY,))
    with tempfile.TemporaryDirectory(prefix="ledalert-macos-extracted-") as temporary:
        directory = Path(temporary).resolve() / "package"
        verify.extract(entries, directory)
        app = directory / APP
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(app)], check=True, timeout=30)
        smoke(directory / BINARY, version)
        if gui:
            gui_smoke(app)
    verify.report("macos_native_smoke", status="passed", signature="ad-hoc integrity only",
                  notarized=False, apple_trust="not assessed")


def build(args: argparse.Namespace) -> None:
    root = release.ROOT
    verify.require(platform.system() == "Darwin" and platform.machine() == "arm64",
                   "macOS packaging requires native Apple Silicon macOS and Xcode command-line tools")
    verify.require(args.commit is None or re.fullmatch(r"[0-9a-f]{40}", args.commit), "Expected full lowercase commit SHA")
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    artifacts = names(version)
    output = args.output.resolve()
    release.prepare_output(output, artifacts)
    # Common metadata/runtime-notice helpers must use the same pinned toolchain as Cargo.
    env = os.environ
    env.update(RUSTUP_TOOLCHAIN=release.RUST_TOOLCHAIN, CARGO_TARGET_DIR=str(root / "target"),
               MACOSX_DEPLOYMENT_TARGET=MIN_MACOS)
    env.pop("RUSTFLAGS", None)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
        f"--remap-path-prefix={Path.home()}=/usr/src/build", f"--remap-path-prefix={root}=/usr/src/ledalert"])
    verify.require(release.command("rustc", "-V", env=env).startswith("rustc " + release.RUST_TOOLCHAIN + " "),
                   "Unexpected macOS Rust toolchain")
    if args.verify:
        for command in (["cargo", "test", "--all-targets", "--locked", "--target", TARGET],
                        [sys.executable, "-B", "-m", "unittest", "discover", "-s", "packaging", "-p", "test_*.py"]):
            subprocess.run(command, cwd=root, env=env, check=True)
    sources = {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()}
    notices, _ = release.notice_inputs(release.metadata(root, TARGET), root, TARGET)
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", TARGET, "--bin", "ledalert"],
                   cwd=root, env=env, check=True)
    binary = root / "target" / TARGET / "release/ledalert"
    with tempfile.TemporaryDirectory(prefix=".ledalert-macos-", dir=output) as temporary:
        staging = Path(temporary)
        layout = staging / "layout"
        files = {BINARY: binary.read_bytes(), PLIST: info_plist(version), RESOURCES + "LedAlert.icns": icon(root)}
        files.update({RESOURCES + name: data for name, data in {**notices, **documentation(root)}.items()})
        for name, data in files.items():
            destination = layout / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            destination.chmod(0o755 if name == BINARY else 0o644)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--timestamp=none", str(layout / APP)],
                       check=True, timeout=60)
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(layout / APP)],
                       check=True, timeout=30)
        files = {}
        for path in sorted(layout.rglob("*")):
            verify.require(not path.is_symlink(), "Signed macOS bundle contains a symlink")
            if path.is_file():
                files[path.relative_to(layout).as_posix()] = path.read_bytes()
        files["INSTALL.txt"] = INSTALL
        info = {"application": "LedAlert", "version": version, "target": TARGET, "minimum_macos": MIN_MACOS,
                "macos_build_host": platform.mac_ver()[0], "sdk_version": release.command("xcrun", "--show-sdk-version"),
                "rust_toolchain": release.RUST_TOOLCHAIN, "bundle_identifier": BUNDLE_ID,
                "binary_sha256": release.digest(files[BINARY]), "source_commit": args.commit,
                "source_files": sources, "signing": "ad-hoc", "notarized": False, "developer_id": False,
                "first_party_license": verify.first_party_license(root),
                "payload_sha256": {name: release.digest(data) for name, data in sorted(files.items())}}
        files["BUILD-INFO.json"] = release.json_bytes(info)
        verify.require(sources == {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()},
                       "macOS source inputs changed during build")
        entries = {name: (data, 0o755 if name == BINARY else 0o644) for name, data in files.items()}
        epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
        release.archive(staging / artifacts[0], artifacts[0].removesuffix(".tar.gz"), entries, epoch)
        (staging / artifacts[1]).write_text(f"{verify.file_digest(staging / artifacts[0])}  {artifacts[0]}\n")
        inspect_artifacts(root, staging, commit=args.commit)
        native_verify(staging, version, args.gui_smoke)
        release.publish(staging, output, artifacts)
    verify.report("macos_package", status="passed", artifacts=artifacts, signing="ad-hoc", notarized=False)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=release.ROOT / "dist/macos")
    parser.add_argument("--inspect", action="store_true", help="Inspect existing artifacts without executing them")
    parser.add_argument("--verify", action="store_true", help="Run native Rust and portable packaging regressions before building")
    parser.add_argument("--gui-smoke", action="store_true", help="Explicitly launch the isolated extracted app; no output or permissions")
    parser.add_argument("--commit", help="Exact source commit for CI provenance")
    args = parser.parse_args()
    try:
        if args.inspect:
            info = inspect_artifacts(release.ROOT, args.output.resolve(), commit=args.commit)
            if args.gui_smoke:
                native_verify(args.output.resolve(), info["version"], True)
            verify.report("macos_inventory", status="passed", native_signature="not checked unless --gui-smoke was requested")
        else:
            build(args)
    except (ValueError, OSError, KeyError, TypeError, struct.error, plistlib.InvalidFileException,
            subprocess.SubprocessError) as error:
        parser.exit(1, f"macos.py: {error}\n")


if __name__ == "__main__":
    main()
