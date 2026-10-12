# CABINET-UX-TRUTH-01 — Shelly-GPUI donor audit and separation contract

**Date:** 2026-10-10
**Disposition:** SOURCE AUDIT / STRATEGY PROPOSED, **NOT IMPLEMENTED**
**Cabinet source:** `feat/cabinet-shelley-01-20261010 @ 72580a00240d75ae66cf47caa58b7934e374e6a4`
**Shelly-GPUI donor:** `main @ 6fce3747fa0e1254efd4a37d32ba1d29b6a708dd`, read-only; local uncommitted Render Lab files remain private and untouched
**Boundary:** no feature work, visual effect promotion, license change, push, merge, deployment or system portal mutation.

## Scope and source truth

The donor is **Shelly-GPUI**, unfinished package-workstation/store concept, not a completed file manager or finished App Store. Its Rust GPUI frontend sits atop a Zig/ALPM package-management backend. Its current GPUI crate declares `GPL-3.0-only`, `gpui 0.2.2`, and `blade-graphics =0.7.1`. Cabinet uses `gpui-pre 0.3.5`. Do not conflate its rendered source with a qualified direct dependency.

Read-only source census at audit: **72 Rust modules** in `Shelly.Ui.Gpui/src`, **15 Rust files** in `components/` (including `mod.rs`), **14 Render Lab Rust modules**, **85 GPUI Markdown methodology files**. Counts describe files, not qualified transferable primitives. The donor had modified `AGENTS.md` and untracked `RENDER-04-P2-FLICKER-01/02` evidence; these bytes and worktree were not modified.

High-value source documents:
- `docs/gpui/ux-workbench/01-PRODUCT-PRINCIPLES.md`: dense professional workstation, Results primary, Inspector secondary.
- `docs/gpui/ux-workbench/02-LAYOUT-AND-INSPECTOR.md`: explicit geometry, breakpoint, pinned inspector header.
- `docs/gpui/slice-05/03-VISUAL-SYSTEM.md`: semantic dark/light tokens, vector iconography, density.
- `docs/gpui/slice-05/04-INTERACTION-AND-FOCUS.md`: keyboard, focus, NO per-result tab stops in long virtual lists.
- `docs/gpui/slice-04/03-MOTION-ARCHITECTURE.md`: retargeting, owner-limited RAF, reduced motion.
- `docs/gpui/slice-06/06-CONCURRENCY-AND-PERFORMANCE.md`: concurrent search fanout, normalized caches, generation guard.
- `docs/gpui/ux-workbench/09-RUNTIME-EVIDENCE.md`: genuine Wayland screenshots of search, results, inspector, table mode, News and operation drawer.
- `docs/gpui/render-lab/{00-README,03-CAPABILITY-LEDGER,05-ACCEPTANCE}.md`: deterministic fixture/capability/promotion program.
- `docs/gpui/render-lab/20-RENDER-04-P2-FLICKER-01.md` and `21-RENDER-04-P2-FLICKER-02.md`: experimental failure evidence, NOT promoted.

## Donor primitives → Cabinet adaptation ledger

Classification: **A** = port rule/behavior after independent GPUI 0.3.5 implementation; **B** = adapt to domain-separated Store/News later; **C** = keep lab-only; **BLOCK** = cannot reuse without resolving boundary.

| ID | Shelly-GPUI source | Proven mechanism | Cabinet decision | Gate |
| --- | --- | --- | --- | --- |
| D01 | `views/package_workstation.rs` + UX layout-02 | exact splitter clamps, responsive stacked mode | **A** Files viewport/inspector with measured drag and no list occlusion | pointer E2E, narrow/wide screenshot |
| D02 | `ui_metrics.rs` + slice-05 visual-03 | shared density, pane, row and rail metrics | **A** semantic spacing/density tokens, no per-view magic numbers | geometry lint and 3 densities |
| D03 | `visual_style/roles.rs` | **13 surface roles** and 4 treatment classes, content protection | **A** Gnosix semantic role resolver, contrast-owned surfaces | all role/theme/contrast states |
| D04 | `visual_style/{resolver,projection,transparency}.rs` | style resolution separated from individual widgets | **A** role-owned effects policy, default opaque data planes | renderer/backend matrix; no leaking GPU state |
| D05 | `components/{sidebar,menu}.rs` | stable IDs, bounded menus, collapsed rail, anchored controls | **A** Files locations rail and context commands; software has separate nav subtree | focus, resize, hit zones, no double nav |
| D06 | `components/{package_table,package_card}.rs` | dense table/card metadata patterns, virtualized results | **A/B** abstract layout; **no package rows in Files** | file-specific rows + sorting, grid/list, keyboard |
| D07 | `components/inspector_header.rs` + UX-04 | pinned identity/action header; independently scrolling body | **A** file kind/size/path/metadata with secondary width limits | long metadata scroll, pinning, empty selection |
| D08 | `components/search_input.rs` | cursor/grapheme/UTF16/selection/IME model | **A after audit**; GPUI 0.2 API incompatible, prefer native GPUI 0.3.5 text input adaptation | compose, paste, multi-lingual, shortcuts |
| D09 | `components/{query_workbench,unified_search}.rs` | query intent separated from result state | **A/B** Files path-local and indexed search ≠ Store catalog lookup | mode labels, cancellation, query source |
| D10 | `state/{session,package_store,query}.rs` | typed events; generation fencing, source health, cache | **A/B** separate FileTabState and StoreCatalogState | stale response and tab-isolation tests |
| D11 | `state/motion.rs` + slice-04 | interruptible scalar and reduced-motion policy | **A** continue pure motion primitive with view-owned frames | mid-flight reversal frames, no idle loop |
| D12 | `components/toast_overlay.rs`, `state/toast.rs` | typed transient feedback lifecycle | **A** durable file-operation progress/error UI later | dismissal, failure visibility, no false success |
| D13 | `components/log_drawer.rs`, `state/console.rs` | progress/phases/logs/ANSI sanitization | **B** software installation console only, NOT default Files chrome | independent installer authority and terminal sanitization |
| D14 | `components/package_identity.rs` | icon provenance and fallback chain | **B** catalog/software identity; Files uses MIME/icon/thumbnail identity | attribution and safe external assets |
| D15 | `views/news.rs`, `backend/{client,protocol}.rs` | Arch/package announcement reading tied to source | **B** Software News subtree, separate untrusted RSS scope | safe fetch/render, no installer escalation |
| D16 | `control/{ipc,protocol,cli}.rs` | offline inspectable typed status protocol | **A** optional read-only desktop diagnostics for UX proof | no credential access, bounded IPC |
| D17 | `render_lab/{catalog,graph,recipe,ledger}.rs` | 46 deterministic fixtures and verdict ledger | **C** audit methodology; reuse concept not unqualified pixel effects | stock/composable/texture categories |
| D18 | `render_lab/{texture,experimental}.rs` | seeded noise, cached textures, experimental GPU seam | **C/BLOCK** no shader/texture import during Files parity | seam correctness and CPU/GPU replay |
| D19 | `views/settings.rs`, `config.rs` | draft vs committed configuration and theme | **A** well-defined global + per-surface preferences | migration, resets and defaults |
| D20 | `Shelly.Cli.Zig`, `Shelly.PackageManager`, package transactions | backend package authority and source adapters | **BLOCK for Files** / Installer later only | licence, consent, dry-run, policy, rollback |

## Evidence limitations (must not be erased)

The Render Lab ledger reports **46/46** canonical fixture evaluations: **12 NATIVE, 18 COMPOSABLE, 16 TEXTURE_PROOF**, NOT 46 production shader capabilities. `RENDER-04-P2-FLICKER-02` **establishes scissor-state leakage** in the experimental callback and a callback/Scene residency mismatch; it does NOT prove shader-visible flicker causality or safe production integration. No external Blade seam is authorized into Cabinet's render path by this audit.

The donor's `README.md` makes broad “120+ FPS” and multi-backend claims; do **not** transplant such claims without independent measurements of the exact chosen Cabinet build. Render Lab evidence and a live UX screenshot are not proof that every backend is healthy.

## Concrete current Cabinet audit, grounded in source and stored images

- `crates/in-gnu-os-filechooser-portal/src/bin/cabinet.rs` has a **duplicated route hierarchy**: top tabs “Files/Store/News/Installer” and identically repeated global sidebar. This blurs file locations versus software administration and burns the canvas.
- The Files list is currently only a single-column list with generic bullet/triangle labels; no proven Grid, Columns, Media, thumbnail pipeline, QuickPreview, breadcrumb/path edit, real query field or source mount layer.
- The File inspector is read-only and responsive with a splitter (SHELL-03) but has only a flattened metadata line; the richer pinned-header + independent scrolling body is not implemented.
- Some controls, notably every virtual row, use `tab_stop(true)`; this contradicts the donor's proven “arrow navigation, no hundreds of tab stops” rule and is a keyboard scalability concern.
- The app uses raw global color constants `BG/PANEL/BORDER/TEXT/MUTED/ORANGE` and repeated hexadecimal colors, rather than a full Gnosix semantic surface-role/light-dark token layer.
- Window geometry includes a literal `window.bounds().size.width - 216.` for main workspace content; the treatment of sidebar, gap, outer padding and compact rail must be measured, not asserted.
- The current FileChooser/Files scan remains limited to 4,096 directory entries and does not yet have incremental filesystem paging, watcher recovery or stale-data envelope.
- Store, News and Installer currently render *unavailable* placeholders; **that is truthful** but presenting them as equal primary views makes Cabinet feel like a launcher rather than a Files-first product.
- Evidence inspected: donor screenshot `docs/gpui/ux-workbench/evidence_ux02_07_package_selected_inspector.png` (1263x1302) vs Cabinet `/mnt/workbench/evidence/runs/cabinet-shell03-final-routes-20261010/files-start.png` (1100x820). **Not pixel-comparable:** different sizes, themes, datasets and domains. This is qualitative hierarchy evidence only.
- Both visuals have limitations: donor package cards show truncation/crowding in the historical screenshot; Cabinet's empty vertical space and duplicate navigation are obvious. Donor is engineering capital, **not** a visual gold standard to copy verbatim.

## Target product separation (PROPOSED, pending owner UX acceptance)

**Cabinet is Files-first.** At normal startup it opens an actual Files workspace; no file toolbar action should silently navigate to a package catalog.

- **Global rail:** `Files` (primary, selected by default), `Software` (secondary destination), `Settings` (global). Developer Render Lab is tool-only, **not** a sidebar feature.
- **Files route:** a second-level Locations rail (Home, Projects, drives, bookmarks), breadcrumb and scope-aware search, one primary file canvas (grid/list/columns/media), optional inspector/preview, discreet job/activity strip. Multiple Files tabs store independent folder/history/view/selection/scroll settings.
- **Software route:** independent Software workspace with `Discover (Store)`, `Installed`, `Updates`, `News`, `Transactions`. Catalog cards and package-origin indicators belong here, not among actual filesystem items. News derives from attributed feeds; no automatic execution or install permission.
- **Installer:** not a root surface equal to Files. Transaction planner/consent/progress is invoked from Software and guarded by a separate capability boundary. Product UX must show dry-run, source and privilege BEFORE any mutation.
- **Cross-link:** “Reveal installed files” may open a Files route through an explicitly typed path; “Install from URL/file” never follows from filesystem selection without a new capability/consent.
- **State and preferences:** global app shell state separate from `FilesWorkspaceState`, `SoftwareWorkspaceState` and per-tab state. No mixing package identifiers with filesystem paths.

**No visual effects or Store backend implementation until the separate [Files-first parity contract](CABINET_FILES_VISUAL_PARITY_20261010.md) closes.**

## Publication boundary

The source audit is documentation-only: Shelly-GPUI bytes stay private and its in-progress untracked experiments untouched. Reimplementation may require copyright/licensing advice and attribution. The distinct MIT worker retains all relicensing authority. Neither GPL-derived code nor visual assets may be published by implication from this audit.
