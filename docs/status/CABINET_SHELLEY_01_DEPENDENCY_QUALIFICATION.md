# CABINET-SHELLEY-01 — Dependency and native-component qualification

Date: 2026-10-10
Target branch: feat/cabinet-shelley-01-20261010 (isolated worktree).
Gate: PARTIAL — selected geometry/inspector port qualified locally, NOT a full Shelley runtime release.

## Source truth and boundaries

- Donor: shelly-gpui main at 6fce3747fa0e1254efd4a37d32ba1d29b6a708dd.
  Reference: Shelly.Ui.Gpui/src/views/package_workstation.rs,
  Shelly.Ui.Gpui/src/ui_metrics.rs and Shelly.Ui.Gpui/src/state/motion.rs.
- Target: tension-atoi/gnosix Cabinet baseline 92193f9ade9c736f02752723d38658ca33ffcbcb.
- Donor main has uncommitted research and altered AGENTS.md. Remained read-only;
  nothing was reset, published, or copied wholesale.
- Target lives in its own worktree. No host FileChooser, Hyprland configuration,
  system package installation or implicit privileges were changed.

## Explicit engine incompatibilities

| Axis | Shelley GPUI | Cabinet / Gnosix |
| --- | --- | --- |
| UI | gpui 0.2.2 | gpui-pre 0.3.5, gpui-pre-platform 0.3.5 |
| GPU | blade-graphics 0.7.1 | WGPU 29.0.4 / 30.0.1 in workspace lock |
| Shader stack | naga 25.0.1 | naga 29.0.4 / 30.0.1 |
| Linux implementation | Shelley GPUI backend | GPUI-pre Wayland, local patched gpui-pre-linux 0.3.5 |
| License | GPL-3.0-only | GPL-3.0-or-later workspace |

These are different framework/renderer implementations. Semver dependency
unification, blind crate replacement, and copying old GPUI view types are NOT
qualified migrations.

## Lock dependency inventory

Reproduction tool: tools/cabinet_shelley_dependency_audit.py.
Evidence: /mnt/workbench/evidence/runs/cabinet-shelley-01-dependency-ledger-20261010/lock-comparison.json

- Donor lock: 740 package records, 649 distinct names.
  SHA256 2ae96bb3e2ad0a63268c220414e1daf43e0e6b5d36fc067e9d95631a0f107399.
- Target lock: 742 package records, 663 distinct names.
  SHA256 52d8344e203b4a10193126d4d2d8a6ebdeb71fa96a5ae3736a08e040c58b5712.
- Shared names with identical version sets: 417.
- Shared names with different version sets: 96.
- Names only in donor: 136; only in target: 150.

These numbers are inventory statistics, not API/ABI or security approval.
The target Cargo.lock is unchanged.

Donor cargo metadata --offline --locked --no-deps: PASS.
Full donor cargo tree --offline --locked -p gpui@0.2.2: BLOCKED by missing
ar_archive_writer 0.5.3 in the local Cargo cache; not a claim of bad sources.
Target cargo tree --offline --locked -p gpui-pre@0.3.5 --depth 1: PASS.
cargo-audit and cargo-deny are absent; no vulnerability/advisory clearance claimed.

Native pkg-config inventory of reference workstation:
Wayland client/server 1.26.0; xkbcommon 1.13.2; libdrm 2.4.134;
GBM 26.2.4-arch3.1; Vulkan 1.4.363; libinput 1.32.0; libudev 262.
Observation only: no all-vendor distribution certification.

## Adapted GPL model and live native component

Cabinet-specific geometry in cabinet_workbench.rs reimplements bounded
invariants from the Shelley workstation without gpui 0.2.2 or Blade imports.

- Pointer delta / clamping, finite-value validation, 340px min list width.
- Minimum 300px file inspector with 55% content cap, rather than the
  unsuitable 320px / 3:7 package-manager ratio.
- Stacked fallback on narrow windows and explicit width accounting.
- A 16px horizontal gap overflow was removed from the split calculation.
- File size and modification timestamp are cached in the asynchronous
  directory enumeration, not recomputed in render or drag events.
- Native GPUI-pre host draws the list, active draggable divider and
  a metadata inspector responding to selection. No file mutation added.

## Qualification receipts

- 40/40 Rust tests PASS, including range, reversal, breakpoints, invalid
  input, 70px practical drag range and cached metadata.
- Native GPUI build PASS; all-target Clippy strict -D warnings PASS.
  /mnt/workbench/evidence/runs/cabinet-shelley-01-refined-build-20261010/
- Real isolated Weston/Xvfb E2E divider geometry from x=721 to x=654:
  delta -67px, at least 50px required, PASS. Wayland pointer buttons and
  Cancel response checked, before/after PNGs retained.
  /mnt/workbench/evidence/runs/cabinet-shelley-01-canonical-splitter-v2-20261010/
- Nine existing independently launched Wayland E2E cases PASS:
  cancel, save, open-keyboard, open-tab, open-shift-tab, open-pointer,
  ux-hidden, ux-history, ux-search.
  /mnt/workbench/evidence/runs/cabinet-shelley-01-regression-{mode}-20261010/
- Complete repository CI: 48/48 PASS, 0 non-PASS. Receipt:
  /mnt/workbench/evidence/runs/cabinet-shelley-01-full-ci-20261010/

The failed early splitter gate is retained: Shelley package ratios allowed
only 24px of movement on Cabinet. The screenshot oracle was also hardened
against one-point RGB rasterization variance and pointer-hover occlusion.

## License/provenance gate — NOT AUTHORIZED FOR PUBLIC PUSH

Shelley declares GPL-3.0-only and lists Tension Atoi and Seafoam Labs.
Gnosix workspace declares GPL-3.0-or-later. Although GPL version 3 can
provide a common distribution version, contributor rights, attribution
and the combined-work distribution policy require explicit review.
Repository control does not prove sole copyright for every donor primitive.

No original Shelley component file was copied verbatim. The target is a
framework-specific implementation of documented geometry constraints.
Do not merge, push publicly, install, or promote this branch until the
provenance and licensing gate is explicitly closed.

## Remaining gates

1. Independently acquire and clean-build Shelley's locked GPUI 0.2.2/Blade
   baseline outside its dirty working tree.
2. Review GPL provenance, contributor permissions and public distribution.
3. Adapt the motion policy and virtualized lists; qualify accessibility,
   keyboard splitter control and real frame-time budgets.
4. Keep package management behind independent typed authority/grants.
   A FileChooser does not acquire package-manager privileges by sharing a UI.
5. Review the new surface on actual Hyprland, multiple window sizes, and
   Chromium/Flatpak frontend integration separately.

No UI-level or unit-test PASS may be promoted into an unrelated runtime gate.
