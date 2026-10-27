# GNOS.UX GPUI — scoped engineering constraints

Applies after the parent .claude/CLAUDE.md and AGENTS.md. The source of
authority is ARCHITECTURE.md, AUTHORITY.md and existing typed contracts.

- gnos-ux-scene is the canonical product process/scene; standalone Bar
  and Dock are qualification tools, NOT parallel production observers.
- Preserve a single source of truth for semantics, output-local identity,
  topology, geometry, input, reservation and motion. SceneFrame should
  derive all coupled render/hit/reservation facts from the same layout.
- Provider adapters emit typed read models; renderers must not import raw
  D-Bus/Hyprland protocol objects, invent facts when unavailable or
  execute unsafe host commands from UI widgets.
- Keep product defaults distinct from feature-gated component-lab,
  donor reference recipes and exploratory Gnosix sandbox behavior.
  Caelestia observable fidelity is tested through a selectable profile,
  not by specializing the platform's global geometry engine.
- Real layer-shell pixels, blur capability, multi-output reservation,
  input/focus and teardown need compositor evidence; pure Rust tests
  establish only scoped model behavior. Operator qualifies appearance.
- Use a bounded per-surface action/recovery contract; no implicit writes,
  focus hijacking, resource leaks, global pointer listener or stale
  session output IDs. Handle absence and provider replacement explicitly.
- Run cargo fmt --all -- --check and relevant cargo test / clippy with
  and without component-lab where features cross paths. Prefer the
  canonical cargo xtask local verification for integrated changes.
  Never massage assertions merely to claim visual success.
- Reuse existing reference_bar, product_scene, product_shell_material,
  product_shell_blur, provider and DONOR/PARITY receipts before writing
  duplicate components. Preserve exact known defects and status.
