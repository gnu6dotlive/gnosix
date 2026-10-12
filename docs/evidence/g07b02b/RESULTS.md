# G07B-02B — Paired Wayland Input-Focus Qualification
12 October 2026 · private experiment · **bounded A/B PASS** · production gate **BLOCKED**

## Goal and provenance
Confirm whether Hyprland 0.56.2 commits dynamic `wl_surface.set_input_region` without recomputing pointer focus while the cursor is stationary, and isolate the effect of the candidate compositor fix. Source version: `efb50993780079460b0cbed1363e2166a2de1d9f`. Original G07B-01 observation: `3b8349a`. Candidate patch: `docs/evidence/g07b02/hyprland-pointer-focus-probe.patch`; fixture branch commit: `e8f1992`.

The patched and stock Hyprland binaries were compiled from the **same checkout, compiler and CMake build directory** by applying then reversing the two-file patch. Their SHA-256 digests are recorded in `receipt.json`. This is not an A/B of unlike distribution packages.

## Environment
Two independent native GPUI layer-shell processes: a bottom-positioned full-input witness and an overlapping shaped-input Dock atlas. Both surfaces were 900 × 560 on a nested `WAYLAND-1` output. The outer desktop and nested compositor were separate; the nested compositor listened on `wayland-2`. The nested config used `preferred`, rather than the incorrectly forced 2560×1440 mode that previously led to GBM 1920×1080 allocation failures. No package or production compositor was replaced.

## Observed results

| Case | Stock v0.56.2 | Candidate patch |
|---|---|---|
| Initial surface input region expands over stationary cursor | **FAIL:** witness receives click (2 standalone reproductions; 1 more in reverse scenario) | **PASS:** overlay receives click (2 reproductions, including reverse scenario) |
| Explicit positive-control focus prime via +4/−4 mouse movement | **PASS:** overlay receives click | **PASS:** overlay receives click |
| Committed region then excludes stationary cursor; no motion afterward | **FAIL:** overlay incorrectly retains click | **PASS:** witness receives click |

The initial paired run declared the *same exact rectangles*: `[30,392,870,496]` and `[0,496,900,501.3]`, with test point `(450,460)`. The reverse paired run declared hidden region `[0,496,900,501.3]`, which excludes that point. Actual clients independently logged `wl_pointer.enter`, `leave`, and `button`; every test compared independent click counters. Reached physical pointer positions and post-test restoration are recorded in each observation.

## Source-level mechanism and experimental fix
`CWLSurfaceResource::commitState` applies the changed `SSurfaceState.input`; `CLayerSurface::onCommit` did not request focus re-evaluation specifically for those input-only commits. `CInputManager::mouseMoveUnified` avoids recomputing when the pointer has not changed position. The candidate patch tracks committed input region membership at the current pointer point and invokes the compositor's `simulateMouseMovement()` **only on transitions**. It also checks the initially mapped layer. This is compositor-side focus recomputation, not a GPUI-induced physical cursor nudge.

## Receipts and negatives
`receipt.json` records every accepted case, hash, geometry, relevant Wayland protocol events, and raw log hashes. Compressed original logs are in `traces/*.tar.zst`; scripts and isolated compositor config are retained alongside them. A **sixth attempted run was rejected at preflight** after the outer tiling compositor resized the nested window to less than the witness's 900-pixel width. No click was injected and this attempt is excluded from all PASS/FAIL counts.

## Non-claims and next gate
The **bounded causal A/B gate is PASS**; the original 16-case dynamic scheduler matrix, hardware/display generalization, pointer grabs, races and upstream acceptance are not qualified. G07-B dynamic and runtime promotion remain **BLOCKED**. The candidate Hyprland patch is not a production-shippable upstream fix yet.

**Next — G07B-02C:** hold nested output geometry stable independently of parent tiling; repeat expansion/retraction across multiple positions, successive region commits and actual Dock motion; qualify grabs, mapping/unmapping, double-click and untouched regions; rerun the original 16-case dynamic matrix; collect compositor-side transition counters and separately review whether to upstream the minimal compositor fix. No system compositor replacement before those gates.
