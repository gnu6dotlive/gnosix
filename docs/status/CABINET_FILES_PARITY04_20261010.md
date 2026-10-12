# CABINET-FILES-PARITY-04 — Progressive Files and live observation

Date: 2026-10-10. Worktree `feat/cabinet-files-parity-04-20261010` from
`8ea4102`. **Bounded E2/E3 technical evidence only.**
Visual parity and operator E4 acceptance remain OPEN.

## Actual first-party implementation

- Native read-only standalone Files enumerates **100,000 entries maximum**,
  sending 2,048-item provisional batches through a **three-batch bounded**
  queue with generation cancellation and backpressure. Background sorting
  supplies a deterministic final snapshot. **Not unbounded/keyset pagination.**
- XDG FileChooser remains on its independent **4,096-entry** limit.
  No change to the host GTK portal or privileged file-operation authority.
- A nonrecursive inotify watch observes the current Linux folder, debounced
  at **300ms**, cancelled on route/folder change. Create/delete/move/close-write
  triggers a read-only rescan.
- Watch invalidation/overflow is never silently discarded when the small event
  queue is full: a sticky atomic high-priority marker upgrades the next pending
  event, terminates the stale watcher consumer and triggers a fallback rescan.
  The interface reports an offline watcher; **Refresh** explicitly rearms it.
- Native view-build latency telemetry samples a rolling 512 CPU view
  construction durations. It **does not** measure GPU present, compositor
  frame pacing, animation FPS or end-to-end responsiveness.

## Exact proofs and dispositions

| Test | Receipt | Disposition |
| --- | --- | --- |
| Source and safety | 76 Rust tests PASS, including overflow priority, duplicate coalescing, kernel observation and scan cancellation; two explicit heavy tests omitted from routine run | E2 PASS |
| 100k standalone stress | 100,001 temporary files, 100,000 valid, 49 batches, capped/truncated; local run: creation 170ms, scan+sort 104ms | E2 scoped PASS |
| Native 100k user journey | Weston native GPUI window displays 100,000 rows, keyboard End selects a last file; its isolated fixture reported 545ms worker scan | E3 scoped PASS |
| Live filesystem | Native inotify create and delete events caused actual rescans and separately captured updates | E3 scoped PASS |
| Prior controls | Routes, restart, list focus, folder navigation, search, Grid, thumbnail inspector and path input all replayed on native Wayland | E3 scoped PASS |
| Strict build | Rust 76 PASS, strict Clippy, native binary rebuild with SHA-256 | E2 PASS |
| Repository CI | Fresh hermetic `cargo xtask ci` receipt 48/48 PASS, no non-PASS gate | E2 PASS |
| Frame-present p95/p99 | Not measured; view-build metrics are only CPU UI-tree construction | OPEN |
| Spacedrive visual parity | No normalized viewport/DPI reference set, no Hyprland E4 operator acceptance | OPEN |

Evidence roots (private workstation):
- `/mnt/workbench/evidence/runs/cabinet-files04-100k-scan-20261010/`
- `/mnt/workbench/evidence/runs/cabinet-files04-final-100k-20261010/`
- `/mnt/workbench/evidence/runs/cabinet-files04-final-watch-20261010/`
- `/mnt/workbench/evidence/runs/cabinet-files04-final-{routes,files,focus,view}-20261010/`
- `/mnt/workbench/evidence/runs/cabinet-files-parity04-qualified-build-20261010/`
- `/mnt/workbench/evidence/runs/cabinet-files-parity04-qualified-ci-20261010/`

## Live UX review and next gates

The 100k screenshot genuinely shows beginning and end of a virtualized list,
selection, inspector and bounded result count. It also shows **breadcrumb
clipping**, excess header space and the user-visible 100k cap notice.
These are real gaps, not proof of premium UX.

**P05 proposal:** stable sortable/indexed paged results beyond 100k;
persistent range/multiselect, keyboard + pointer semantics across rescans;
debounced scope-aware search; end-to-end p95/p99 Hyprland presentation and
input latency; responsive breadcrumbs/toolbar and semantic previews;
accessibility and independent operator multi-viewport visual acceptance.
The app store, news, installer, rendering experiments, push, merge and host
portal changes are outside this bounded milestone.
