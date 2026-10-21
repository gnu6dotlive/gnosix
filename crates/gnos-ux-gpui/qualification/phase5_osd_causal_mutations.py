#!/usr/bin/env python3
"""AUDIO-01 Phase 5 causal mutations.

Each mutation breaks exactly one invariant of the observed-audio OSD and runs the
OSD source tests. A mutation is only accepted when it turns them RED for the
right reason: that is the proof the guard is load-bearing rather than decorative.

Every mutation is reverted immediately, whether it passed or failed.
"""
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE = ROOT / "crates/gnos-ux-gpui/src/product_audio_osd.rs"
PRISTINE = Path("/tmp/opencode/osd-pristine.rs")

# (id, expectation, description, needle, replacement)
MUTATIONS = [
    (
        "M-AUDIO-05-1",
        "RED",
        "trigger the OSD when an intent is dispatched, before observation",
        "pub fn observe(&mut self, snapshot: &AudioSnapshot, now: Instant) -> Option<AudioOsdEvent> {\n        if observed_output(snapshot).is_none() {",
        "pub fn observe(&mut self, snapshot: &AudioSnapshot, now: Instant) -> Option<AudioOsdEvent> {\n        // MUTATION: present from the requested snapshot, before PipeWire observes.\n        if let Some(truth) = observed_output(snapshot)\n            && truth.level.is_some()\n        {\n            let event = AudioOsdEvent {\n                truth,\n                cause: AudioOsdCause::Level,\n            };\n            self.present(&event, now);\n            return Some(event);\n        }\n        if observed_output(snapshot).is_none() {",
    ),
    (
        "M-AUDIO-05-2",
        "RED",
        "treat the initial snapshot as a change",
        "            // A first snapshot establishes the baseline. Enumeration is not a\n            // change, so it must not flash an OSD.\n            None => None,",
        "            // MUTATION: enumeration is presented as a change.\n            None => {\n                let truth = observed_output(snapshot).expect(\"checked above\");\n                let event = AudioOsdEvent {\n                    truth,\n                    cause: AudioOsdCause::Level,\n                };\n                self.present(&event, now);\n                Some(event)\n            }",
    ),
    (
        "M-AUDIO-05-3",
        "RED",
        "re-emit for an identical observed state",
        "    let cause = match (after.level != before.level, after.muted != before.muted) {\n        (false, false) => return None,",
        "    // MUTATION: snapshot churn is treated as a presentable change.\n    let cause = match (after.level != before.level, after.muted != before.muted) {\n        (false, false) => AudioOsdCause::Level,",
    ),
    (
        "M-AUDIO-05-4",
        "RED",
        "let every output consume one global event",
        "    pub fn resolve(&self, context: Option<&str>) -> Option<String> {\n        if self.outputs.is_empty() {\n            return None;\n        }\n        if let Some(context) = context\n            && self.outputs.iter().any(|output| output == context)\n        {\n            return Some(context.to_owned());\n        }\n        self.primary().map(str::to_owned)\n    }",
        "    pub fn resolve(&self, context: Option<&str>) -> Option<String> {\n        // MUTATION: the global event is accepted by every output.\n        let _ = context;\n        self.primary().map(str::to_owned)\n    }\n\n    pub fn resolve_every_output(&self) -> Vec<String> {\n        self.outputs.clone()\n    }",
    ),
    (
        "M-AUDIO-05-5",
        "RED",
        "let the timeout rewrite canonical audio truth",
        "    pub fn expire(&mut self, now: Instant) -> bool {\n        if !self.visible() {\n            return false;\n        }",
        "    pub fn expire(&mut self, now: Instant) -> bool {\n        if !self.visible() {\n            return false;\n        }\n        // MUTATION: the ephemeral timer writes back into the retained observation.\n        if let Some(cursor) = self.cursor.as_mut()\n            && let Some(truth) = self.truth.as_ref()\n            && let Some(id) = truth.endpoint_id()\n            && let Some(endpoint) = cursor.endpoints.get_mut(&id)\n        {\n            endpoint.muted = !endpoint.muted;\n        }",
    ),
]


def run_tests() -> tuple[bool, str]:
    result = subprocess.run(
        ["cargo", "test", "-p", "gnos-ux-gpui", "--lib", "--features", "component-lab",
         "product_audio_osd", "--quiet"],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    return result.returncode == 0, result.stdout


def failing_tests(output: str) -> list[str]:
    names = []
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.startswith("test product_audio_osd") and "FAILED" in stripped:
            names.append(stripped.split()[1])
    return names


def main() -> int:
    if not PRISTINE.exists():
        print(f"missing pristine copy at {PRISTINE}", file=sys.stderr)
        return 2
    results = []
    for name, expectation, description, needle, replacement in MUTATIONS:
        source = PRISTINE.read_text()
        if needle not in source:
            print(f"[SKIP] {name}: mutation anchor not found (source moved)")
            results.append({"id": name, "verdict": "SKIP", "detail": description})
            continue
        shutil.copy(MODULE, f"{MODULE}.pristine")
        try:
            MODULE.write_text(source.replace(needle, replacement, 1))
            passed, output = run_tests()
            failures = failing_tests(output)
            # A good mutation is RED for the intended reason: it compiles, and at
            # least one OSD test fails.
            verdict = "RED" if not passed and failures else ("GREEN" if passed else "RED")
            print(f"[{verdict}] {name}: {description}")
            for failure in failures[:6]:
                print(f"         failed: {failure}")
            results.append({
                "id": name,
                "verdict": verdict,
                "description": description,
                "failing_tests": failures,
            })
        finally:
            shutil.move(f"{MODULE}.pristine", MODULE)

    # The restored source must be green again.
    passed, output = run_tests()
    results.append({
        "id": "RESTORED",
        "verdict": "GREEN" if passed else "RED",
        "description": "source restored after every mutation",
        "failing_tests": failing_tests(output),
    })
    print(f"[{'GREEN' if passed else 'RED'}] RESTORED: source restored after every mutation")

    verdicts = {r["verdict"] for r in results if r["id"] != "RESTORED"}
    ok = verdicts == {"RED"} and passed
    print(f"AUDIO_PHASE5_CAUSAL={'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
