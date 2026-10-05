import tempfile
import unittest
from pathlib import Path

from tools.publication_lint import lint_repository


REQUIRED = (
    "README.md",
    "ARCHITECTURE.md",
    "AUTHORITY.md",
    "NAMING.md",
    "AGENTS.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "docs/status/STATUS.md",
    "docs/status/ROADMAP.md",
    "docs/evidence/README.md",
)


class PublicationLintTests(unittest.TestCase):
    def make_surface(self, root: Path) -> None:
        for rel in REQUIRED:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# gnosix\n", encoding="utf-8")
        (root / "README.md").write_text(
            "# gnosix\n\nExperimental / pre-release\n\n"
            "## What gnosix is\n\n## Current status\n\n"
            "## Architecture\n\n## Build and inspect\n\n## Evidence\n\n## License\n",
            encoding="utf-8",
        )

    def test_rejects_stale_authored_name_but_allows_machine_identifier(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "AUTHORITY.md").write_text(
                "# Authority\nGNOSIX owns this layer.\n"
                "`GNOSIX_DESKTOP_SEMANTIC_RECEIPT` is a machine marker.\n",
                encoding="utf-8",
            )
            findings = lint_repository(root)
            self.assertTrue(any(f.rule == "canonical-name" for f in findings))
            self.assertFalse(any("SEMANTIC_RECEIPT" in f.message for f in findings))

    def test_allows_explicit_historical_name_context(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "NAMING.md").write_text(
                "# Naming\nHistorical name: gnosis.OS is superseded.\n",
                encoding="utf-8",
            )
            findings = lint_repository(root)
            self.assertFalse(any(f.rule == "canonical-name" for f in findings))

    def test_reports_missing_legal_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            for rel in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
                path = root / rel
                if path.exists():
                    path.unlink()
            findings = lint_repository(root)
            messages = {f.message for f in findings if f.rule == "required-file"}
            self.assertTrue(any("LICENSE" in message for message in messages))
            self.assertTrue(any("THIRD_PARTY_NOTICES.md" in message for message in messages))

    def test_reports_missing_required_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "SECURITY.md").unlink()
            findings = lint_repository(root)
            self.assertTrue(any(f.rule == "required-file" and "SECURITY.md" in f.message for f in findings))

    def test_reports_broken_relative_markdown_link(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "README.md").write_text(
                (root / "README.md").read_text(encoding="utf-8")
                + "\n[missing](docs/does-not-exist.md)\n",
                encoding="utf-8",
            )
            findings = lint_repository(root)
            self.assertTrue(any(f.rule == "relative-link" for f in findings))

    def test_rejects_unqualified_stable_release_claim(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "README.md").write_text(
                (root / "README.md").read_text(encoding="utf-8")
                + "\ngnosix is production-ready.\n",
                encoding="utf-8",
            )
            findings = lint_repository(root)
            self.assertTrue(any(f.rule == "release-claim" for f in findings))

    def test_requires_public_readme_sections(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.make_surface(root)
            (root / "README.md").write_text("# gnosix\n\nExperimental / pre-release\n", encoding="utf-8")
            findings = lint_repository(root)
            self.assertTrue(any(f.rule == "required-section" for f in findings))


if __name__ == "__main__":
    unittest.main()
