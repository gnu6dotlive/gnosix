# blobin-ui-core, vendored

- **Source:** `crates/blobin-ui-core` of the private repository `tension-atoi/blob.in-showcase`, at revision `5c963b61fd52c46df94d3b950f815f7d3022f5e6` (the revision `gnos-ux-gpui` pinned as a git dependency until 2026-10-20).
- **Why vendored:** the remote CI runner has no credentials for that private repository, so `cargo xtask ci` failed before running a single check ("failed to authenticate when downloading repository"). Vendored, the workspace resolves with no access beyond crates.io, as `vendor/gpui-pre-*` already do.
- **What was copied:** `src/`, `tests/` and `README.md`, unchanged. `Cargo.toml` spells out the fields the source workspace inherited (edition 2021, rust-version 1.85, publish = false, LicenseRef-Proprietary). The examples are left out.
- **To update:** copy the same paths from the new revision with `git archive <rev> crates/blobin-ui-core/src crates/blobin-ui-core/tests crates/blobin-ui-core/README.md`, then record the revision here. The crate has no dependencies.
