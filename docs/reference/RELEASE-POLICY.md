# Gnosix release policy

Ratified by the operator on 2026-10-21. This document is the source; the
public repository receives it with each release.

## Where the code lives

- **The private repository is the truth** (`gnu6dotlive/gnosix-engineering-archive`).
  Everything is developed, reviewed and merged there, through pull requests
  only.
- **The public repository is a projection** (`gnu6dotlive/gnosix`), written
  one way, at each release, by `tools/public_cut.py`: a whitelist of paths,
  private exclusions, a sha256 manifest. Nothing is developed there.
- **A public contribution** (an issue, a pull request) is read, integrated in
  the private repository, and comes back through the next release. The public
  pull request is closed with a reference to that release.
- **Mirrors** (`git.gnu6.live`) copy; they are not a second truth until the
  operator decides otherwise, repository by repository.

## Versions

Two kinds of version, for two kinds of promise.

**The product: `YY.MM` and a codename.** A release is named by the year and
month it ships, and a codename: `26.11 « Ardoise »`. The number says how old
it is at a glance; the codename gives it a voice. Codenames are minerals and
rocks, in alphabetical order (Ardoise, Basalte, Calcite…).

| Form | Meaning |
| --- | --- |
| `26.11-dev` | The edge: private `main` on its way to 26.11 |
| `26.11-rc1`, `26.11-rc2` | Release candidates, published as pre-releases |
| `26.11` | The release |
| `26.11.1`, `26.11.2` | Corrections to it, nothing new |

The canonical product version is the root [`VERSION`](../../VERSION) file.

**The crates: SemVer.** Every crate keeps its own semantic version: it is a
contract with whatever depends on it, inside Gnosix or out. A crate whose
public interface changed since the last release must change its version in
the release pull request; the release check refuses one that did not. Each
release lists the version of every crate it ships
(`release_manifest.json`).

## Channels

- **edge** — private `main`, deployed in the operator's session.
- **stable** — the latest `YY.MM` release.
- **LTS** — decided later, when a release is meant to be kept.

## Cadence

Two releases a year, in May and November. Candidates in the weeks before;
corrections when needed.

## What a release needs

1. Every quest it carries confirmed by the operator (MEGA-REVIEW).
2. The projection, alone, builds and passes its tests in the pinned runner
   container, and the public workflow passes there, locally. A GitHub run is
   never how a release finds out it works.
3. The publication lint clean: no workstation path, no secret, no model
   weight, every file in the manifest.
4. The changelog written: what it carries, grouped by quest.
5. A signed commit (« Gnosix 26.11 « Ardoise » ») and a signed annotated tag
   `v26.11` in the public repository; one commit per release, the private
   history never leaves.

## License

MIT, for first-party code and documentation (decided 2026-10-10 in public,
ratified here). Bundled third-party material keeps its own terms; see
`THIRD_PARTY_NOTICES.md`. Releases already published under another license
keep the permissions they gave.
