#!/usr/bin/env python3
"""Hermetic negative controls for the installed historical-engine guard."""
import hashlib
from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
GUARD = ROOT / "tools/legacy-in-gnu-os-preflight.sh"
DROP_IN = ROOT / "systemd/user/in.gnu.os.service.d/90-legacy-guard.conf"
PRODUCTION_SHA = "44cb6461c9f025923098485bd7a5a7550e0da9018b6c63de5778c1482c66e7e7"


class LegacyGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="legacy-guard-")
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.home = base / "home"
        self.bin = self.home / ".local/bin"
        self.bin.mkdir(parents=True)
        self.engine = self.bin / "in.gnu.os"
        self.engine.write_bytes(b"test engine payload\n")
        self.engine.chmod(0o755)
        self.guard = base / "preflight"
        fixture_hash = hashlib.sha256(self.engine.read_bytes()).hexdigest()
        source = GUARD.read_text()
        self.assertEqual(source.count(PRODUCTION_SHA), 1)
        self.guard.write_text(source.replace(PRODUCTION_SHA, fixture_hash))
        self.guard.chmod(0o755)
        self.path = base / "path"
        self.path.mkdir()
        self.env = dict(os.environ, HOME=str(self.home),
                        PATH=str(self.path) + os.pathsep + os.environ["PATH"])

    def run_gate(self):
        return subprocess.run([str(self.guard)], env=self.env, text=True,
                              capture_output=True, timeout=5)

    def test_known_binary_without_tetra_allowed_but_never_claims_sandbox(self):
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Tetragon enforcement NOT active", result.stdout)

    def test_missing_binary_denied(self):
        self.engine.unlink()
        result = self.run_gate()
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing", result.stderr)

    def test_changed_binary_denied(self):
        self.engine.write_bytes(b"different executable\n")
        result = self.run_gate()
        self.assertEqual(result.returncode, 1)
        self.assertIn("re-audit", result.stderr)

    def test_tetra_presence_denied_even_for_known_binary(self):
        tetra = self.path / "tetra"
        tetra.write_text("#!/bin/sh\nexit 0\n")
        tetra.chmod(0o755)
        result = self.run_gate()
        self.assertEqual(result.returncode, 1)
        self.assertIn("unscoped policy", result.stderr)

    def test_dropin_is_narrow(self):
        content = DROP_IN.read_text()
        self.assertIn("After=graphical-session.target", content)
        self.assertIn("ExecCondition=%h/.local/libexec/gnosix/legacy-in-gnu-os-preflight", content)
        self.assertNotIn("ExecStart=", content)
        self.assertNotIn("ConditionEnvironment=", content)

    def test_real_production_pin_is_not_a_fixture(self):
        self.assertIn(PRODUCTION_SHA, GUARD.read_text())
        self.assertEqual(len(PRODUCTION_SHA), 64)


if __name__ == "__main__":
    unittest.main()
