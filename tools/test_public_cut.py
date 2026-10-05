#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tools.public_cut import (
    BuildError,
    build_cut,
    should_include,
)


class PublicCutPolicyTests(unittest.TestCase):
    def test_includes_public_source_and_excludes_internal_corpus(self):
        admitted = (
            "README.md",
            "LICENSE",
            "THIRD_PARTY_NOTICES.md",
            "Cargo.toml",
            "crates/gnosix-desktop-model/src/lib.rs",
            "xtask/src/main.rs",
            "vendor/gpui-pre-linux-0.3.5/Cargo.toml",
            "docs/status/STATUS.md",
            "docs/evidence/CURRENT.md",
        )
        for path in admitted:
            with self.subTest(path=path):
                self.assertTrue(should_include(path))

        rejected = (
            "GNOS_UX_1_0_PROGRAM_REGISTER.md",
            "evidence/runs/private.json",
            "crates/in-gnu-os-personal-profile-provision/src/lib.rs",
            "docs/publication/PUBLICATION-01-RESULT.md",
            "target/debug/gnos-ux-scene",
            ".gitleaksignore",
        )
        for path in rejected:
            with self.subTest(path=path):
                self.assertFalse(should_include(path))

    def test_required_legacy_ci_tools_are_admitted(self):
        for path in (
            "tools/stage-r13-production-caller.sh",
            "tools/test_stage_r13_production_caller.py",
            "tools/r13-authority.sha256",
            "tools/publication_lint.py",
            "tools/test_publication_lint.py",
            "tools/public_cut.py",
            "tools/test_public_cut.py",
        ):
            with self.subTest(path=path):
                self.assertTrue(should_include(path))


class PublicCutBuildTests(unittest.TestCase):
    def make_source(self, base: Path) -> tuple[Path, list[str]]:
        source = base / "source"
        files = {
            "README.md": "# gnosix\n",
            "LICENSE": "GNU GENERAL PUBLIC LICENSE\n",
            "THIRD_PARTY_NOTICES.md": "# Third-party notices\n",
            "Cargo.toml": (
                "[workspace]\n"
                "members = [\n"
                '    "crates/demo",\n'
                '    "crates/in-gnu-os-personal-profile-provision",\n'
                "]\n"
            ),
            "docs/status/STATUS.md": "# status\n",
            "docs/evidence/CURRENT.md": "# evidence\n",
            "crates/demo/src/lib.rs": "pub fn demo() {}\n",
            "evidence/runs/raw.json": "{}\n",
            "GNOS_UX_1_0_PROGRAM_REGISTER.md": "private\n",
        }
        for rel, content in files.items():
            path = source / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        return source, sorted(files)

    def test_build_cut_is_deterministic_and_excludes_raw_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source, tracked = self.make_source(base)
            first = base / "cut-a"
            second = base / "cut-b"
            build_cut(source, first, tracked, "source-sha", "publication-sha")
            build_cut(source, second, tracked, "source-sha", "publication-sha")
            manifest_a = (first / "PUBLICATION-MANIFEST.json").read_bytes()
            manifest_b = (second / "PUBLICATION-MANIFEST.json").read_bytes()
            self.assertEqual(manifest_a, manifest_b)
            self.assertFalse((first / "evidence").exists())
            self.assertFalse((first / "GNOS_UX_1_0_PROGRAM_REGISTER.md").exists())
            self.assertTrue((first / "crates/demo/src/lib.rs").is_file())

    def test_manifest_records_file_hashes_without_timestamps(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source, tracked = self.make_source(base)
            destination = base / "cut"
            build_cut(source, destination, tracked, "source-sha", "publication-sha")
            manifest = json.loads((destination / "PUBLICATION-MANIFEST.json").read_text())
            self.assertEqual(manifest["schema"], "gnosix-public-cut-v1")
            self.assertEqual(manifest["source_commit"], "source-sha")
            self.assertEqual(manifest["publication_commit"], "publication-sha")
            self.assertNotIn("generated_at", manifest)
            self.assertGreater(manifest["file_count"], 0)
            for entry in manifest["files"]:
                self.assertRegex(entry["sha256"], r"^[0-9a-f]{64}$")
                self.assertGreaterEqual(entry["bytes"], 0)

    def test_public_cut_requires_license_and_third_party_notices(self):
        for required in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
            with self.subTest(required=required), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                source, tracked = self.make_source(base)
                (source / required).unlink()
                with self.assertRaisesRegex(BuildError, f"required legal file missing: {required}"):
                    build_cut(source, base / "cut", tracked, "source-sha", "publication-sha")

    def test_private_workspace_member_is_removed_from_public_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source, tracked = self.make_source(base)
            destination = base / "cut"
            build_cut(source, destination, tracked, "source-sha", "publication-sha")
            cargo = (destination / "Cargo.toml").read_text()
            self.assertIn('"crates/demo"', cargo)
            self.assertNotIn("in-gnu-os-personal-profile-provision", cargo)

    def test_private_marker_aborts_cut(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source, tracked = self.make_source(base)
            leaking = source / "crates/demo/src/lib.rs"
            private_path = "/home/" + "tension_atoi/private"
            leaking.write_text(f'const P: &str = "{private_path}";\n')
            with self.assertRaisesRegex(BuildError, "private marker"):
                build_cut(source, base / "cut", tracked, "source-sha", "publication-sha")

    def test_refuses_existing_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source, tracked = self.make_source(base)
            destination = base / "cut"
            destination.mkdir()
            with self.assertRaisesRegex(BuildError, "destination already exists"):
                build_cut(source, destination, tracked, "source-sha", "publication-sha")


if __name__ == "__main__":
    unittest.main(verbosity=2)
