"""Windows package consumer/security regressions; no SDK, Cargo or Windows runtime required."""
from __future__ import annotations

from pathlib import Path
import stat
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET
import zipfile

import release
import publish as publication
import verify
import windows
from test_macos import fixture_bundle, write_fixture


class ManifestTests(unittest.TestCase):
    def test_release_versions_preserve_upgrade_order_without_collisions(self):
        versions = ("0.2.9", "0.3.0-alpha", "0.3.0-alpha.1", "0.3.0-alpha.9999",
                    "0.3.0-beta", "0.3.0-beta.1", "0.3.0-beta.9999",
                    "0.3.0-rc", "0.3.0-rc.1", "0.3.0-rc.9999", "0.3.0", "0.3.1-alpha")
        mapped = [tuple(map(int, windows.package_version(version).split("."))) for version in versions]
        self.assertTrue(all(earlier < later for earlier, later in zip(mapped, mapped[1:])))
        self.assertEqual(windows.package_version("0.3.0-alpha"), "0.3.0.10000")
        self.assertEqual(windows.package_version("0.3.0"), "0.3.0.40000")
        self.assertEqual(windows.package_version("65535.65535.65535"), "65535.65535.65535.40000")

    def test_ambiguous_unsupported_and_out_of_range_versions_are_rejected(self):
        for value in ("0.3.0-preview", "0.3.0+build", "0.3", "0.3.0.1", "01.2.3", "1.65536.0",
                      "65536.0.0", "0.0.65536", "../0.3.0", "0.3.0-alpha.0", "0.3.0-alpha.01",
                      "0.3.0-alpha.10000", "0.3.0-beta.10000", "0.3.0-rc.10000",
                      "0.3.0-ALPHA", "0.3.0-alpha.1.2", "0.3.0-1", "0.3.0-final", "0.3.0-rc+build"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                windows.package_version(value)

    def test_publisher_is_xml_escaped_and_roundtrips(self):
        publisher = 'CN=Example & Company, O="Example"'
        data = windows.manifest("0.3.0", publisher)
        windows.validate_manifest(data, "0.3.0", publisher)
        self.assertEqual(ET.fromstring(data).find("Identity", windows.NS).get("Publisher"), publisher)

    def test_missing_permission_cannot_produce_installable_contract(self):
        for capability in ("userNotificationListener", "runFullTrust", "globalMediaControl"):
            tree = ET.fromstring(windows.manifest("0.3.0", "CN=LedAlert"))
            capabilities = tree.find("Capabilities", windows.NS)
            for child in list(capabilities):
                if child.get("Name") == capability:
                    capabilities.remove(child)
            with self.subTest(capability=capability), self.assertRaises(ValueError):
                windows.validate_manifest(ET.tostring(tree), "0.3.0", "CN=LedAlert")

    def test_foreign_activation_and_wrong_publisher_are_rejected(self):
        data = windows.manifest("0.3.0", "CN=LedAlert")
        with self.assertRaises(ValueError):
            windows.validate_manifest(data, "0.3.0", "CN=Other")
        tree = ET.fromstring(data)
        tree.find("Applications/Application", windows.NS).set("Executable", "other.exe")
        with self.assertRaises(ValueError):
            windows.validate_manifest(ET.tostring(tree), "0.3.0", "CN=LedAlert")


class ZipSafetyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-windows-zip-")
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name) / "package.zip"

    def test_windows_path_aliases_cannot_escape_or_overwrite(self):
        for name in ("../outside", "/absolute", "C:/outside", "a\\b", "file:stream", "AUX.txt", "a/COM1",
                     "CONIN$", "a/CONOUT$.txt", "NUL .txt", "COM¹.txt", "a/LPT²", "LPT³.log", "name.", "name "):
            with self.subTest(name=name):
                with zipfile.ZipFile(self.path, "w") as archive:
                    # ZipInfo normalizes host separators in its constructor.
                    # Set the raw wire name afterward to exercise Windows too.
                    member = zipfile.ZipInfo()
                    member.filename = name
                    archive.writestr(member, b"invalid")
                with self.assertRaises(ValueError):
                    windows.read_zip(self.path)

    def test_case_insensitive_duplicate_is_rejected(self):
        with zipfile.ZipFile(self.path, "w") as archive:
            archive.writestr("ledalert.exe", b"first")
            archive.writestr("LEDALERT.EXE", b"second")
        with self.assertRaises(ValueError):
            windows.read_zip(self.path)

    def test_zip_symlink_is_rejected_without_extraction(self):
        member = zipfile.ZipInfo("ledalert.exe")
        member.create_system = 3
        member.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(self.path, "w") as archive:
            archive.writestr(member, b"../outside")
        with self.assertRaises(ValueError):
            windows.read_zip(self.path)
        self.assertFalse((self.path.parent / "outside").exists())

    def test_archive_creation_preserves_existing_download(self):
        self.path.write_bytes(b"original")
        with self.assertRaises(FileExistsError):
            windows.write_zip(self.path, {"ledalert.exe": b"replacement"})
        self.assertEqual(self.path.read_bytes(), b"original")

    def test_cli_preserves_existing_output_before_sdk_access(self):
        version = tomllib.loads((release.ROOT / "Cargo.toml").read_text())["package"]["version"]
        existing = self.path.parent / windows.names(version)[0]
        existing.write_bytes(b"original")
        result = subprocess.run([sys.executable, "-B", str(release.ROOT / "packaging/windows.py"),
                                 "--output", str(self.path.parent)], capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(existing.read_bytes(), b"original")
        self.assertFalse((self.path.parent / windows.names(version)[1]).exists())


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-windows-artifact-")
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
        manifest = self.root / "packaging/windows/AppxManifest.xml"
        manifest.parent.mkdir(parents=True, exist_ok=True)
        manifest.write_bytes((release.ROOT / "packaging/windows/AppxManifest.xml").read_bytes())
        (self.root / "docs").mkdir()
        (self.root / "docs/reference.md").write_bytes(b"# Offline manual\n")
        executable = bytearray(128)
        executable[:2] = b"MZ"
        struct.pack_into("<I", executable, 60, 64)
        executable[64:68] = b"PE\0\0"
        struct.pack_into("<H", executable, 68, 0x8664)
        struct.pack_into("<H", executable, 88, 0x20B)
        self.entries = {
            "ledalert.exe": bytes(executable), "AppxManifest.xml": windows.manifest("0.3.0", "CN=LedAlert", self.root),
            "PORTABLE.txt": windows.PORTABLE_NOTICE, "THIRD-PARTY-NOTICES.txt": b"Third-party terms\n",
            "licenses/fixture/LICENSE": b"Fixture license\n", "licenses/rust/LICENSE": b"Runtime license\n",
            "licenses/Silkscreen-OFL.txt": b"Font license\n", "licenses/Silkscreen-metadata.txt": b"Font attribution\n",
            "licenses/Saira-OFL.txt": b"Documentation font license\n",
            **{name: windows.logo(size) for name, size in windows.LOGOS.items()},
            **{name: (self.root / name).read_bytes() for name in ("README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md", "docs/reference.md")},
        }
        self.inventory = {"target": windows.TARGET, "packages": [{
            "name": "fixture", "version": "1.0.0", "crate_sha256": "a" * 64, "registry_source": "registry+fixture",
            "license_expression": "MIT", "notices": [{"path": "licenses/fixture/LICENSE",
                "sha256": release.digest(self.entries["licenses/fixture/LICENSE"])}]}],
            "rust_runtime": {"notices": [{"path": "licenses/rust/LICENSE", "sha256": release.digest(self.entries["licenses/rust/LICENSE"])}]}}
        self.entries["licenses/INVENTORY.json"] = release.json_bytes(self.inventory)
        self.info = {"application": "LedAlert", "version": "0.3.0", "target": windows.TARGET,
                     "minimum_windows": windows.MIN_WINDOWS, "rust_toolchain": release.RUST_TOOLCHAIN,
                     "source_commit": None, "publisher": "CN=LedAlert", "signing": "unsigned",
                     "binary_sha256": release.digest(bytes(executable)), "first_party_license": verify.first_party_license(self.root),
                     "source_files": {name: release.digest(path.read_bytes()) for name, path in release.source_files(self.root).items()}}

    def package(self, *, msix_overrides=None):
        self.info["payload_sha256"] = {name: release.digest(data) for name, data in self.entries.items()}
        portable = {**self.entries, "BUILD-INFO.json": release.json_bytes(self.info)}
        msix = {**portable, "AppxBlockMap.xml": b"<BlockMap/>", "[Content_Types].xml": b"<Types/>"}
        msix.update(msix_overrides or {})
        for name, entries in zip(windows.names(self.info["version"])[:2], (portable, msix)):
            windows.write_zip(self.output / name, entries)
        (self.output / "WINDOWS-SHA256SUMS").write_text("".join(
            f"{verify.file_digest(self.output / name)}  {name}\n" for name in windows.names(self.info["version"])[:2]))

    def test_offline_manual_cannot_be_stripped_from_a_windows_release(self):
        del self.entries["docs/reference.md"]
        self.package()
        with self.assertRaises(ValueError):
            windows.inspect_artifacts(self.root, self.output)

    def test_unsigned_local_package_is_valid_but_not_publishable(self):
        self.package()
        info = windows.inspect_artifacts(self.root, self.output)
        self.assertEqual(info["target"], windows.TARGET)
        with self.assertRaisesRegex(ValueError, "signed MSIX"):
            windows.inspect_artifacts(self.root, self.output, require_signed=True)

    def publication_bundle(self, version):
        commit = "1" * 40
        (self.root / "Cargo.toml").write_text(f'[package]\nname = "ledalert"\nversion = "{version}"\n')
        for name in ("assets/ledalert.png", "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((release.ROOT / name).read_bytes() if name.endswith(".png") else b"public asset fixture\n")
            self.entries[name] = path.read_bytes()
        sources = release.source_files(self.root)
        self.info.update(version=version, source_commit=commit,
                         source_files={name: release.digest(path.read_bytes()) for name, path in sources.items()})
        self.entries["AppxManifest.xml"] = windows.manifest(version, "CN=LedAlert", self.root)
        self.package()
        crate_sources = {"Cargo.toml", "Cargo.lock", "build.rs", "README.md", "assets/ledalert.png",
                         "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt"}
        crate = {name: (sources[name].read_bytes(), 0o644) for name in crate_sources}
        crate["Cargo.toml.orig"] = crate["Cargo.toml"]
        names = publication.artifact_names(version)
        release.archive(self.output / names[3], f"ledalert-{version}", crate, 0)
        info = {"version": version, "source_commit": commit, "source_files": self.info["source_files"],
                "crate_sha256": verify.file_digest(self.output / names[3])}
        build_info = {"BUILD-INFO.json": (release.json_bytes(info), 0o644)}
        source = {name: (path.read_bytes(), 0o644) for name, path in sources.items()}
        release.archive(self.output / names[0], f"ledalert-{version}-linux-x86_64", build_info, 0)
        release.archive(self.output / names[1], f"ledalert-{version}-source", {**source, **build_info}, 0)
        notices = {name: data for name, data in self.entries.items()
                   if name.startswith("licenses/") or name == "THIRD-PARTY-NOTICES.txt"}
        self.mac_entries, self.mac_info = fixture_bundle(self.root, version, commit, notices)
        write_fixture(self.output, version, self.mac_entries, self.mac_info)
        (self.output / "SHA256SUMS").write_text("".join(
            f"{verify.file_digest(self.output / name)}  {name}\n" for name in names if name != "SHA256SUMS"))
        return commit

    def test_unsigned_alpha_passes_real_publication_inventory(self):
        commit = self.publication_bundle("0.3.0-alpha")
        with patch.object(release, "ROOT", self.root):
            receipt = publication.local_receipt(self.output, "0.3.0-alpha", commit)
        self.assertTrue(receipt["prerelease"])
        self.assertEqual(receipt["windows_signing"], "unsigned")
        self.assertEqual(receipt["sha256"]["ledalert-0.3.0-alpha-windows-x86_64.msix"],
                         verify.file_digest(self.output / "ledalert-0.3.0-alpha-windows-x86_64.msix"))

    def test_unsigned_stable_fails_real_publication_inventory(self):
        commit = self.publication_bundle("0.3.0")
        with patch.object(release, "ROOT", self.root), self.assertRaisesRegex(ValueError, "signed MSIX"):
            publication.local_receipt(self.output, "0.3.0", commit)

    def test_mixed_commit_macos_artifact_cannot_authorize_an_otherwise_valid_prerelease(self):
        version = "0.3.0-alpha"
        commit = self.publication_bundle(version)
        with patch.object(release, "ROOT", self.root):
            publication.local_receipt(self.output, version, commit)
        self.mac_info["source_commit"] = "2" * 40
        write_fixture(self.output, version, self.mac_entries, self.mac_info)
        (self.output / "SHA256SUMS").write_text("".join(
            f"{verify.file_digest(self.output / name)}  {name}\n"
            for name in publication.artifact_names(version) if name != "SHA256SUMS"))
        with patch.object(release, "ROOT", self.root), self.assertRaises(ValueError):
            publication.local_receipt(self.output, version, commit)

    def test_wrong_architecture_fails_despite_consistent_hashes(self):
        executable = bytearray(self.entries["ledalert.exe"])
        struct.pack_into("<H", executable, 68, 0x14C)
        self.entries["ledalert.exe"] = bytes(executable)
        self.info["binary_sha256"] = release.digest(bytes(executable))
        self.package()
        with self.assertRaisesRegex(ValueError, "x64 PE32"):
            windows.inspect_artifacts(self.root, self.output)

    def test_portable_and_msix_must_contain_identical_application(self):
        self.package(msix_overrides={"ledalert.exe": b"different executable"})
        with self.assertRaisesRegex(ValueError, "payloads differ"):
            windows.inspect_artifacts(self.root, self.output)

    def test_missing_notice_fails_even_with_rehashed_payload(self):
        del self.entries["licenses/fixture/LICENSE"]
        self.package()
        with self.assertRaisesRegex(ValueError, "notice"):
            windows.inspect_artifacts(self.root, self.output)

    def test_unexpected_executable_fails_even_with_rehashed_payload(self):
        self.entries["other.exe"] = self.entries["ledalert.exe"]
        self.package()
        with self.assertRaisesRegex(ValueError, "payload files"):
            windows.inspect_artifacts(self.root, self.output)

    def test_wrong_source_commit_cannot_be_published(self):
        self.package()
        with self.assertRaisesRegex(ValueError, "source commit"):
            windows.inspect_artifacts(self.root, self.output, commit="1" * 40)

    def test_changed_source_cannot_be_adopted(self):
        self.package()
        (self.root / "Cargo.lock").write_text("different lock file")
        with self.assertRaisesRegex(ValueError, "sources differ"):
            windows.inspect_artifacts(self.root, self.output)

    def test_signature_claim_without_signature_is_rejected(self):
        self.info["signing"] = "certificate-store"
        self.package()
        with self.assertRaisesRegex(ValueError, "signing envelope"):
            windows.inspect_artifacts(self.root, self.output)

    def test_checksum_cannot_omit_or_duplicate_an_asset(self):
        self.package()
        checksum = self.output / "WINDOWS-SHA256SUMS"
        first = checksum.read_text().splitlines()[0] + "\n"
        checksum.write_text(first + first)
        with self.assertRaisesRegex(ValueError, "checksums"):
            windows.inspect_artifacts(self.root, self.output)


if __name__ == "__main__":
    unittest.main()
