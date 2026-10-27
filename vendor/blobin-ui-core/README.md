# blobin-ui-core

Renderer-agnostic Rust component identity, page-granular copy-on-write state, keyed child reconciliation and deterministic DPI-aware constraint layout (UI-G02/A+B, UI-G03). Requires only the Rust standard library. **Not yet a rendering toolkit.**

Each `UiTree::transact` call either atomically publishes all changes or returns a typed error without publishing a partial tree. Snapshots are immutable, sharing untouched 64-slot node pages. `NodeId` includes a generation so a deleted or aborted handle cannot impersonate a new node. `UiTransaction::reconcile_children` preserves keyed subtrees through reorder, reporting domain-specific damage without invoking a renderer.

```sh
cargo run --offline -p blobin-ui-core --example tree_transaction
cargo test --offline -p blobin-ui-core --all-targets
cargo run --offline -p blobin-ui-core --example layout_workbench
```

See [G02/A](../../docs/BLOB-UI-G02A-PORTABLE-COMPONENT-SUBSTRATE-20261012.md) and [G02/B](../../docs/BLOB-UI-G02B-INCREMENTAL-RECONCILIATION-20261012.md) for API boundaries, tests and non-claims. Run `python3 scripts/blobin_ui_g02b_verify.py` or `python3 scripts/blobin_ui_g03_verify.py` from the repository root to reproduce each scoped qualification. [G03 deterministic layout](../../docs/BLOB-UI-G03-DETERMINISTIC-LAYOUT-20261012.md) defines geometry, clip and invalidation boundaries; no hit-testing or renderer is claimed.

Input routing (UI-G04/A) is exposed through `blobin_ui_core::input`; see [the headless contract](../../docs/BLOB-UI-G04A-HEADLESS-INPUT-20261012.md). The native Wayland delivery gate **UI-G04/B remains OPEN**.
