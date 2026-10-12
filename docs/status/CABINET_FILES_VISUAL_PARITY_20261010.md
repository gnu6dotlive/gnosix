# CABINET-FILES-PARITY-01 — Files-first UX parity before feature expansion

**2026-10-10** · proposed engineering acceptance contract · **NOT QUALIFIED / no promotion**.
Companion source audit: [CABINET-UX-TRUTH-01](CABINET_UX_DONOR_AUDIT_20261010.md).

## Why this gate exists

Spacedrive is a **reference for interaction quality, navigational coherence, hierarchy and file-native presentation**, not a mandatory clone or a substitute for real native Linux architecture. Cabinet's screenshot evidence at SHELL-03 shows the persistent native window is real, but its file explorer is still visually sparse. A polished skeleton is not parity.

Version boundary: published [Spacedrive v1 Explorer](https://spacedrive.com/blog/october-alpha-release) describes Icons, List, Columns and Media; [2024 v0.2 release](https://spacedrive.com/blog/alpha-zero-two-release) adds tabs and multi-select. As of this audit, [Spacedrive v2's official main](https://github.com/spacedriveapp/spacedrive) describes a new unified source/index/operation architecture and **plans 2.0 Beta on 2026-11-01**. This is not proof those future beta features are available today. Its separate [SpaceUI](https://github.com/spacedriveapp/spaceui) inventories tokens, 40+ primitives and 14 Explorer elements, which are an architectural *reference*, not a compatible GPUI code dependency.

**Concurrent file-manager references:** [Flux](https://github.com/killown/flux) describes Tokio+Rayon chunked traversal and background thumbnails; claims of 105ms startup are **author-reported, not independently reproduced**. [COSMIC Files](https://github.com/pop-os/cosmic-files) provides an actively maintained Rust desktop file manager and a separate thumbnailer; adopt message/operation separation as a research reference. These are file-manager concurrency comparators; no uniquely named product “Concurrent File Manager” was verified.

## Visual baseline set and provenance rules

- Cabinet source: `72580a0`; screenshot `/mnt/workbench/evidence/runs/cabinet-shell03-final-routes-20261010/files-start.png` (1100x820, Weston X11/Wayland, synthetic two-entry fixture).
- Donor Shelly-GPUI source: `6fce3747`; screenshot `docs/gpui/ux-workbench/evidence_ux02_07_package_selected_inspector.png` (1263x1302, older Hyprland captured *package* UI). Used ONLY for patterns such as view density, source/identity and pinned detail.
- External Spacedrive published media: official v1 website/release visual references must carry date and version. **Never** write “Spacedrive 2 parity PASS” by comparison to a v1 promotional screenshot.
- Snapshot captures must record: exact source HEAD + dirty-state flag, binary SHA-256, OS/compositor/GPU, viewport/window dimensions, DPI/scale, theme, density, test dataset hashes, active route, keyboard focus, pending tasks, image SHA-256 and clipping/scroll state.
- Direct pixel comparisons require matching normalized viewport, scale, fixtures, themes and stable capture sequence. Without these, screenshots permit qualitative hierarchy review only.

## P0 Files-first visual and interaction gates

All gates below are **OPEN unless separately given actual evidence**. Target states are contractual, not current Cabinet capabilities.

| Gate | Real behavior/geometry demanded | Evidence to collect | Original pre-foundation Cabinet disposition |
| --- | --- | --- | --- |
| V01 Navigation authority | Files default opens its own **Locations rail**; Store reachable only through secondary `Software` workspace; no duplicated Files/Store/News/Installer tabs and sidebar; typed route + scoped focus | initial screenshot and tab traversal; no duplicate first-level nav | **FAIL / duplicated global route navigation** |
| V02 Primary file canvas | Files pane dominates content; inspector secondary, no content occlusion. Verify at 513x933, 980x720, 1100x820, 1440x900 and 1920x1080, 100–200% scale | normalized screenshots + pane rectangles + clamp solver | **PARTIAL** responsive splitter; per-window constraints unmeasured |
| V03 Canonical pathbar | Real accessible breadcrumbs, edit/copy absolute path, Home/back/forward, places and reload; no path loss on tab switches | folder back/forward and source-resident states | **FAIL / raw header path only** |
| V04 Files search | Explicit **search in this folder** then indexed all-locations only when available; keyboard focus, Escape/clear, cancellation, query status | screenshots & E2E exact match/query origin | **FAIL / no standalone Files search input** |
| V05 Native views | Working **List** and **Grid** modes with file-type-specific icons, consistent selection/hover/focus. Columns/Media staged after initial parity, not faked as static buttons | real file fixture covering directories, text, image, video, non-UTF8; mode-switch E2E | **FAIL / rows only** |
| V06 Thumbnail provenance | Lazy real image/media thumbnails with fallback, caching budget, no false preview on unreadable file | thumbnail source + cache watermark + screenshot | **FAIL / text glyphs only** |
| V07 Real inspector | Pinned file identity/actions, independent scroll body, MIME/size/timestamps/path, empty and error states; inspector never eclipses Files canvas | overflow, selection, long metadata and reflow screenshots | **PARTIAL / plain detail string** |
| V08 Focus correctness | List view keyboard traversal uses Up/Down, typeahead, Home/End, Shift range as specified; **constant tab-stop count** independent of row count; focus indicator remains visible after scroll | 4k and 100k fixtures, Tab count and re-entry | **FAIL / per-row tab stops** |
| V09 Context/menus | Anchored and keyboard-operable menus, correct disabled/cancel actions, no clipping or accidental files mutations | pointer+keyboard at 3 sizes, no unintended writes | **OPEN / no full context menu** |
| V10 Selection semantics | Range/multi selection; tab/state isolation, no stale selected path after navigate, folder vs file affordances | click/Shift/Ctrl sequences on two tabs with restart | **PARTIAL / tab state exists, multi-select absent** |
| V11 Feedback states | Native loading, empty folder, inaccessible, stale, offline, cancelled; no status labels pretending data is loaded | seeded failure corpus and screenshot states | **PARTIAL / narrow loading/error labels** |
| V12 Motion and resizing | Every motion interruptible, focus stable, reduced motion snap, divider drag measured; no idle RAF churn or clipping | real Hyprland drag and p95 frame metrics | **PARTIAL / isolated inspector F9, drag not pixel-gated** |
| V13 Visual semantics | Gnosix token system, theme contrast, consistent typography and icons, opacity floor for content surfaces; no arbitrary RGB per view | token/contrast checks, dark/light at each size | **FAIL / hardcoded colors and no proven light parity** |
| V14 Files/Software separation | Files tabs never become Store tabs; Software has its own discovery/installed/updates/news subsections; package-source cards NOT file icons; distinct query scopes and permissions | route screenshots, state tests, privilege analysis | **FAIL / four equally weighted global views** |
| V15 Stable async | No stale folder response after rapid navigation; watcher overflow recovery; bounded 100k listing, cancellation/backpressure and lazy thumbnail pipeline | 100k / 10k with rapid change + memory and correctness traces | **PARTIAL / 4096 cap, async single read_dir job** |
| V16 Human acceptance | Operator works through 3 actual file tasks without prompts; visual rank and interaction quality compared on same screen against reference | operator recorded results/screenshots with consent | **OPEN** |

## Promotion levels

- **E0** idea/reference screenshot; **E1** code exists; **E2** unit/geometry oracle; **E3** isolated native Wayland interaction; **E4** identical fixtures on reference hardware plus visual review; **E5** independent human acceptance and clean build; **E6** product release. Report **per gate** with exact evidence, not one “premium PASS”.
- **Freeze:** until P0 V01–V04, V07–V08, V12–V14 and the basic subset of V05/V11 are E4, no new Store backend, no RSS integration, no installer mutations, no ambient shaders/translucency experiments.
- **P1:** after P0: V06/V09/V10, scalable filesystem V15, richer previews and cross-source indexing, each with distinct technical security qualifications.
- **P2:** only after P1: distributed source model, advanced visual effects, animated thumbnail transitions, multi-window drag-and-drop, and Store ↔ News ↔ Installer deep integration.

## Toolchain to build for parity (tooling scope, not new product features)

1. **Audit input manifest** — read-only tool enumerates exact donor/current HEAD and SHA-256 of canonical screenshots; rejects stale/missing input. Does not distribute private screenshots.
2. **Deterministic fixtures** — seed small, 4k and 100k dirs; mixed MIME, symlinks, metadata, invalid permissions, delete/recreate, watcher overflow; maintain hash manifest and source of generated data.
3. **UI semantic state capture** — typed route/tab/focus/query/view mode/selected IDs, observer-only, no installing or walking unrelated user files.
4. **Native compositor harness** — Weston isolated for regression, real Hyprland for human visual acceptance; capture initial/list/grid/inspector/compact/empty/error/keyboard/focus/hover/drag.
5. **Visual diff** — only matching dimensions/scale/data; canonical crop masks for chrome and canvas; separately report typography clipping, pixel displacement, contrast and hit area, no unreviewed per-pixel “sameness” threshold.
6. **Frame + async traces** — compositor frame cadence, GPUI build/layout/paint, queue depth, scan stage, thumbnail decoding, input-to-commit, memory/VRAM. p50/p95/p99 on fixed datasets.
7. **Keyboard/a11y oracle** — Tab/Shift+Tab focus cycle count bounded with list length; screen reader/AT-SPI independent gate; no fake focus rings.
8. **Privilege oracle** — Files and Software context types are distinct; RSS/catalog data cannot authorize file deletion, install or shell execution.

## Proposed quantitative performance goals — NOT measured

- Stable 60 Hz target: frame pacing p95 ≤ 16.7 ms under the stated workload; 120 Hz aspirational target p95 ≤ 8.3 ms on supported display. This requires **frame-present** instrumentation, not a row-render callback.
- Interaction: input-to-visible-state p95 ≤ 50 ms for navigation and selection on local warm fixtures; report p99 separately and identify cold-storage stalls.
- Zero synchronous I/O or long decoding in the GPUI render callback; worker queues bounded with explicit cancellation and generation fencing.
- Directory corpus of 100,000 entries with measured memory ceiling, coherent sort/selection, no UI hangs; **not** implied by the existing 4,096-row virtualized evidence.
- 0 dropped/duplicated items through cancellation, watcher overflow/recovery and rapid location changes.
- No invented claims about Spacedrive benchmark performance or zero crashes from a single clean run.

## Exact next two engineering quests

**CABINET-UX-FOUNDATION-01 (first)**: read-only donor and comparator audit is now documented; next implement a *semantic token/role and workspace-routing prototype* in an isolated branch only after operator approval of the revised Files/Software information architecture. Test layout/focus and screenshot fixtures before migrating UI components.

**CABINET-FILES-PARITY-02 (second)**: actual Files-specific pathbar, grid/list interaction, focus and true file inspector, with deterministic screenshot+interaction matrix and p95 traces. This is still Files core refinement; Store remains read-only placeholder until parity gates close.

**Do not claim Spacedrive visual parity or Shelly-GPUI render parity until the operator has viewed matching reference and Cabinet states and signed off the matrix.**

## Execution amendment — UX-FOUNDATION-01 (2026-10-10)

The table above is retained as the dated **pre-foundation baseline**. The
operator-ratified isolated implementation now has the following **scoped**
evidence. It does not automatically promote a gate to full E4 acceptance.

| Gate | New evidence | Status |
| --- | --- | --- |
| **V01** | Files/Software navigation hierarchy, separate Locations/Software secondary rails; native F1–F4 and screenshot journey | **E3 scoped PASS**, full viewport/accessibility review OPEN |
| **V02** | Finite responsive geometry tests for 513/720/849/850/1100/1440/1920px; native 1100×820 capture | **E2 PASS** geometry, E4 matrix OPEN |
| **V08** | Single ListBox tab stop; Home/Down/Up/Enter native Wayland selection→folder→restart | **E3 two-item fixture PASS**, 4k/100k + AT-SPI OPEN |
| **V13** | First semantic dark roles and WCAG text contrast unit tests | **E2 dark baseline PASS**, light/DS parity OPEN |
| **V14** | Files-specific tabs, Software-only Discover/News/Transactions navigation; no catalog/installer authority | **E3 scoped PASS**, final product acceptance OPEN |

Engineering receipt: [CABINET-UX-FOUNDATION-01](CABINET_UX_FOUNDATION_01_20261010.md).
**V03/V04/V05/V06/V07/V09/V10/V11/V12/V15/V16 remain OPEN**; the
original Files-first freeze on unrelated feature development remains enforced.

## Execution amendment — FILES-PARITY-03 (2026-10-10)

This amendment reports **bounded** progress only; the original audit table
is a historical baseline, not retroactively rewritten.

| Gate | New qualified slice | Remaining gate |
| --- | --- | --- |
| V05 | Responsive 1–6-column Grid driven by actual canvas; E2 geometry and E3 visual oracle | Columns/Media and normalized reference parity OPEN |
| V06 | Genuine generated PNG thumbnail in GPUI tile; PNG decoder limits, malformed/symlink reject oracle | JPEG/WebP compositor proof, disk/VRAM budgets, broad MIME corpus OPEN |
| V07 | Cached image reused in inspector with scan-derived metadata; E3 click-to-select screenshot | Full MIME metadata, overflow, human acceptance OPEN |
| V08 | Memoized result indices protect frame render from repeated global name scans | 32768/100k keyboard/a11y and input-latency profiles OPEN |
| V15 | Isolated 32769-file scanner fixture, standalone 32768 cap, XDG chooser 4096 unchanged | 100k async paging, watcher overflow and frame-present performance OPEN |

Evidence and exact limitations:
[CABINET-FILES-PARITY-03](CABINET_FILES_PARITY03_20261010.md).
Store/Installer, renderer experiments and publication remain behind Files-first
acceptance.


## Execution amendment — FILES-PARITY-04 (2026-10-10)

The original reference table stays a dated baseline. P04 earns only
bounded E2/E3 proof; E4 is independently open.

| Gate | Bounded native evidence | Unqualified remainder |
| --- | --- | --- |
| V08 | Real 100k file list, keyboard End and selection in Weston | full large-data selection, accessibility and latency |
| V12/V15 | 2,048-entry progressive batches, bounded 100k worker, cancellation, background final sort, live kernel create/delete | true pagination, overflow stress, GPU presentation p95/p99 |
| V13/V16 | Recorded CPU UI-tree build sample distribution, real native screenshots | clipped breadcrumb, multi-DPI parity, reduced motion, operator E4 |

Full [P04 source and proof](CABINET_FILES_PARITY04_20261010.md).
