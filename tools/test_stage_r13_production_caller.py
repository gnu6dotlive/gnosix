#!/usr/bin/env python3
"""Hermetic staging contract: real script, isolated files, recorded Cargo calls.

The fixture pin is the digest of fixture executable bytes. Production has no
pin override; a separate assertion binds both production consumers to the
ratified digest. Real artifact qualification remains a separate host gate.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
CANONICAL = "c5c0b227b456caa275e12d6c911983d2130680d8fb15e63a0ef052c01daecfd9"
AUTHORITY = "usr/libexec/gnosix/in-gnu-os-activationd"
CALLER = "usr/libexec/gnosix/gnos-ux-scene"
MANIFEST = "usr/share/gnosix/manifests/r13-production-caller.sha256"
FAKE_CARGO = '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["STAGING_CALLS"], "a") as log:
    log.write(json.dumps(args) + "\\n")
target = pathlib.Path(os.environ["STAGING_TARGET"])
if args[0] == "metadata":
    print(json.dumps({"target_directory": str(target)}))
elif args[0] == "build":
    binary = args[args.index("--bin") + 1]
    if binary == "gnos-ux-scene" and os.environ.get("STAGING_FAIL"):
        print("PASS")
        sys.exit(19)
    if binary == "gnos-ux-scene" and os.environ.get("STAGING_MUTATE"):
        pathlib.Path(os.environ["STAGING_MUTATE"]).write_bytes(b"changed during build")
    output = target / "release" / binary
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(("built:" + binary).encode())
    output.chmod(0o755)
else:
    sys.exit(20)
'''


class StagingContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="gnosix-stage-test-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.repo = self.base / "repo"
        (self.repo / "tools").mkdir(parents=True)
        shutil.copyfile(ROOT / "tools/stage-r13-production-caller.sh",
                        self.repo / "tools/stage-r13-production-caller.sh")
        for name in ("systemd/user/in-gnu-os-activation.service",
                     "systemd/user/gnos-ux-scene.service",
                     "dbus-1/services/in.gnu.os.Activation1.service"):
            destination = self.repo / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)
        self.artifact = self.base / "frozen-authority"
        self.artifact.write_bytes(b"#!/bin/sh\nexit 0\n")
        self.artifact.chmod(0o755)
        self.frozen_bytes = self.artifact.read_bytes()
        (self.repo / "tools/r13-authority.sha256").write_text(
            hashlib.sha256(self.frozen_bytes).hexdigest() + "\n")
        self.bin = self.base / "bin"
        self.bin.mkdir()
        (self.bin / "cargo").write_text(FAKE_CARGO)
        (self.bin / "cargo").chmod(0o755)
        self.stage = self.base / "stage"
        self.calls = self.base / "calls.jsonl"
        self.target = self.base / "target"
        stale = self.target / "release/in-gnu-os-activationd"
        stale.parent.mkdir(parents=True)
        stale.write_bytes(b"poisoned stale authority")
        self.env = dict(os.environ)
        self.env.pop("GNOSIX_R13_AUTHORITY_ARTIFACT", None)
        self.env.update(PATH=str(self.bin) + os.pathsep + self.env["PATH"],
                        DESTDIR=str(self.stage), STAGING_CALLS=str(self.calls),
                        STAGING_TARGET=str(self.target))

    def run_stage(self):
        return subprocess.run(["bash", str(self.repo / "tools/stage-r13-production-caller.sh")],
                              env=self.env, capture_output=True, text=True, timeout=30)

    def recorded(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def expected_build(self, package, feature, binary):
        return ["build", "--manifest-path", str(self.repo / "Cargo.toml"),
                "-p", package, "--release", "--features", feature, "--bin", binary]

    def assert_manifest(self):
        manifest = self.stage / MANIFEST
        self.assertEqual(len(manifest.read_text().splitlines()), 5)
        result = subprocess.run(["sha256sum", "-c", str(manifest)], cwd=self.stage,
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_production_pin_and_consumers(self):
        self.assertEqual((ROOT / "tools/r13-authority.sha256").read_text().strip(), CANONICAL)
        self.assertIn('include_str!("../../tools/r13-authority.sha256").trim()',
                      (ROOT / "xtask/src/runner.rs").read_text())
        self.assertIn('$repo_root/tools/r13-authority.sha256',
                      (ROOT / "tools/stage-r13-production-caller.sh").read_text())

    def test_frozen_mode_builds_caller_and_preserves_exact_authority(self):
        self.env["GNOSIX_R13_AUTHORITY_ARTIFACT"] = str(self.artifact)
        result = self.run_stage()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.recorded(), [
            self.expected_build("gnos-ux-gpui", "r13-production-client", "gnos-ux-scene"),
            ["metadata", "--manifest-path", str(self.repo / "Cargo.toml"),
             "--no-deps", "--format-version", "1"]])
        self.assertEqual((self.stage / AUTHORITY).read_bytes(), self.frozen_bytes)
        self.assertEqual((self.stage / CALLER).read_bytes(), b"built:gnos-ux-scene")
        self.assert_manifest()

    def test_default_mode_retains_build_both(self):
        result = self.run_stage()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.recorded()[:2], [
            self.expected_build("gnosix-hyprland", "r13-production-dbus", "in-gnu-os-activationd"),
            self.expected_build("gnos-ux-gpui", "r13-production-client", "gnos-ux-scene")])
        self.assertEqual((self.stage / AUTHORITY).read_bytes(), b"built:in-gnu-os-activationd")
        self.assert_manifest()

    def test_invalid_artifacts_fail_before_build(self):
        wrong = self.base / "wrong"
        wrong.write_bytes(b"wrong bytes")
        wrong.chmod(0o755)
        nonexec = self.base / "nonexec"
        nonexec.write_bytes(self.frozen_bytes)
        nonexec.chmod(0o644)
        for value in ("", "relative", str(self.base / "missing"), str(self.base),
                      str(wrong), str(nonexec)):
            with self.subTest(artifact=value):
                self.env["GNOSIX_R13_AUTHORITY_ARTIFACT"] = value
                result = self.run_stage()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.calls.exists())
                self.assertFalse((self.stage / MANIFEST).exists())

    def test_invalid_pin_fails_before_build(self):
        self.env["GNOSIX_R13_AUTHORITY_ARTIFACT"] = str(self.artifact)
        (self.repo / "tools/r13-authority.sha256").write_text("invalid\n")
        self.assertNotEqual(self.run_stage().returncode, 0)
        self.assertFalse(self.calls.exists())

    def test_snapshot_survives_supplied_path_change_during_build(self):
        self.env["GNOSIX_R13_AUTHORITY_ARTIFACT"] = str(self.artifact)
        self.env["STAGING_MUTATE"] = str(self.artifact)
        result = self.run_stage()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(self.artifact.read_bytes(), self.frozen_bytes)
        self.assertEqual((self.stage / AUTHORITY).read_bytes(), self.frozen_bytes)
        self.assert_manifest()

    def test_build_failure_cannot_be_reported_as_pass(self):
        self.env["GNOSIX_R13_AUTHORITY_ARTIFACT"] = str(self.artifact)
        self.env["STAGING_FAIL"] = "1"
        result = self.run_stage()
        self.assertEqual(result.returncode, 19)
        self.assertIn("PASS", result.stdout)
        self.assertFalse((self.stage / MANIFEST).exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
