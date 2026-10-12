# CABINET-FILES-PARITY-05 — Indexed Files, selection continuity, premium fluidity

Date: 2026-10-10. **ACTIVE / OPEN**, child worktree of clean P04 `60340f7`.
This is an engineering contract, not a PASS receipt.

## Objective and invariant

Cabinet must browse arbitrarily large single directories without silently hiding
entries, losing selection during ordinary filesystem churn, blocking GPUI on I/O,
or misrepresenting CPU view construction as presented-frame latency.

P04 remains binding for its tested 100k/read-only/watch scope. P05 does not
retire the 4096-entry standalone XDG chooser isolation contract.

## Authority / excluded scope

Allowed: native standalone Cabinet Files model, asynchronous read-only scanner,
bounded private cache/index, interaction state, GPUI layout, performance harness,
isolated Wayland tests, reproducible evidence and local commits.

Excluded: privileged writes or generic filesystem operation authority, GTK/XDG
portal rewiring, package installation, Store/News/Installer, speculative
public claims, merge/push without an explicit integration decision.

No user configuration changes; use an isolated compositor for E3 proofs.
Do not mutate the operator's real files to simulate workload.

## Workstream A — stable indexed directory paging

Establish canonical total-order sort: folders first, Unicode label casefold
behavior explicitly declared, and exact path tie-breaker. Keyset/index page
boundaries must have no gaps, duplicates or reorder under a fixed generation.
No `skip(n).take(page)` over an unbounded materialized Vec presented as
"paging". The implementation must genuinely bound resident memory or declare
a documented full-index cost; an on-disk external-sort/merge index is an
acceptable candidate. Scan, sort/index, page and cache work run off GPUI.

Explicit generation identity; cancel old scans, drop stale arrivals,
atomically promote only complete index generations and preserve last-known
view with a named refreshing/stale state where appropriate.

Prove >100k with 100001, 250000 and 1000000-item fixture paths in a lab
that can be skipped on hardware limits without claiming PASS. Page sizes
bounded, backpressure specified, index ownership/cleanup and error recovery
qualified. Search and hidden-file filters must apply across *all* pages,
not only cached or initially visible entries.

## Workstream B — selection as a first-class model

Use stable file identity distinct from row position. Specify treatment of
rename/move (path identity alone cannot magically follow renames), hidden
filters, live deletion, reorder and multi-tab independence.

Implement click, Ctrl-toggle, Shift-range, Ctrl+Shift additive-range,
keyboard arrows/Home/End with Shift, select-all of current query scope,
focus/anchor/active selection, and inspector semantics for zero/one/many.
Selection is not reduced to a single `selected: Option<PathBuf>`.

During provisional scans avoid pruning valid selection. Reconcile only on an
authoritative generation or exact disappearance event; restore visible
cursor and scroll by identity where possible. No stale item opens.
## Workstream C — end-to-end latency and memory

Separate: input event → application state change; UI tree build; renderer
submission; compositor/frame presentation. Instrument only where measurable.
Report uninstrumented links as NOT_OBSERVED, not zero or inferred FPS.

Collect p50/p95/p99, max, count, warmup/discard rule and hardware/runtime
provenance. Scenarios: idle, scroll, rapid keyboard selection, filtered search,
Grid resize, watcher burst and route cancellation. Corpora: 0, 4k, 100k,
250k+; repeat with thumbnails on/off, inspector open/closed, 1x/2x.

Report RSS/cache budget and allocation trends; no unbounded thumbnail or
visible-cache growth. Compare against P04 source/binary under identical
compositor resolution/DPI, machine load and workload. State whether each
threshold is a target or an observation; avoid invented 60 FPS claims.

## Workstream D — actual premium layout

Fix path bar overflow at narrow widths by preserving final path segment and
navigation actions; ancestor collapse/scroll is allowed. Ensure text is
ellipsized, keyboard-accessible and reversible into the full path editor.
Repair excessive header density, toolbar shrink behavior, inspector responsive
limits, hover/focus/selection visual hierarchy and long-name handling.

Confront actual Cabinet versus Spacedrive 2 at normalized viewport/DPI and
equivalent task states. Reference imagery must be provenance-bound. Copy
neither proprietary assets nor unsupported internals. User decides E4 taste.

## Non-negotiable regressions

P04: cancellation, 2048-sized previews, bounded producer queue, stable watch
refresh, sticky high-priority overflow semantics, 100k End path.
P03: adaptive 1–6-column grid and bounded genuine thumbnails.
P02: per-tab state, Unicode search, absolute path, focused navigation.
GTK portal: unchanged chooser source, 4096 cap, authorization and process.
## Close gates

P05-A: source tests including adversarial Unicode/tie order, page-boundary
no-gap/no-duplicate, cancellation, error recovery and indexing resource budget.
P05-B: independent selection pure-model tests including scan generations and
cross-page/range behavior, and real pointer/keyboard Wayland E3 journeys.
P05-C: nonzero frame/input observation with documented instrument reach,
repeatable p95/p99 and memory recordings versus P04.
P05-D: real narrow/wide/2x screenshot comparisons, no pathbar overflow,
keyboard-accessible controls and operator E4 visual decision.
P05-E: strict Rust fmt/Clippy/tests, `cargo xtask ci` exact-head,
clean tree and SHA-256 binned artifacts. No GitHub Actions test loop.

Capture exact HEAD, tree state, Cargo.lock, toolchain, compositor, GPU,
resolution, fixture construction and raw traces in compact falsifiable
receipts. A missing external presentation instrument is a named gate OPEN.

Closure requires all main gates, not a narrative description of progress.
Commit by behavioral invariant and retain each discovered side quest separately.

## Initial observed implementation debt at P04

- `cabinet_stream.rs` caps source entries at 100000 then sorts in-memory.
- `bin/cabinet.rs` stores a single selected path and resets cursor on rescan.
- `rebuild_visible` retains a full `Vec<usize>` per new preview generation.
- narrow breadcrumb is visibly overflowing; GPU presentation not measured.

Initial execution sequence: (1) pure indexed ordering/selection contracts,
(2) worker pagination/search wiring, (3) GPUI gestures/reconciliation,
(4) latency instruments + Wayland routes, (5) responsive visual E4.

## Implementation checkpoint 01 — selection model and source wiring

- Implemented identity-based replace/toggle/range/add-range pure model and
  wired Ctrl/Shift pointer gestures in native List/Grid and Shift keyboard
  navigation. Provisional scan no longer prunes selected paths; final scan
  reconciles existence. A small multi-selection status is displayed.
- Narrow breadcrumb count/width is now bounded at source level; no real
  screenshot has qualified its perceptual result.
- Qualification: selection pure tests 5/5 PASS; entire crate unit suite
  75 PASS / 2 explicit heavy ignores; native-gpui `cargo check` PASS;
  strict all-features/all-targets Clippy PASS; hermetic `cargo xtask ci`
  48/48 PASS. Logs: `/tmp/cabinet-p05-{unit,clippy,xtask-ci}.log`.
- Explicitly OPEN: end-to-end Wayland gestures, full multi-select durable
  session persistence, indexed >100k paging, measured GPU-present p95/p99,
  visual Spacedrive E4 and operator approval.
- This does not constitute P05 closure.

## Implementation checkpoint 02 — disk index and native page navigation

**Scope: technical E2/E3 partial; P05 remains OPEN.** The P04 progressive
scanner remains in use as a capped 100000-item preview during each directory
load. A second worker pass builds an external sorted index from all qualifying
entries, using 8192-entry sorted runs, at most 32-way merge and a disk offset
map. Only after index/filter completion is a native 1024-entry page promoted.
This *temporarily* retains the P04 100k preview in memory; P05 does **not**
yet claim bounded peak RSS independent of directory size, only bounded
indexed steady-state page residency.

A source-owned temporary root is mode 0700 and deleted when no index/view
owner remains. Cancellation stops publication, stale scan epochs reject late
results, and filters are evaluated over the full index on a background worker
with a replaceable filtered offset file. Sorting uses folders-first, lowercase
Unicode label comparison and full path tie break. Raw OS path bytes survive
the index encoding. No GTK chooser source/limit/authority was modified.

The native UI now renders truthful page N/M controls, page up/down shortcuts
and Ctrl+End navigation to the last indexed file. Search over all records
is live-qualified against a record beyond the former cap. Selection is still
per-path, with a scoped off-page indicator, but cross-page Shift ranges,
durable multiselection restore and whole-index deletion reconciliation remain
OPEN. The page buttons are not yet independently screen-reader reviewed.

Recorded experiments (temporary local filesystems, not general I/O SLA):

- E2 100001 physical files: complete index, last page, single-result filter
  PASS; build+query recorded as 265 ms in first lab run.
- E2 300001 physical files: 37 sorted runs, multi-pass merge, boundary and
  last-page order, global single-result filter PASS; 1013 ms recorded in lab.
- E3 Weston/Xvfb isolated GPUI 1100x820: index published with 100003
  entries (two extra fixture entries), PageDown reaches page 2, Ctrl+End
  reaches page 98/98 and selects f-100000.txt, Ctrl+F searches that same
  out-of-cap record globally and returns exactly one match. Screenshots
  observed for page end and filtered match, inspector remains functional.
- Narrow search ornament and paging controls were separated from the action
  row; root geometry inset is aligned with 16px actual GPUI padding.
  Screenshot at 1100x820 no longer clips right header text. E4 cross-viewport
  premium judgement remains OPEN.

Evidence roots:
- /mnt/workbench/evidence/runs/cabinet-files05-100003-page-20261010-v3
- /tmp/cabinet-p05-index100k.log
- /tmp/cabinet-p05-index300k.log

The v3 isolated journey precedes the final checkpoint commit; an exact-HEAD
replay and hermetic CI are required before this checkpoint can be promoted to
a durable exact-HEAD evidence receipt.

## Remaining main gates

- PAGING: independent high-cardinality RAM/RSS and disk budget; query burst,
  watcher churn and error recovery under indexed mode, index mutations.
- SELECTION: contiguous Ctrl+Shift ranges across page boundaries, complete
  per-tab/durable session semantics, exact deletion reconciliation.
- PERFORMANCE: p50/p95/p99 input-to-present, frame pacing, memory and real
  Hyprland comparison at normalized resolution/DPI.
- VISUAL: 1x/2x narrow/medium/wide review vs attributable Spacedrive reference,
  no clipping, operator E4 acceptance and accessibility review.

## Implementation checkpoint 03 — indexed range, durable selection and bounded latency

**E2/E3 qualification, E4/perceptual CLOSE GATES OPEN.**
No new authority and no GTK FileChooser behavior change.

### Stable cross-page range

- Native Shift and Ctrl+Shift range gestures now resolve selected identities
  on a worker from one immutable, generation-scoped filtered index. An
  unavailable anchor, cancellation, stale scan/filter/selection generation,
  or >8192 paths fails without publishing an incomplete set.
- The pure model preserves anchors/focus as paths independently of page rows.
  Membership checks against the complete index reconcile deletions without
  falsely pruning selected items outside the currently loaded page.
- A real regression was discovered: filter refresh incremented the selection
  epoch after membership was launched, discarding the repair. The ordering
  was corrected and the exact deletion scenario replayed.

### State persistence

- Backward-compatible session version 1 fields store the full selected path
  identity list, selection anchor and focus per tab. Legacy single selection
  remains compatible; no migration of host/GTK state is required.
- Validation rejects duplicate/relative identities, excessive individual
  path length, excess global selection count (>8192) and oversized serialized
  state. The private session file remains atomically written, mode 0600.
  Source state budget raised from 32768 bytes to 64 MiB to make a bounded
  large selection representable; no unbounded session storage is claimed.
- A selection change that cannot be validated is rolled back with an explicit
  error. The entire multi-tab session shares a cap. Extreme long-path/tab
  combinations are not yet performance-qualified.

### E3 isolation replay and causal repair

Wayland compositor: Weston on isolated Xvfb, 1100x820.
Corpus: 100001 generated files, plus two existing fixture entries.

- Cross-page gesture selected 1699 identities: PASS.
- Session persisted 1699 identities and original anchor: PASS.
- Process stopped and relaunched; 1699 restored: PASS.
- Removed the selected anchor file; native inotify refreshed. Fully indexed
  scan reported 100002; membership removed one identity: 1698 retained,
  invalid anchor removed: PASS.
- Before repair, the same test failed to reconcile after restart; the
  counterexample was retained as RED evidence.

Roots:
- /mnt/workbench/evidence/runs/cabinet-files05-range-durable-20261010-v1 (RED)
- /mnt/workbench/evidence/runs/cabinet-files05-range-durable-20261010-v2 (GREEN)

### Input-to-state instrumentation

Optional CABINET_INPUT_LATENCY_TRACE=1 records handler entry to completed
application model mutation. Instrumented types include local selection,
indexed range, indexed page and keyboard cursor. It does NOT measure physical
keydown-to-handler, GPU submission or compositor presentation.

Lab playback on 100003 entries produced 260 keyboard transitions. The
rolling 256-sample summary was p50 137us / p95 145us / p99 170us,
max 238us. Not a hardware SLA or input-to-photon metric.

Root: /mnt/workbench/evidence/runs/cabinet-files05-inputstate-20261010-v1.

### Open obligations for P05

- Real input-to-visible GPU/compositor timing, Hyprland and p95/p99.
- Peak RSS and preview cache measurements, 250k–1M and multi-tab stress.
- Viewport/DPI accessibility and operator E4 Spacedrive comparison.
- Adversarial watcher/filter races and selection beyond the 8192 cap.

Closure of P05 remains prohibited until those gates are satisfied.
