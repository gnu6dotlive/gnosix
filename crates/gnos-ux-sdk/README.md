# GNOS.UX Surface SDK

Status: **internal / provisional**

`gnos-ux-sdk` is the current development façade over the GNOS.UX surface
model, GPUI runtime, geometry/debug instrumentation and scenario lab.

It exists to make proven surface work reusable without collapsing model,
runtime and development tooling into one crate.

## Stack

```text
gnos-ux-surface
  backend-agnostic model
  Envelope contracts
  SurfaceRole
  surface grammar
    atoms → molecules → recipes → resolved frames
  MonitorScene / SceneFrame kernel
       │
       ▼
gnos-ux-gpui
  GPUI / Wayland runtime
  persistent per-monitor scene host
  product surface plans
  Hyprland observation adapter
  frame → paint/input/debug/telemetry adapters
       │
       ▼
gnos-ux-sdk
  façade + grammar catalogue
  scene runtime façade
  surface catalogue
  scenario catalogue
  launch planning
  gnos-ux-lab
```

The SDK is not yet a stable public ABI.

## Current surface catalogue

Run:

```bash
gnos-ux-lab surfaces
```

Current expected status:

```text
Bar      runtime=GPUI_PRODUCT  atlas=ATLAS_RICH
Dock     runtime=GPUI_PRODUCT  atlas=ATLAS_RICH
Rail     runtime=PROBE_ONLY    atlas=PROBE_ONLY
Overlay  runtime=PROBE_ONLY    atlas=PROBE_ONLY
```

Rail and Overlay are deliberately not promoted to runnable product targets.

## Lab commands

```bash
gnos-ux-lab list
gnos-ux-lab scene
gnos-ux-lab surfaces
gnos-ux-lab grammar
gnos-ux-lab dynamics
gnos-ux-lab coverage

gnos-ux-lab describe envelope
gnos-ux-lab describe dock autohide

gnos-ux-lab plan envelope detach-relax
gnos-ux-lab run envelope geometry
gnos-ux-lab verify envelope detach-relax

gnos-ux-lab run bar responsive
gnos-ux-lab verify bar morph

gnos-ux-lab run dock responsive
gnos-ux-lab verify dock autohide
```

### `run`

`run` is interactive. It launches the existing qualifier through the proven
nested-Hyprland harness and stays alive until the operator closes it.

### `verify`

`verify` is finite and evidence-oriented. It is available only for scenarios
that already publish a deterministic completion marker.

The SDK provides the scenario's required markers to the generic nested harness.
The harness returns PASS only when:

1. the candidate maps in nested Hyprland;
2. frame health passes;
3. the deterministic completion marker is observed;
4. every required proof marker declared by the scenario exists in the candidate
   log.

There is no hidden wall-clock timeout used as scenario success criteria.

## Initial scenario catalogue

| Target | Scenario | Purpose | Mode |
| --- | --- | --- | --- |
| Scene | `desktop` | persistent single-window desktop scene integrating Bar, Body, Dock, relations, resize and influence | deterministic |
| Envelope | `geometry` | interactive hull/topology/debug inspection | interactive |
| Envelope | `detach-relax` | Fused/Bridged/Detached release + reattach cycle | deterministic |
| Bar | `responsive` | compositor-sized Bar geometry inspection | interactive |
| Bar | `morph` | Standard/Hug geometry morph | deterministic |
| Dock | `responsive` | compositor-sized Dock/input inspection | interactive |
| Dock | `autohide` | visible/hidden/reveal-input cycle | deterministic |

Scenario descriptors declare binary, args, namespace, capabilities, required
markers and completion semantics. The runner orchestrates existing qualifiers;
it does not duplicate their solver/render logic.

## SDK namespaces

### `gnos_ux_sdk::model`

Re-exports the backend-agnostic surface and Envelope contracts.

### `gnos_ux_sdk::runtime`

Exposes product-surface plans and the Hyprland observation adapter.

Use `try_surface_plan` when probing support. It returns `None` for roles that
do not have a GPUI product runtime instead of turning a declared role into an
implicit product.

### `gnos_ux_sdk::grammar`

Re-exports the backend-independent surface grammar and publishes a development
catalogue of its foundational layers.

The current chain is:

```text
atoms
  SurfaceViewport
  UnitInterval
  ResponsiveLength / LengthBasis
  GeometryRect
  Point2 / Vector2
      ↓
molecules
  RectRecipe
  visible_from_bottom
  reveal_strip_below
  visible_plus_reveal_input
  envelope_only_input

  CubicBezier
  CubicJoinPair
  g1_transition_controls
  monotonic_join_morph
  CurveValidation

  ConnectorSpan
  BridgeShoulderRecipe

  SecondOrderSpec
  SecondOrderChannel
  PhaseChannel
  SettlePolicy
  TemporalResponseMode
  DynamicsDiagnostics

  InfluenceSample
  RectInfluenceField
      ↓
recipes
  DockGeometryRecipe
  BarGeometryRecipe
      ↓
resolved frames
  DockGeometryFrame
  BarGeometryFrame
```

Recipes carry a stable internal `RecipeIdentity { id, revision }`. Resolved
frames validate their own structural invariants and are the geometry payload
consumed by paint, input, debug and telemetry.

The reusable curved/connective sub-grammars also carry explicit internal
identity:

```text
gnos.ux.grammar.curves     @ 1
gnos.ux.grammar.connectors @ 1
gnos.ux.grammar.dynamics   @ 1
gnos.ux.grammar.influences @ 1
```

The Reactive Envelope consumes these implementations for cubic evaluation,
subsegments/splits, G1 controls, monotonic Fused→Bridged join morphing,
directional validation, connector spans and shoulder runs. Its remaining tuple
types are GPUI/path adapters rather than independent curve algorithms.

SDK 4 also moves the generic temporal response and neighbor soft-influence
mechanisms into the grammar. The Reactive Envelope keeps product targets,
tuning values, hard constraints and Fused/Bridged/Detached decisions; it now
delegates scalar second-order stepping, normalized phase progression and
neighbor sampling to the shared grammar.

The intended boundary is explicit:

```text
Influence   = advisory vector / pressure
Constraint  = hard admissibility / non-penetration
Topology    = semantic relationship authority
```

Only Influence is generalized by the current SDK foundation.

`gnos-ux-lab grammar` prints the current catalogue and sub-grammar revisions.
`gnos-ux-lab dynamics` prints the qualified 30/60/120 Hz and bounded
variable-dt step-response diagnostics from the shared dynamics implementation.

### `gnos_ux_sdk::geometry`

Provides convenient SDK re-exports for the grammar geometry types plus the
GPUI-side debug/Atlas vocabulary. The geometry primitives themselves live in
`gnos-ux-surface::grammar`; GPUI no longer owns duplicate
`SurfaceViewport`, `GeometryRect` or Dock geometry implementations.

### `gnos_ux_sdk::dynamics`

Re-exports the backend-independent temporal response vocabulary:

- `SecondOrderSpec`;
- `SecondOrderChannel`;
- `PhaseChannel`;
- `SettlePolicy`;
- `TemporalResponseMode`;
- `DynamicsDiagnostics`.

The current integrator is the qualified semi-implicit response already used by
the Reactive Envelope. The SDK does not claim unconditional stability for
arbitrary response/damping/dt values.

### `gnos_ux_sdk::influences`

Re-exports `InfluenceSample` and `RectInfluenceField`.

These are soft/advisory spatial primitives. They do not perform collision
resolution and do not change topology state.

### `gnos_ux_sdk::scene`

Re-exports the backend-independent `MonitorScene` / `SceneFrame` kernel and
the canonical persistent GPUI scene identity/namespace.

The current integrated host resolves Bar, Body and Dock into one frame and
renders them inside one visible layer-shell surface. A visual scene node is not
equivalent to a Wayland surface.

`gnos-ux-lab scene` launches the actual scene runtime through the nested
qualification harness; it does not contain a second scene solver.

### `gnos_ux_sdk::surface`

Provides the current surface catalogue, combining canonical identity,
namespace, runtime availability, Atlas coverage and scenario count.

### `gnos_ux_sdk::lab`

Provides the scenario catalogue and launch-plan model used by
`gnos-ux-lab`.

## Geometry/debug invariant

The development environment preserves the current GNOS.UX rule:

```text
resolved geometry / solver
          │
          ▼
      one frame
   ┌──────┼────────┬──────────┬────────────┐
   ▼      ▼        ▼          ▼            ▼
 paint   input   overlay   telemetry   assertions
```

Debug tooling must consume the geometry used by the runtime surface rather than
reconstructing a visually similar shape independently.

## Hyprland / GNOSIX authority boundary

Hyprland owns compositor runtime reality and execution semantics.

GNOSIX owns Hyprland compositor integration and has authority to modify
Hyprland configuration/state through that integration.

GNOS.UX owns visual scene composition and emits typed compositor-facing intent. SDK6 establishes the GNOSIX projection boundary: surface-local state uses native layer-shell, while admitted session-only Hyprland-global mutations use the typed GNOSIX runtime-Lua adapter with fencing, observation and rollback.

SDK7/8 add explicit current-session output binding and dynamic topology reconciliation; SDK9 adds in-process lifecycle/readiness/health semantics and graceful signal shutdown. None of these SDKs establishes persistent configuration, durable output identity, or a production process host/lifecycle owner.

The nested compositor harness remains qualification infrastructure rather than a competing compositor authority.

## Current runtime adoption

The grammar is not documentation-only:

- the Dock Atlas qualifier resolves through `DockGeometryRecipe`;
- the legacy Dock path and Reactive Envelope base Dock use the same Dock recipe;
- the Bar qualifier resolves paint/input/debug geometry through
  `BarGeometryRecipe`;
- previous local Bar/Dock formulas were removed from those runtime call sites;
- Reactive Envelope scalar dynamics use the shared second-order stepper;
- join/release/reattach progression uses the shared `PhaseChannel`;
- neighbor soft feedback samples the shared `RectInfluenceField`;
- `envelope.influence` is a deterministic lab scenario that requires a real
  non-zero influence sample before PASS;
- `MonitorScene` resolves Bar, Body, Dock, connector relation, input regions
  and reservation intent into one `SceneFrame`;
- `gnos-ux-scene` renders that frame in one persistent visible layer-shell
  surface and survives compositor-driven viewport rebases;
- `scene.desktop` proves stable process/scene/host identity through the
  integrated deterministic desktop cycle.

The current recipe constants reproduce the previously proven geometry. They are
foundation baselines, not a ratification of final product dimensions.

## Out of scope for the current SDK foundation

- stable public ABI;
- plugin loading;
- persistent scenario configuration;
- Rail/Overlay product runtime;
- final product geometry or material defaults;
- production process host/lifecycle ownership and restart policy;
- persistent configuration/profile authority;
- durable physical-output identity/matching and OutputHub persistence/writer authority;
- notification service ownership;
- plugin loading/ABI/sandboxing;
- Shelley-GUI / Render Lab integration.
