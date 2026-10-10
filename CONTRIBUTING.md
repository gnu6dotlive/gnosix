# Contributing

gnosix is experimental / pre-release and its public engineering surface is still being normalized. Contributions should preserve the project's explicit authority and evidence boundaries rather than treating it as a conventional theme or monolithic shell.

## Before changing code

Read [ARCHITECTURE.md](ARCHITECTURE.md), [AUTHORITY.md](AUTHORITY.md), [NAMING.md](NAMING.md), [AGENTS.md](AGENTS.md), and [docs/status/STATUS.md](docs/status/STATUS.md).

For architecture-sensitive changes, identify the semantic owner, state owner, mutation authority, failure behavior, and evidence required before implementation. Do not create a new daemon, persistent store, IPC surface, or generic command executor as an implementation shortcut.

## Local checks

```bash
cargo xtask explain
cargo xtask ci
```

`cargo xtask ci` is the same repository-owned qualification contract invoked by GitHub CI. Run it locally before opening or updating a change that should be publication-ready.

`cargo xtask verify` adds host/live checks and is not a casual superset: use it only in an appropriate authorized environment.

## Evidence

Tests and receipts should state exactly what they prove and what they do not prove. Prefer deterministic source tests for invariants, runtime evidence for runtime behavior, installed-path evidence for deployment claims, and explicit operator review for human/perceptual gates.

Raw logs are not a substitute for a bounded claim. Link the smallest evidence set that supports the claim.

## Changes and commits

Keep commits coherent and reviewable. Preserve unrelated work and byte-exact provenance. Do not rewrite upstream/vendor patches or historical evidence to satisfy cosmetic formatting rules.

AI-assisted changes are acceptable when their source, review boundary, tests, and evidence remain inspectable. Generated text or code does not receive special authority merely because a model produced it.

## Licensing

First-party contributions are submitted for inclusion under the **MIT License** (see [LICENSE](LICENSE)). Contributors must have the necessary rights to license their work under MIT; do not copy GPL-only third-party code into MIT-designated first-party files. Retain third-party notices and update `THIRD_PARTY_NOTICES.md` when importing third-party material.
