"""Consumer-visible source-export and artifact-envelope regressions (no Cargo builds)."""
from __future__ import annotations

import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

import release
import publish as publication
import verify


class SourceExportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-source-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in release.FILES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("public fixture\n")
        (self.root / "Cargo.toml").write_text('[package]\nname = "ledalert"\nversion = "0.1.0"\n')
        (self.root / "src").mkdir()
        (self.root / "src/main.rs").write_text("fn main() {}\n")
        patched = patch.object(release, "ROOT", self.root)
        patched.start()
        self.addCleanup(patched.stop)

    def test_symlink_cannot_export_external_source(self):
        outside = self.root / "private.txt"
        outside.write_text("not public")
        (self.root / "src/escape.rs").symlink_to(outside)
        with self.assertRaises(ValueError):
            release.source_files()

    def test_symlinked_source_tree_is_rejected(self):
        private = self.root / "private"
        private.mkdir()
        (private / "hidden.rs").write_text("not public")
        (self.root / "tests").symlink_to(private, target_is_directory=True)
        with self.assertRaises(ValueError):
            release.source_files()

    def test_allowlisted_workflow_cannot_traverse_symlinked_parent(self):
        (self.root / ".github").rename(self.root / "private-ci")
        (self.root / ".github").symlink_to(self.root / "private-ci", target_is_directory=True)
        with self.assertRaises(ValueError):
            release.source_files()

    def test_hidden_input_inside_exported_tree_is_rejected(self):
        hidden = self.root / "src/.private"
        hidden.mkdir()
        (hidden / "credentials").write_text("not public")
        with self.assertRaises(ValueError):
            release.source_files()

    def test_unrelated_dotfiles_are_not_exported(self):
        (self.root / ".env").write_text("not public")
        (self.root / ".ssh").mkdir()
        (self.root / ".ssh/key").write_text("not public")
        (self.root / ".impeccable").mkdir()
        (self.root / ".impeccable/design.json").write_text('{"private": true}')
        (self.root / "PRODUCT.md").write_text("private product context")
        (self.root / "DESIGN.md").write_text("private design context")
        exported = release.source_files()
        self.assertNotIn(".env", exported)
        self.assertNotIn(".ssh/key", exported)
        self.assertNotIn(".impeccable/design.json", exported)
        self.assertNotIn("PRODUCT.md", exported)
        self.assertNotIn("DESIGN.md", exported)
        self.assertIn(".github/workflows/verify.yml", exported)
        self.assertIn(".github/workflows/pages.yml", exported)
        self.assertIn("src/main.rs", exported)

    def test_crate_cannot_include_capture_or_private_context(self):
        for name in ("assets/ledalert.png", "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"public embedded asset")
        names = ("Cargo.toml", "Cargo.lock", "README.md", "src/main.rs", "assets/ledalert.png",
                 "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt")
        entries = {name: ((self.root / name).read_bytes(), 0o644) for name in names}
        entries["Cargo.toml.orig"] = entries["Cargo.toml"]
        for index, private in enumerate(("PRODUCT.md", "docs/site/assets/screenshots/capture.png")):
            with self.subTest(private=private):
                path = self.root / f"rejected-{index}.crate"
                release.archive(path, "ledalert-0.1.0", {**entries, private: (b"not a crate input", 0o644)}, 0)
                with self.assertRaises(ValueError):
                    verify.inspect_crate(self.root, path)

    def test_crate_must_retain_embedded_font_notice(self):
        for name in ("assets/ledalert.png", "assets/fonts/Silkscreen-Bold.ttf", "assets/fonts/OFL.txt"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"public embedded asset")
        names = ("Cargo.toml", "Cargo.lock", "README.md", "src/main.rs", "assets/ledalert.png",
                 "assets/fonts/Silkscreen-Bold.ttf")
        entries = {name: ((self.root / name).read_bytes(), 0o644) for name in names}
        entries["Cargo.toml.orig"] = entries["Cargo.toml"]
        path = self.root / "missing-notice.crate"
        release.archive(path, "ledalert-0.1.0", entries, 0)
        with self.assertRaises(ValueError):
            verify.inspect_crate(self.root, path)


class PreservationTests(unittest.TestCase):
    def test_cli_preserves_existing_download_before_builder_access(self):
        version = tomllib.loads((verify.ROOT / "Cargo.toml").read_text())["package"]["version"]
        with tempfile.TemporaryDirectory(prefix="ledalert-overwrite-test-") as temporary:
            output = Path(temporary)
            existing = output / f"ledalert-{version}-linux-x86_64.tar.gz"
            original = b"existing download must survive"
            existing.write_bytes(original)
            result = subprocess.run([sys.executable, "-B", str(verify.ROOT / "packaging/release.py"),
                                     "--verify", "--output", str(output)], capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(existing.read_bytes(), original)
            self.assertEqual({path.name for path in output.iterdir()}, {existing.name})

    def test_archive_creation_does_not_truncate_existing_download(self):
        with tempfile.TemporaryDirectory(prefix="ledalert-archive-test-") as temporary:
            path = Path(temporary) / "existing.tar.gz"
            path.write_bytes(b"original archive")
            with self.assertRaises(FileExistsError):
                release.archive(path, "release", {"new": (b"replacement", 0o644)}, 0)
            self.assertEqual(path.read_bytes(), b"original archive")


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.commit = "1" * 40
        self.hashes = {name: "a" * 64 for name in release.release_names("0.2.0", True)}
        self.receipt = {"version": "0.2.0", "commit": self.commit, "sha256": self.hashes}
        self.record = {
            "body": f"<!-- ledalert-release:{json.dumps(self.receipt)} -->",
            "target_commitish": self.commit, "draft": False, "prerelease": False,
            "assets": [{"name": name, "state": "uploaded", "digest": "sha256:" + checksum}
                       for name, checksum in self.hashes.items()],
        }
        self.entry = {"name": "ledalert", "vers": "0.2.0", "cksum": "a" * 64, "yanked": False}
        for name, value in (("github", self.record), ("tag_commit", self.commit), ("registry_version", self.entry)):
            patched = patch.object(publication, name, return_value=value)
            mock = patched.start()
            self.addCleanup(patched.stop)
            setattr(self, name, mock)

    def test_published_version_is_not_republished_on_later_main_push(self):
        self.assertFalse(publication.preflight("0.2.0", "2" * 40))

    def test_partial_publication_requires_original_source_commit(self):
        self.registry_version.return_value = None
        with self.assertRaises(ValueError):
            publication.preflight("0.2.0", "2" * 40)
        self.assertTrue(publication.preflight("0.2.0", self.commit))

    def test_changed_release_bytes_cannot_be_adopted_as_complete(self):
        self.record["assets"][0]["digest"] = "sha256:" + "b" * 64
        with self.assertRaises(ValueError):
            publication.preflight("0.2.0", self.commit)

    def test_missing_assets_can_resume_only_in_an_unpublished_draft(self):
        self.registry_version.return_value = None
        self.record["assets"].pop()
        with self.assertRaises(ValueError):
            publication.preflight("0.2.0", self.commit)
        self.record["draft"] = True
        self.assertTrue(publication.preflight("0.2.0", self.commit))

    def test_registry_version_with_different_bytes_cannot_be_republished(self):
        self.entry["cksum"] = "b" * 64
        with self.assertRaises(ValueError):
            publication.preflight("0.2.0", self.commit)

    def test_remote_auth_failure_is_not_absence(self):
        url = "https://api.github.com/repos/CritX-ai/LedAlert/releases/tags/v0.2.0"
        with patch.object(publication, "urlopen", side_effect=HTTPError(url, 404, "missing", {}, None)):
            self.assertIsNone(publication.request(url, missing=True))
        with patch.object(publication, "urlopen", side_effect=HTTPError(url, 403, "forbidden", {}, None)):
            with self.assertRaises(ValueError):
                publication.request(url, missing=True)

    def test_local_snapshot_cannot_be_published_as_a_commit(self):
        with tempfile.TemporaryDirectory(prefix="ledalert-local-publication-") as temporary:
            output = Path(temporary)
            names = release.release_names("0.2.0", True)
            crate = output / names[3]
            crate.write_bytes(b"local crate fixture")
            info = {"version": "0.2.0", "source_commit": None, "crate_sha256": verify.file_digest(crate)}
            for name, prefix in zip(names[:2], ("ledalert-0.2.0-linux-x86_64", "ledalert-0.2.0-source")):
                release.archive(output / name, prefix, {"BUILD-INFO.json": (release.json_bytes(info), 0o644)}, 0)
            (output / "SHA256SUMS").write_text("".join(
                f"{verify.file_digest(output / name)}  {name}\n" for name in names if name != "SHA256SUMS"))
            with self.assertRaises(ValueError):
                publication.local_receipt(output, "0.2.0", self.commit)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-artifact-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "source"
        self.root.mkdir()
        self.output = Path(temporary.name) / "output"
        self.output.mkdir()
        self.prefixes = ("ledalert-0.1.0-linux-x86_64", "ledalert-0.1.0-source")
        sources = {name: b"public fixture\n" for name in release.FILES}
        sources.update({
            "Cargo.toml": b'[package]\nname = "ledalert"\nversion = "0.1.0"\n',
            "Cargo.lock": ('version = 4\n[[package]]\nname = "fixture-dependency"\nversion = "1.0.0"\n'
                           'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
                           f'checksum = "{"a" * 64}"\n').encode(),
            "src/main.rs": b"fn main() {}\n", "src/lib.rs": b"// fixture library\n",
            "packaging/verify.py": b"# fixture verifier\n", "packaging/test_release.py": b"# fixture regression\n",
            "assets/fonts/OFL.txt": b"third-party font notice fixture\n",
        })
        for name, data in sources.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        # A structural envelope fixture only; check_artifacts separately executes real built ELF files.
        executable = b"\x7fELF\x02\x01" + b"structural fixture, not executable"
        hashes = {name: release.digest(content) for name, content in sources.items()}
        self.info = {
            "application": "LedAlert", "version": "0.1.0", "target": release.TARGET, "target_cpu": "x86-64",
            "baseline": dict(release.BASELINE), "builder": {
                "os_release": {"ID": "debian", "VERSION_ID": "12", "VERSION_CODENAME": "bookworm"},
                "glibc": "2.36", "containerfile_sha256": hashes["packaging/Containerfile"]},
            "rustc": "rustc 1.95.0 (fixture)\nhost: x86_64-unknown-linux-gnu\nrelease: 1.95.0",
            "cargo": "cargo 1.95.0 (fixture)", "glibc_required": "2.36", "direct_shared_libraries": ["libc.so.6"],
            "binary_sha256": release.digest(executable), "source_files": hashes,
            "source_manifest_sha256": release.digest(release.json_bytes(hashes)), "cargo_lock_sha256": hashes["Cargo.lock"],
            "dependency_packages": 1, "archive_epoch": 0, "first_party_license": {"status": "not_selected"},
        }
        self.notice = "licenses/crates/fixture-dependency-1.0.0/LICENSE"
        self.attribution = "licenses/crates/fixture-dependency-1.0.0/NOTICE"
        notices = {
            self.notice: b"third-party dependency notice fixture\n",
            self.attribution: b"required upstream attribution fixture\n",
            "licenses/rust/COPYRIGHT-library.html": b"Rust runtime notice fixture\n",
            "licenses/Silkscreen-OFL.txt": sources["assets/fonts/OFL.txt"],
            "licenses/Silkscreen-metadata.txt": b"font name-table attribution fixture\n",
            "THIRD-PARTY-NOTICES.txt": b"third-party notices fixture; no application license\n",
        }
        inventory = {
            "target": release.TARGET, "packages": [{"name": "fixture-dependency", "version": "1.0.0",
                "license_expression": "MIT", "registry_source": "registry+https://github.com/rust-lang/crates.io-index",
                "crate_sha256": "a" * 64, "notices": [
                    {"path": self.notice, "sha256": release.digest(notices[self.notice])},
                    {"path": self.attribution, "sha256": release.digest(notices[self.attribution])}]}],
            "rust_runtime": {"compiler": self.info["rustc"].splitlines()[0], "notices": [{
                "path": "licenses/rust/COPYRIGHT-library.html",
                "sha256": release.digest(notices["licenses/rust/COPYRIGHT-library.html"])}]},
        }
        notices["licenses/INVENTORY.json"] = release.json_bytes(inventory)
        common = {name: (data, 0o644) for name, data in notices.items()}
        self.source = {**common, **{name: (data, 0o644) for name, data in sources.items()}}
        self.binary = {**common, "bin/ledalert": (executable, 0o755)}
        self.binary.update({name: (data, 0o644) for name, data in sources.items() if name in {
            "README.md", "CHANGELOG.md", "RELEASE.md", "SECURITY.md"} or name.startswith(("assets/", "docs/"))})
        self.metadata = {
            "resolve": {"root": "application", "nodes": [
                {"id": "application", "deps": [{"pkg": "direct"}]},
                {"id": "direct", "deps": []},
            ]},
            "packages": [
                {"id": "application", "name": "ledalert", "version": "0.1.0", "source": None},
                {"id": "direct", "name": "fixture-dependency", "version": "1.0.0",
                 "source": "registry+https://github.com/rust-lang/crates.io-index"},
                {"id": "other-target", "name": "unselected-platform", "version": "1.0.0",
                 "source": "registry+https://github.com/rust-lang/crates.io-index"},
            ],
        }
        metadata = patch.object(release, "metadata", return_value=self.metadata)
        metadata.start()
        self.addCleanup(metadata.stop)
        trusted_notices = patch.object(release, "notice_inputs", return_value=(dict(notices), inventory["packages"]))
        trusted_notices.start()
        self.addCleanup(trusted_notices.stop)

    def package(self):
        for entries in (self.binary, self.source):
            entries["BUILD-INFO.json"] = (release.json_bytes(self.info), 0o644)
        for prefix, entries in zip(self.prefixes, (self.binary, self.source)):
            release.archive(self.output / (prefix + ".tar.gz"), prefix, entries, 0)
        self.checksums()

    def checksums(self):
        (self.output / "SHA256SUMS").write_text("".join(
            f"{verify.file_digest(self.output / (prefix + '.tar.gz'))}  {prefix}.tar.gz\n" for prefix in self.prefixes))

    def test_technical_inventory_reports_pending_license_but_license_gate_fails(self):
        self.package()
        info, _, _ = verify.inspect_artifacts(self.root, self.output)
        self.assertEqual(info["first_party_license"]["status"], "not_selected")
        with self.assertRaises(ValueError):
            verify.require_license(self.root)

    def test_corrupt_download_fails_checksum(self):
        self.package()
        archive = self.output / (self.prefixes[0] + ".tar.gz")
        with archive.open("ab") as file:
            file.write(b"corrupt download")
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)


    def test_entire_dependency_omission_fails_with_consistent_archive_metadata(self):
        # A transitive dependency belongs to the locked target graph, but its whole
        # inventory entry and all notices were omitted from both otherwise consistent bundles.
        lock = self.root / "Cargo.lock"
        lock.write_bytes(lock.read_bytes() + (
            '[[package]]\nname = "fixture-transitive"\nversion = "1.0.0"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
            f'checksum = "{"b" * 64}"\n'
        ).encode())
        self.source["Cargo.lock"] = (lock.read_bytes(), 0o644)
        self.info["source_files"]["Cargo.lock"] = release.digest(lock.read_bytes())
        self.info["cargo_lock_sha256"] = self.info["source_files"]["Cargo.lock"]
        self.info["source_manifest_sha256"] = release.digest(release.json_bytes(self.info["source_files"]))
        self.metadata["resolve"]["nodes"][1]["deps"].append({"pkg": "transitive"})
        self.metadata["resolve"]["nodes"].append({"id": "transitive", "deps": []})
        self.metadata["packages"].append({
            "id": "transitive", "name": "fixture-transitive", "version": "1.0.0",
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        })
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)
    def test_missing_notice_fails_even_with_valid_archive_checksums(self):
        del self.source[self.notice]
        del self.binary[self.notice]
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_changed_notice_fails_inventory_hash(self):
        self.source[self.notice] = self.binary[self.notice] = (b"altered notice", 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_omitted_attribution_with_consistent_inventory_is_rejected(self):
        inventory = verify.read_json(self.source["licenses/INVENTORY.json"][0], "fixture")
        inventory["packages"][0]["notices"] = inventory["packages"][0]["notices"][:1]
        for entries in (self.source, self.binary):
            del entries[self.attribution]
            entries["licenses/INVENTORY.json"] = (release.json_bytes(inventory), 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_substituted_terms_with_consistent_inventory_are_rejected(self):
        content = b"substituted nonempty license terms"
        inventory = verify.read_json(self.source["licenses/INVENTORY.json"][0], "fixture")
        inventory["packages"][0]["notices"][0]["sha256"] = release.digest(content)
        for entries in (self.source, self.binary):
            entries[self.notice] = (content, 0o644)
            entries["licenses/INVENTORY.json"] = (release.json_bytes(inventory), 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_changed_license_expression_with_consistent_inventory_is_rejected(self):
        inventory = verify.read_json(self.source["licenses/INVENTORY.json"][0], "fixture")
        inventory["packages"][0]["license_expression"] = "Apache-2.0"
        for entries in (self.source, self.binary):
            entries["licenses/INVENTORY.json"] = (release.json_bytes(inventory), 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_archive_version_must_match_source_package(self):
        self.info["version"] = "0.2.0"
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_source_content_must_match_recorded_hashes(self):
        self.source["src/main.rs"] = (b"different source", 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_verification_tree_must_match_released_source(self):
        self.package()
        (self.root / "src/main.rs").write_text("different checkout\n")
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_newer_abi_cannot_claim_baseline_compatibility(self):
        self.info["glibc_required"] = "2.37"
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_unlisted_hidden_archive_payload_is_rejected(self):
        self.source["src/.credentials"] = (b"not public", 0o644)
        self.package()
        with self.assertRaises(ValueError):
            verify.inspect_artifacts(self.root, self.output)

    def test_archive_special_paths_links_and_duplicates_are_rejected_before_extraction(self):
        for attack in ("traversal", "symlink", "duplicate"):
            with self.subTest(attack=attack):
                path = self.output / f"{attack}.tar.gz"
                with tarfile.open(path, "w:gz") as archive:
                    member = tarfile.TarInfo("bundle/../outside" if attack == "traversal" else "bundle/file")
                    member.mode = 0o644
                    if attack == "symlink":
                        member.type = tarfile.SYMTYPE
                        member.linkname = "../../outside"
                    else:
                        member.size = 1
                    archive.addfile(member, io.BytesIO(b"x"))
                    if attack == "duplicate":
                        archive.addfile(member, io.BytesIO(b"x"))
                with self.assertRaises(ValueError):
                    verify.read_archive(path, "bundle")
                self.assertFalse((self.output.parent / "outside").exists())


if __name__ == "__main__":
    unittest.main()
