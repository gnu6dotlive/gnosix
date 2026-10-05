#!/usr/bin/env python3
"""Lint the curated, authored public surface of the gnosix repository."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


PUBLIC_SURFACE = (
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

README_SECTIONS = (
    "## What gnosix is",
    "## Current status",
    "## Architecture",
    "## Build and inspect",
    "## Evidence",
    "## License",
)

FORBIDDEN_AUTHORED_NAMES = ("GNOSIX", "GNOS.UX", "gnosis.OS", "Quickshell")
HISTORICAL_MARKERS = ("historical", "superseded", "former", "legacy")


@dataclass(frozen=True)
class Finding:
    path: str
    line: int
    rule: str
    message: str

    def render(self) -> str:
        location = f"{self.path}:{self.line}" if self.line else self.path
        return f"{location}: {self.rule}: {self.message}"


def _strip_inline_code(line: str) -> str:
    return re.sub(r"`[^`]*`", "", line)


def _is_historical_context(line: str) -> bool:
    lower = line.lower()
    return any(marker in lower for marker in HISTORICAL_MARKERS)


def _is_negated_release_line(line: str) -> bool:
    lower = line.lower()
    return any(token in lower for token in ("not ", "no stable", "is not", "isn't", "without"))


def _relative_links(line: str) -> list[str]:
    links = []
    for target in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", line):
        target = target.strip().strip("<>")
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        links.append(target)
    return links


def lint_repository(root: Path) -> list[Finding]:
    root = root.resolve()
    findings: list[Finding] = []

    for rel in PUBLIC_SURFACE:
        path = root / rel
        if not path.is_file():
            findings.append(Finding(rel, 0, "required-file", f"missing required public file {rel}"))

    readme = root / "README.md"
    if readme.is_file():
        text = readme.read_text(encoding="utf-8")
        if "Experimental / pre-release" not in text:
            findings.append(Finding("README.md", 0, "release-status", "missing Experimental / pre-release marker"))
        for heading in README_SECTIONS:
            if heading not in text:
                findings.append(Finding("README.md", 0, "required-section", f"missing section {heading!r}"))

    for rel in PUBLIC_SURFACE:
        path = root / rel
        if not path.is_file():
            continue
        in_fence = False
        for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if raw.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            authored = _strip_inline_code(raw)
            if not _is_historical_context(authored):
                for stale in FORBIDDEN_AUTHORED_NAMES:
                    if stale in authored:
                        findings.append(Finding(rel, number, "canonical-name", f"authored copy uses stale spelling {stale!r}"))
            lower = authored.lower()
            if re.search(r"\bproduction[- ]ready\b|\bstable (?:end-user )?release\b", lower):
                if not _is_negated_release_line(authored):
                    findings.append(Finding(rel, number, "release-claim", "unqualified stable/production-ready claim"))

            for target in _relative_links(authored):
                target_path = target.split("#", 1)[0].split("?", 1)[0]
                if not target_path:
                    continue
                candidate = (path.parent / target_path).resolve()
                try:
                    candidate.relative_to(root)
                except ValueError:
                    findings.append(Finding(rel, number, "relative-link", f"link escapes repository: {target}"))
                    continue
                if not candidate.exists():
                    findings.append(Finding(rel, number, "relative-link", f"missing relative link target: {target}"))

    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    findings = lint_repository(args.root)
    for finding in findings:
        print(finding.render())
    if findings:
        print(f"publication-lint: FAIL ({len(findings)} finding(s))")
        return 1
    print("publication-lint: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
