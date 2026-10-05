# Agent contract

This repository is developed with human and agent assistance. Current source, current runtime evidence, and the latest explicit operator ratification are the working truth.

## Required behavior

- Inspect the current worktree and concurrent work before substantial edits.
- Preserve unrelated changes; do not reset, restore, or stash them merely to make a check green.
- Patch forward from current truth instead of reviving superseded local trees.
- Keep source facts, runtime observations, operator decisions, and hypotheses distinct.
- Do not infer authority from names, dependencies, services, processes, file locations, or old documents.
- Keep product claims within the boundary documented in [docs/status/STATUS.md](docs/status/STATUS.md).
- Preserve byte-exact upstream/vendor/provenance artifacts when their bytes are evidence.

## Host boundary

Repository, build, test, and in-scope runtime work does not automatically authorize mutation of the operator workstation. Installing/removing/upgrading host packages, changing persistent desktop configuration, or widening runtime privileges requires explicit operator authorization.

Privilege escalation inside an authorized operation follows the repository policy for that operation; never invent a broader authority path because it is convenient.

## Qualification

Run repository checks locally before relying on GitHub Actions. `cargo xtask ci` is the repository-hermetic publication contract. GitHub Actions is canonical remote publication CI, not a test bench.

`cargo xtask verify` includes host/live checks. Use it only when its environmental assumptions and required host interactions are satisfied and authorized.

A passing check proves only the bounded contract exercised by that check. It does not imply stable release readiness, product-default ratification, perceptual acceptance, or authority outside the tested scope.

## Current product boundary

The integrated tree contains qualified native product surfaces, current-session multi-output routing, durable logical identity/profile contracts, installed typed workspace activation, application lifetime separation, and qualified user-session lifecycle work.

Whole-product release acceptance remains open. End-user screen-reader usability is not claimed. Default personal-profile provisioning and other individually named open capabilities remain separate work; consult the current status/register before changing those boundaries.

## Checkpoints

Commit coherent, reviewable progress when it establishes implementation, a regression fix, qualification evidence, an operator decision, or a durable research conclusion. Review staged paths first and do not absorb unrelated concurrent work.

When evidence contradicts a prior claim, retain the counterevidence and narrow or correct the claim. Never make the evidence disappear to preserve a narrative.
