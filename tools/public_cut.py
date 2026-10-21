#!/usr/bin/env python3
"""Build a deterministic, curated public projection of the private gnosix tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path, PurePosixPath
import shutil
import subprocess


class BuildError(RuntimeError):
    pass


ROOT_FILES = {
    ".gitignore",
    "AGENTS.md",
    "ARCHITECTURE.md",
    "AUTHORITY.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "NAMING.md",
    "README.md",
    "SECURITY.md",
    "THIRD_PARTY_NOTICES.md",
    "VERSION",
    "rust-toolchain.toml",
}
PUBLIC_PREFIXES = (
    ".cargo/",
    ".github/",
    "crates/",
    "dbus-1/",
    "scripts/",
    "share/",
    "systemd/",
    "vendor/",
    "xtask/",
    "docs/architecture/",
    "docs/contributing/",
    "docs/decisions/",
    "docs/evidence/",
    "docs/overview/",
    "docs/reference/",
    "docs/status/",
)

# The public face of documents the private repository keeps for itself
# (its README, its agent contract…): a file here is published at the same
# path without the prefix, in place of the private one. The overlay itself
# is never published.
PUBLIC_OVERLAY = "publication/public/"

REQUIRED_LEGAL_FILES = (
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
)

PRIVATE_PREFIXES = (
    "crates/in-gnu-os-personal-profile-provision/",
)

PRIVATE_WORKSPACE_MEMBERS = (
    "crates/in-gnu-os-personal-profile-provision",
)

PUBLIC_FIXTURES = {
    "evidence/fixtures/gnosix-r12-b2-r11-identity-current-v1.json",
    "evidence/fixtures/gnosix-r12-b2-r11-issued-v1.json",
    "evidence/fixtures/gnosix-r12-b2-profile-current-v1.json",
}

PUBLIC_TOOLS = {
    "tools/public_cut.py",
    "tools/test_public_cut.py",
    "tools/publication_lint.py",
    "tools/i18n.py",
    "tools/test_publication_lint.py",
    "tools/r13-authority.sha256",
    "tools/stage-r13-production-caller.sh",
    "tools/test_stage_r13_production_caller.py",
}

PRIVATE_MARKERS = (
    b"/home/" + b"tension_atoi",
    b"/mnt/" + b"workbench",
    b"gnosismedia" + b"@proton.me",
)


def should_include(path: str) -> bool:
    rel = PurePosixPath(path).as_posix()
    if any(rel.startswith(prefix) for prefix in PRIVATE_PREFIXES):
        return False
    if rel in ROOT_FILES or rel in PUBLIC_TOOLS or rel in PUBLIC_FIXTURES:
        return True
    return any(rel.startswith(prefix) for prefix in PUBLIC_PREFIXES)


def _safe_relative(path: str) -> PurePosixPath:
    rel = PurePosixPath(path)
    if rel.is_absolute() or ".." in rel.parts:
        raise BuildError(f"unsafe tracked path: {path}")
    return rel


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _scan_private_markers(path: Path) -> None:
    data = path.read_bytes()
    for marker in PRIVATE_MARKERS:
        if marker in data:
            raise BuildError(
                f"private marker {marker.decode(errors='replace')!r} in {path}"
            )


def _validate_literal_compile_time_includes(root: Path) -> None:
    pattern = re.compile(r'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)')
    root = root.resolve()
    for source in sorted(root.rglob("*.rs")):
        text = source.read_text(encoding="utf-8")
        for raw_target in pattern.findall(text):
            target = (source.parent / raw_target).resolve()
            try:
                target.relative_to(root)
            except ValueError as exc:
                raise BuildError(
                    f"compile-time include escapes public cut: {source.relative_to(root)} -> {raw_target}"
                ) from exc
            # A vendored snapshot's own tests reference upstream fixtures
            # that were never vendored (never built here); first-party code
            # must find every file it includes.
            vendored = source.relative_to(root).parts[0] == "vendor"
            if not target.is_file() and not vendored:
                raise BuildError(
                    f"missing compile-time include: {source.relative_to(root)} -> {raw_target}"
                )


def _rewrite_public_cargo_manifest(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    removed = []
    for member in PRIVATE_WORKSPACE_MEMBERS:
        line = f'    "{member}",\n'
        if line in text:
            text = text.replace(line, "", 1)
            removed.append(member)
    path.write_text(text, encoding="utf-8")
    return removed


def build_cut(
    source_root: Path,
    destination: Path,
    tracked_files: list[str],
    source_commit: str,
    publication_commit: str,
) -> dict:
    source_root = source_root.resolve()
    destination = destination.resolve()
    if destination.exists():
        raise BuildError(f"destination already exists: {destination}")

    tracked = set(tracked_files)
    for required in REQUIRED_LEGAL_FILES:
        if required not in tracked or not (source_root / required).is_file():
            raise BuildError(f"required legal file missing: {required}")

    # Published path -> tracked source path; the overlay wins.
    sources = {path: path for path in tracked_files if should_include(path)}
    for path in tracked_files:
        if path.startswith(PUBLIC_OVERLAY):
            published = path[len(PUBLIC_OVERLAY) :]
            _safe_relative(published)
            sources[published] = path
    destination.mkdir(parents=True)
    entries = []
    workspace_members_removed: list[str] = []
    try:
        for published in sorted(sources):
            raw = sources[published]
            source = source_root / _safe_relative(raw)
            rel = _safe_relative(published)
            if not source.is_file():
                raise BuildError(f"tracked public file is missing: {raw}")
            target = destination / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
            if rel.as_posix() == "Cargo.toml":
                workspace_members_removed = _rewrite_public_cargo_manifest(target)
            _scan_private_markers(target)
            entries.append(
                {
                    "path": rel.as_posix(),
                    "bytes": target.stat().st_size,
                    "sha256": _sha256(target),
                }
            )
        _validate_literal_compile_time_includes(destination)
    except Exception:
        shutil.rmtree(destination, ignore_errors=True)
        raise
    manifest = {
        "schema": "gnosix-public-cut-v1",
        "source_commit": source_commit,
        "publication_commit": publication_commit,
        "history_model": "fresh-public-root",
        "raw_evidence_included": False,
        "public_fixtures_included": sorted(PUBLIC_FIXTURES),
        "overlaid": sorted(p for p, raw in sources.items() if raw != p),
        "private_paths_excluded": list(PRIVATE_PREFIXES),
        "workspace_members_removed": workspace_members_removed,
        "license_present": (destination / "LICENSE").is_file(),
        "third_party_notices_present": (destination / "THIRD_PARTY_NOTICES.md").is_file(),
        "file_count": len(entries),
        "total_bytes": sum(entry["bytes"] for entry in entries),
        "files": entries,
    }
    manifest_path = destination / "PUBLICATION-MANIFEST.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    _scan_private_markers(manifest_path)
    return manifest


def tracked_files_from_git(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        check=True,
        capture_output=True,
    )
    return [item.decode() for item in result.stdout.split(b"\0") if item]


def git_rev(root: Path, rev: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", rev],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--publication-commit")
    args = parser.parse_args()

    root = args.root.resolve()
    publication_commit = args.publication_commit or git_rev(root, "HEAD")
    manifest = build_cut(
        root,
        args.destination,
        tracked_files_from_git(root),
        args.source_commit,
        publication_commit,
    )
    print(
        f"public-cut: PASS files={manifest['file_count']} "
        f"bytes={manifest['total_bytes']} license={manifest['license_present']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
