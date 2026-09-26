"""Portable macOS package consumer regressions. Synthetic Mach-O fixtures are never executed."""
from __future__ import annotations

from pathlib import Path
import plistlib
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest

import macos
import release
import verify


def macho() -> bytes:
    identity = macos.BUNDLE_ID.encode() + b"\0"
    directory = bytearray(44 + len(identity))
    struct.pack_into(">6I", directory, 0, 0xFADE0C02, len(directory), 0x20000, 2, 0, 44)
    directory[44:] = identity
    signature = (struct.pack(">7I", 0xFADE0CC0, 36 + len(directory), 2, 0, 28,
                             0x10000, 28 + len(directory)) + directory +
                 struct.pack(">2I", 0xFADE0B01, 8))
    return (struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 2, 2, 40, 0, 0) +
            struct.pack("<6I", 0x32, 24, 1, 14 << 16 | 2 << 8, 27 << 16, 0) +
            struct.pack("<4I", 0x1D, 16, 72, len(signature)) + signature)


def fixture_bundle(root, version, commit, notices):
    inventory = verify.read_json(notices["licenses/INVENTORY.json"], "fixture inventory")
    inventory["target"] = macos.TARGET
    resources = {**notices, **macos.documentation(root), "LedAlert.icns": macos.icon(root),
                 "licenses/INVENTORY.json": release.json_bytes(inventory)}
    entries = {macos.BINARY: macho(), macos.PLIST: macos.info_plist(version),
               macos.SIGNATURE: b"structural seal fixture, not trusted", "INSTALL.txt": macos.INSTALL,
               **{macos.RESOURCES + name: data for name, data in resources.items()}}
    info = {"application": "LedAlert", "version": version, "target": macos.TARGET,
            "minimum_macos": macos.MIN_MACOS, "bundle_identifier": macos.BUNDLE_ID,
            "rust_toolchain": release.RUST_TOOLCHAIN, "source_commit": commit,
            "signing": "ad-hoc", "notarized": False, "developer_id": False,
            "binary_sha256": release.digest(entries[macos.BINARY]),
            "first_party_license": verify.first_party_license(root),
            "source_files": {name: release.digest(path.read_bytes()) for name, path in release.source_files(root).items()}}
    return entries, info


def write_fixture(output, version, entries, info, modes=None):
    info["payload_sha256"] = {name: release.digest(data) for name, data in entries.items() if name != "BUILD-INFO.json"}
    entries["BUILD-INFO.json"] = release.json_bytes(info)
    archive, sums = macos.names(version)
    path = output / archive
    path.unlink(missing_ok=True)  # Only an isolated fixture's disposable prior archive.
    release.archive(path, archive.removesuffix(".tar.gz"), {
        name: (data, (modes or {}).get(name, 0o755 if name == macos.BINARY else 0o644))
        for name, data in entries.items()}, 0)
    (output / sums).write_text(f"{verify.file_digest(path)}  {archive}\n")


class MachOTests(unittest.TestCase):
    def test_wrong_architecture_platform_minimum_and_signature_cannot_claim_mac_support(self):
        baseline = macho()
        macos.validate_macho(baseline)
        # Header CPU/file type; platform/minimum; signature bounds; CodeDirectory flags/identity.
        changes = [(4, "<I", 0x01000007), (12, "<I", 6), (40, "<I", 2),
                   (44, "<I", 27 << 16), (64, "<I", 0xFFFFFFFF), (112, ">I", 0),
                   (120, ">I", 0xFFFFFFFF)]
        for offset, encoding, value in changes:
            data = bytearray(baseline)
            struct.pack_into(encoding, data, offset, value)
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                macos.validate_macho(bytes(data))

    def test_truncated_and_duplicate_load_commands_fail_before_native_execution(self):
        baseline = macho()
        for size in (0, 31, 39, 71, len(baseline) - 1):
            with self.subTest(size=size), self.assertRaises(ValueError):
                macos.validate_macho(baseline[:size])
        duplicate = bytearray(baseline)
        struct.pack_into("<I", duplicate, 56, 0x32)
        with self.assertRaises(ValueError):
            macos.validate_macho(bytes(duplicate))

    def test_nonempty_cms_cannot_claim_ad_hoc_signing(self):
        data = bytearray(macho())
        data += b"x"
        struct.pack_into("<I", data, 68, len(data) - 72)
        struct.pack_into(">I", data, 76, len(data) - 72)
        struct.pack_into(">I", data, len(data) - 5, 9)
        with self.assertRaises(ValueError):
            macos.validate_macho(bytes(data))


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-macos-artifact-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "source"
        self.output = Path(temporary.name) / "output"
        self.root.mkdir()
        self.output.mkdir()
        for name in release.FILES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"public source fixture\n")
        (self.root / "Cargo.toml").write_text('[package]\nname = "ledalert"\nversion = "0.3.0"\n')
        (self.root / "Cargo.lock").write_text('version = 4\n[[package]]\nname = "fixture"\nversion = "1.0.0"\n'
                                             'source = "registry+fixture"\nchecksum = "' + 'a' * 64 + '"\n')
        (self.root / "assets").mkdir()
        (self.root / "assets/ledalert.png").write_bytes((release.ROOT / "assets/ledalert.png").read_bytes())
        resources = {
            "THIRD-PARTY-NOTICES.txt": b"Third-party terms\n", "licenses/fixture/LICENSE": b"Fixture license\n",
            "licenses/rust/LICENSE": b"Runtime license\n", "licenses/Silkscreen-OFL.txt": b"Font license\n",
            "licenses/Silkscreen-metadata.txt": b"Font attribution\n", "licenses/Saira-OFL.txt": b"Font license\n",
        }
        self.inventory = {"target": macos.TARGET, "packages": [{
            "name": "fixture", "version": "1.0.0", "crate_sha256": "a" * 64, "registry_source": "registry+fixture",
            "license_expression": "MIT", "notices": [{"path": "licenses/fixture/LICENSE",
                "sha256": release.digest(resources["licenses/fixture/LICENSE"])}]}],
            "rust_runtime": {"notices": [{"path": "licenses/rust/LICENSE",
                "sha256": release.digest(resources["licenses/rust/LICENSE"])}]}}
        resources["licenses/INVENTORY.json"] = release.json_bytes(self.inventory)
        self.entries, self.info = fixture_bundle(self.root, "0.3.0", None, resources)

    def package(self, modes=None):
        write_fixture(self.output, "0.3.0", self.entries, self.info, modes)

    def test_local_worktree_package_cannot_be_claimed_as_a_commit_build(self):
        self.package()
        macos.inspect_artifacts(self.root, self.output)
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output, commit="f" * 40)

    def test_rehashed_package_cannot_claim_developer_identity_or_notarization(self):
        for key, value in (("signing", "developer-id"), ("notarized", True), ("developer_id", True)):
            prior = self.info[key]
            self.info[key] = value
            self.package()
            with self.subTest(key=key), self.assertRaises(ValueError):
                macos.inspect_artifacts(self.root, self.output)
            self.info[key] = prior

    def test_foreign_executable_identity_or_entitlement_cannot_replace_launch_manifest(self):
        for key, value in (("CFBundleExecutable", "other"), ("CFBundleIdentifier", "org.example.Other"),
                           ("LSUIElement", True), ("LSMinimumSystemVersion", "10.0")):
            manifest = plistlib.loads(macos.info_plist("0.3.0"))
            manifest[key] = value
            self.entries[macos.PLIST] = plistlib.dumps(manifest)
            self.package()
            with self.subTest(key=key), self.assertRaises(ValueError):
                macos.inspect_artifacts(self.root, self.output)

    def test_changed_executable_or_source_is_not_authorized_by_rehashed_download(self):
        self.entries[macos.BINARY] += b"altered executable"
        self.package()
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)
        self.entries[macos.BINARY] = macho()
        self.package()
        (self.root / "README.md").write_bytes(b"different checkout")
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)

    def test_missing_notice_is_rejected_even_when_download_and_payload_hashes_match(self):
        del self.entries[macos.RESOURCES + "licenses/fixture/LICENSE"]
        self.package()
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)

    def test_archive_executable_allowlist_is_specific_to_the_app_bundle(self):
        self.package({macos.BINARY: 0o644})
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)
        self.entries["bin/ledalert"] = b"not an allowed macOS entry point"
        self.package({"bin/ledalert": 0o755})
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)

    def test_distinct_notice_names_cannot_alias_on_a_normal_macos_volume(self):
        for name in ("licenses/fixture/Caf\u00e9", "licenses/fixture/Cafe\u0301"):
            data = b"Additional notice"
            self.entries[macos.RESOURCES + name] = data
            self.inventory["packages"][0]["notices"].append({"path": name, "sha256": release.digest(data)})
        self.entries[macos.RESOURCES + "licenses/INVENTORY.json"] = release.json_bytes(self.inventory)
        self.package()
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)

    def test_corrupted_download_is_rejected_before_extraction(self):
        self.package()
        with (self.output / macos.names("0.3.0")[0]).open("ab") as file:
            file.write(b"corruption")
        with self.assertRaises(ValueError):
            macos.inspect_artifacts(self.root, self.output)


class PreservationTests(unittest.TestCase):
    def test_cli_never_overwrites_an_existing_native_download(self):
        version = tomllib.loads((release.ROOT / "Cargo.toml").read_text())["package"]["version"]
        with tempfile.TemporaryDirectory(prefix="ledalert-macos-preserve-") as temporary:
            output = Path(temporary)
            archive = output / macos.names(version)[0]
            archive.write_bytes(b"original download")
            result = subprocess.run([sys.executable, "-B", str(release.ROOT / "packaging/macos.py"),
                                     "--output", str(output)], capture_output=True, timeout=15)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(archive.read_bytes(), b"original download")
            self.assertEqual({path.name for path in output.iterdir()}, {archive.name})


if __name__ == "__main__":
    unittest.main()
