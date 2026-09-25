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

    def test_verification_sources_are_exported_but_private_handoff_is_not(self):
        for name in ("examples/windows_probe.rs", "tools/windows-verify.ps1", "docs/windows-verification.md",
                     ".windows-handoff/private.json"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n")
        exported = release.source_files()
        self.assertTrue({"examples/windows_probe.rs", "tools/windows-verify.ps1",
                         "docs/windows-verification.md"}.issubset(exported))
        self.assertNotIn(".windows-handoff/private.json", exported)

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

    def test_windows_checkout_newlines_preserve_pinned_notice_integrity(self):
        canonical = b"Copyright notice fixture\nPermission notice fixture\n"
        notice = self.root / "packaging/licenses/upstream/LICENSE"
        notice.parent.mkdir(parents=True)
        entry = {"file": "upstream/LICENSE", "sha256": release.digest(canonical), "source": "https://example.invalid/LICENSE"}
        (notice.parent.parent / "index.json").write_bytes(release.json_bytes({"packages": {"fixture-1.0.0": [entry]}}))
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "fixture"\nversion = "1.0.0"\nchecksum = "' + "a" * 64 + '"\n')
        crate = self.root / "crate"
        crate.mkdir()
        (crate / "Cargo.toml").write_text('[package]\nname = "fixture"\nversion = "1.0.0"\n')
        metadata = {
            "resolve": {"root": "fixture", "nodes": [{"id": "fixture", "deps": []}]},
            "packages": [{"id": "fixture", "name": "fixture", "version": "1.0.0", "license": "MIT",
                          "source": "registry+fixture", "manifest_path": str(crate / "Cargo.toml")}],
        }
        for name in ("assets/fonts/OFL.txt", "assets/fonts/Silkscreen-Bold.ttf",
                     "docs/site/assets/fonts/saira-OFL.txt", "docs/site/assets/fonts/saira.woff2"):
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((verify.ROOT / name).read_bytes())
        with patch.object(release, "rust_notices", return_value=({}, {"notices": []})):
            for content in (canonical, canonical.replace(b"\n", b"\r\n")):
                with self.subTest(content=content):
                    notice.write_bytes(content)
                    payload, inventory = release.notice_inputs(metadata, self.root)
                    self.assertEqual(payload["licenses/upstream/upstream/LICENSE"], canonical)
                    self.assertEqual(inventory[0]["notices"][0]["sha256"], release.digest(canonical))
            notice.write_bytes(canonical.replace(b"Permission", b"Modified"))
            with self.assertRaises(ValueError):
                release.notice_inputs(metadata, self.root)


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
        self.hashes = {name: "a" * 64 for name in publication.artifact_names("0.2.0")}
        self.receipt = {"version": "0.2.0", "commit": self.commit, "prerelease": False,
                        "windows_signing": "certificate-store", "sha256": self.hashes}
        self.record = {
            "body": f"<!-- ledalert-release:{json.dumps(self.receipt)} -->",
            "tag_name": "v0.2.0", "target_commitish": self.commit, "draft": False, "prerelease": False,
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

    def test_linux_only_receipt_cannot_authorize_cross_platform_publication(self):
        del self.receipt["sha256"]["ledalert-0.2.0-windows-x86_64.msix"]
        self.record["body"] = f"<!-- ledalert-release:{json.dumps(self.receipt)} -->"
        with self.assertRaisesRegex(ValueError, "artifact hashes"):
            publication.preflight("0.2.0", self.commit)

    def test_hidden_draft_is_recovered_from_release_list(self):
        self.registry_version.return_value = None
        draft = {**self.record, "draft": True, "assets": self.record["assets"][:-1]}
        self.github.side_effect = [None, [draft]]
        self.assertTrue(publication.preflight("0.2.0", self.commit))

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
            names = publication.artifact_names("0.2.0")
            crate = output / names[3]
            crate.write_bytes(b"local crate fixture")
            info = {"version": "0.2.0", "source_commit": None, "crate_sha256": verify.file_digest(crate)}
            for name, prefix in zip(names[:2], ("ledalert-0.2.0-linux-x86_64", "ledalert-0.2.0-source")):
                release.archive(output / name, prefix, {"BUILD-INFO.json": (release.json_bytes(info), 0o644)}, 0)
            for name in publication.windows.names("0.2.0"):
                (output / name).write_bytes(b"unused Windows envelope fixture")
            (output / "SHA256SUMS").write_text("".join(
                f"{verify.file_digest(output / name)}  {name}\n" for name in names if name != "SHA256SUMS"))
            with self.assertRaisesRegex(ValueError, "Built artifact identity"):
                publication.local_receipt(output, "0.2.0", self.commit)


class ArtifactRecoveryTests(unittest.TestCase):
    def test_existing_original_bundle_is_reused_for_stable_and_prerelease(self):
        for kind in ("windows", "publication"):
            for version in ("0.3.0", "0.3.0-alpha"):
                artifact = {"name": f"{kind}-release-123", "expired": False}
                with self.subTest(kind=kind, version=version), \
                        patch.object(publication, "github", return_value={"total_count": 1, "artifacts": [artifact]}):
                    self.assertTrue(publication.artifact_recovery(version, "123", kind))

    def test_first_publication_can_build_without_existing_artifact(self):
        with patch.object(publication, "github", return_value={"total_count": 0, "artifacts": []}), \
                patch.object(publication, "release_record", return_value=None):
            self.assertFalse(publication.artifact_recovery("0.3.0-alpha", "123", "publication"))

    def test_existing_publication_cannot_be_rebuilt_after_artifact_loss(self):
        for draft in (True, False):
            with self.subTest(draft=draft), \
                    patch.object(publication, "github", return_value={"total_count": 0, "artifacts": []}), \
                    patch.object(publication, "release_record", return_value={"draft": draft}):
                with self.assertRaises(ValueError):
                    publication.artifact_recovery("0.3.0-alpha", "123", "publication")

    def test_expired_ambiguous_or_incomplete_artifact_listing_is_rejected(self):
        artifact = {"name": "publication-release-123", "expired": False}
        for result in (
            {"total_count": 1, "artifacts": [{**artifact, "expired": True}]},
            {"total_count": 2, "artifacts": [artifact, artifact]},
            {"total_count": 2, "artifacts": [artifact]},
            {"total_count": 1, "artifacts": [{**artifact, "name": "publication-release-456"}]},
        ):
            with self.subTest(result=result), patch.object(publication, "github", return_value=result):
                with self.assertRaises(ValueError):
                    publication.artifact_recovery("0.3.0-alpha", "123", "publication")

    def test_older_hidden_draft_still_blocks_rebuilding_missing_artifact(self):
        responses = [
            {"total_count": 0, "artifacts": []},
            None,
            [{"tag_name": f"v1.0.{index}"} for index in range(100)],
            [{"tag_name": "v0.3.0-alpha", "draft": True}],
        ]
        with patch.object(publication, "github", side_effect=responses), self.assertRaises(ValueError):
            publication.artifact_recovery("0.3.0-alpha", "123", "publication")


class PrereleasePublicationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-prerelease-")
        self.addCleanup(temporary.cleanup)
        self.output = Path(temporary.name)
        self.version = "0.3.0-alpha"
        self.commit = "1" * 40
        for name in publication.artifact_names(self.version):
            (self.output / name).write_bytes(name.encode())
        self.hashes = {name: verify.file_digest(self.output / name) for name in publication.artifact_names(self.version)}
        self.value = {"version": self.version, "commit": self.commit, "prerelease": True,
                      "windows_signing": "unsigned", "sha256": self.hashes}
        self.record = {"id": 123, "body": f"<!-- ledalert-release:{json.dumps(self.value)} -->",
                       "tag_name": "v" + self.version, "target_commitish": self.commit,
                       "draft": False, "prerelease": True, "assets": self.assets()}
        self.uploads = {}
        for name, options in (
            ("registry_version", {"side_effect": AssertionError("Prereleases must not access crates.io")}),
            ("tag_commit", {"return_value": self.commit}),
            ("release_notes", {"return_value": "## 0.3.0-alpha\n\nAlpha release."}),
            ("release_record", {"side_effect": lambda version: self.record}),
        ):
            patched = patch.object(publication, name, **options)
            patched.start()
            self.addCleanup(patched.stop)

    def assets(self):
        return [{"name": name, "state": "uploaded", "digest": "sha256:" + checksum}
                for name, checksum in self.hashes.items()]

    def remote(self, path, *, payload=None, method="GET", **kwargs):
        if path == "/releases" and method == "POST":
            self.record = {**payload, "id": 123, "assets": []}
        elif path == "/releases/123" and method == "PATCH":
            self.record.update(payload)
        elif path != "/releases/123" or method != "GET":
            raise AssertionError(f"Unexpected GitHub operation: {method} {path}")
        return self.record

    def upload(self, url, *, data, method, **kwargs):
        self.assertEqual(method, "POST")
        name = url.split("?name=")[1]
        self.assertNotIn(name, self.uploads)
        self.uploads[name] = data
        self.record["assets"].append({"name": name, "state": "uploaded", "digest": "sha256:" + release.digest(data)})

    def publish(self):
        with patch.object(publication, "local_receipt", return_value=self.value), \
                patch.object(publication, "github", side_effect=self.remote), \
                patch.object(publication, "request", side_effect=self.upload):
            publication.publish_github(self.output, self.version, self.commit)

    def test_completed_prerelease_needs_no_registry_entry_on_original_or_later_push(self):
        self.assertFalse(publication.preflight(self.version, self.commit))
        self.assertFalse(publication.preflight(self.version, "2" * 40))

    def test_partial_prerelease_recovers_only_from_original_commit(self):
        self.record.update(draft=True, assets=self.record["assets"][:1])
        with self.assertRaises(ValueError):
            publication.preflight(self.version, "2" * 40)
        self.assertTrue(publication.preflight(self.version, self.commit))

    def test_receipt_requires_consistent_boolean_prerelease_status(self):
        for field in ("remote", "receipt"):
            for wrong in (False, None, 1):
                value = {**self.value}
                record = {**self.record}
                if field == "remote":
                    record["prerelease"] = wrong
                else:
                    value["prerelease"] = wrong
                    record["body"] = f"<!-- ledalert-release:{json.dumps(value)} -->"
                with self.subTest(field=field, wrong=wrong), self.assertRaises(ValueError):
                    publication.receipt(record, self.version)

    def test_unsigned_alpha_publishes_all_downloads_as_nonlatest_prerelease(self):
        self.record = None
        self.publish()
        self.assertFalse(self.record["draft"])
        self.assertIs(self.record["prerelease"], True)
        self.assertEqual(self.record["make_latest"], "false")
        self.assertEqual({name: release.digest(data) for name, data in self.uploads.items()}, self.hashes)
        self.assertIn("## 0.3.0-alpha", self.record["body"])
        self.assertIn("**unsigned and for development only**", self.record["body"])
        self.assertIn("not a crates.io publication", self.record["body"])

    def test_partial_upload_recovers_missing_original_bytes_without_reupload(self):
        existing = self.record["assets"][0]
        self.record.update(draft=True, assets=[existing])
        self.publish()
        self.assertFalse(self.record["draft"])
        self.assertNotIn(existing["name"], self.uploads)
        self.assertEqual({name: release.digest(data) for name, data in self.uploads.items()},
                         {name: checksum for name, checksum in self.hashes.items() if name != existing["name"]})

    def test_changed_recovery_bytes_are_rejected_before_upload(self):
        self.record.update(draft=True, assets=[])
        self.value = {**self.value, "sha256": {**self.hashes, next(iter(self.hashes)): "b" * 64}}
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.uploads, {})
        self.assertTrue(self.record["draft"])

    def test_completed_prerelease_is_not_uploaded_again(self):
        self.publish()
        self.assertEqual(self.uploads, {})

    def test_crate_commands_reject_prerelease_before_build_or_network(self):
        (self.output / "Cargo.toml").write_text('[package]\nversion = "0.3.0-alpha"\n')
        with patch.object(release, "ROOT", self.output):
            with self.assertRaisesRegex(ValueError, "GitHub-only"):
                publication.publish_crate(self.output, self.version, self.commit, "not-a-real-token")
            with self.assertRaisesRegex(ValueError, "GitHub-only"):
                publication.publish_crate_native(self.output, self.commit, "not-a-real-token")


class ReleaseNotesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ledalert-release-notes-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.changelog = root / "CHANGELOG.md"
        patched = patch.object(release, "ROOT", root)
        patched.start()
        self.addCleanup(patched.stop)

    def test_notes_include_only_exact_current_version_and_keep_unicode(self):
        section = "## 0.3.0 — Windows 11\n\n- Native Windows support.\n\n### Limitations\n\n- Signed MSIX required."
        self.changelog.write_text(
            "# Changelog\n\n## 0.3.0-preview\n\n- Not this release.\n\n" + section +
            "\n\n## 0.2.0 — Linux\n\n- Older release.\n", encoding="utf-8")
        self.assertEqual(publication.release_notes("0.3.0"), section)

    def test_missing_duplicate_or_empty_notes_block_publication(self):
        for content in (
            "# Changelog\n\n## 0.3.1\n\n- Different version.\n",
            "## 0.3.0\n\n- First.\n\n## 0.3.0 — duplicate\n\n- Second.\n",
            "## 0.3.0\n\n## 0.2.0\n\n- Old.\n",
        ):
            with self.subTest(content=content):
                self.changelog.write_text(content, encoding="utf-8")
                with self.assertRaises(ValueError):
                    publication.release_notes("0.3.0")


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
