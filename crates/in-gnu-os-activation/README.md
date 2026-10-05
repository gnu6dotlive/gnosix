# in.gnu.os activation contract core

This crate is the pure, renderer-independent R13-C request and admission
contract. Its current executable vocabulary is deliberately limited to:

- activate one exact, already-observed current-session `WorkspaceId`;
- focus one exact, already-observed `SurfaceId` whose role is `Application`.

Every request carries a correlation ID and a freshness fence consisting of an
ephemeral observer-session epoch plus `DesktopContextGeneration`. Admission
compares both values to the supplied authoritative observation before checking
the target. A stale request is refused; it is never refreshed or retargeted.
Application targets are matched only by `SurfaceId`; class, title, PID, focus
order, connector, and geometry do not resolve targets.

## Capabilities and authority boundary

Workspace activation and application focus use separate capabilities. Each
capability is scoped to one exact target ID. The capability types have private
fields and no public constructor, widening operation, or conversion from R11
identity-writer or R12 profile-writer capabilities. The only production issuer
seam returns an admitted request rather than the capability itself. This source
core therefore preserves the Rust API/type boundary and deterministic admission.
It does not authenticate a process or prove that a caller named
`in.gnu.os` is trusted. Process authentication remains the responsibility of
the separately ratified production host boundary.

The activation crate's private capability fields can only be initialized by
code inside the defining crate module tree. The opt-in
`r13-production-issuer` feature now exposes one narrow host seam,
`admit_authenticated_gnos_ux`, which never returns a capability: it mints the
exact operation-scoped capability internally from an already authenticated
typed operation, binds the request to the authority's own current observation,
and returns only `AdmittedActivation`. Rust module privacy is a source/API
boundary, not process authentication.

`ActivationSource::GnosUx` records request provenance; it is not a credential.
The public request constructor does not provide activation authority. A
`CurrentActivationObservation` is supplied by the in.gnu.os host/observer
adapter; the pure core does not query a compositor or refresh observations.

## Outcomes and exclusions

Admission receipts retain request ID, source, exact freshness, operation, and
the admitted/refused decision. R13-01E adds a pure bounded execution state
machine and a typed receipt for precheck, native attempt, and exact
postcondition. The pure state machine does not itself grant native authority.
Runtime process authentication, hosting and IPC are implemented by the
separately feature-gated gnosix-hyprland production host boundary. Production
capability issuance is source-qualified through `r13-production-issuer`.
The authenticated D-Bus host now connects fresh authority-local admission,
fresh revalidation and the existing bounded executor in source, with a bounded
non-evicting completed-request ledger scoped to one host epoch. The GNOS.UX
transport client and its local semantic-generation send fence are also
source-qualified without depending on this authority crate. The exact
`gnos-ux-scene.service` caller has also been live-qualified through the
production boundary to `AlreadySatisfied` for an already-active workspace.
A typed cross-process workspace activation has now been live-qualified through
the production boundary, including exact postcondition and round-trip restore.
Application-focus live qualification and persistent deployment policy remain
separate concerns.

Application launch is absent because no ratified registered-application
identity exists. A pinned App Card's launch identity must remain distinct from
an observed `SurfaceId`; do not route a launch through focus. Preserve the
future distinction between launching a new instance at a desktop destination,
launching into a non-active workspace, and separately deciding whether to
follow/switch to that workspace. Active windows belong primarily to workspace
semantics; workspace visual slots are distinct from logical workspace IDs.
A move to a full workspace destination must eventually require explicit
resolution rather than silently selecting destructive behavior.

The design intent also preserves a continuous Deck-compact to App-Stack
transformation, allows per-output shell compositions to differ, and keeps
Theme Plane, Material Plane, Living Surface Fabric, and compositor-reserved
space as separate concerns. Workspace move, merge, save, launch-into-workspace,
follow-after-launch, Deck/App Stack, Theme/Material Plane, Gnoshow, and GNOS.UX
execution authority are outside this slice. This contract leaves those future
verbs separate rather than representing them as generic `Move` or
command-string operations.

There is no `RawCommand`, Hyprland dispatch string, process spawn, daemon, or
external endpoint in the public vocabulary or dependency graph.

## R13-01C: current Hyprland representability

`gnosix-hyprland` now exposes a read-only `DesktopExecutionBindingSnapshot`
alongside one stable, double-sampled semantic observation. It retains the
Hyprland runtime session signature and the resulting
`DesktopContextGeneration`; it is an ephemeral observer sidecar, not durable
identity and not execution authority.

An observed `WorkspaceId` is derived from the current Hyprland session
signature and the numeric workspace ID. The sidecar preserves that exact
numeric ID. An observed application `SurfaceId` is derived from the same
session signature and Hyprland `stableId` when the field is available; the
sidecar maps it to that exact stable selector. Hyprland 0.56.2's `j/clients`
JSON includes `stableId`, assigns it from a session-local monotone counter,
and supports exact `stableid:` window selection. Workspace selectors can use
the exact workspace ID or workspace object. See the
[Hyprland 0.56.2 window JSON source](https://github.com/hyprwm/Hyprland/blob/v0.56.2/src/debug/HyprCtl.cpp#L345-L403),
[window stable-ID counter](https://github.com/hyprwm/Hyprland/blob/v0.56.2/src/desktop/view/Window.cpp#L77-L79) and [per-window counter assignment](https://github.com/hyprwm/Hyprland/blob/v0.56.2/src/desktop/view/Window.cpp#L170-L184),
[exact selector documentation](https://wiki.hypr.land/configuring/naming-conventions/#window-selector),
and [workspace focus dispatcher documentation](https://wiki.hypr.land/configuring/core/dispatchers/).

The semantic observer still accepts incomplete/older client observations using
its existing address-derived `SurfaceId` fallback, but it emits no execution
binding for a client without `stableId`. A duplicate or malformed stable ID
fails closed. When a stable ID is available, replacing a client at the same
address changes its semantic `SurfaceId` and advances the context generation.

This qualifies source representability only. The R13-01C receipt predates the
source-bound freshness and execution work below.

## R13-01D: source-bound pre-execution revalidation

`HyprctlLuaBackend::from_env` captures `HYPRLAND_INSTANCE_SIGNATURE`; the
semantic query addresses that same instance through `hyprctl -i`. The resulting
`DesktopExecutionBindingSnapshot` retains the exact signature, semantic
generation and semantic digest from one stable double-sampled observation.
Its admission and execution views require a baseline with both the same
generation **and** the same digest. A baseline from another observation is
refused even when its generation number coincides.

The in.gnu.os observer epoch remains a separate ephemeral value supplied by
the future authority adapter. The Hyprland sidecar pairs that value with its
actual observed instance signature when it creates the two views. An admitted
activation records the session signature supplied at admission. Pure
`revalidate_for_execution` compares the admission epoch and original
generation, both observed instance signatures, target existence/kind and the
exact native binding before issuing a `CurrentExecutionTarget`. Duplicate or
malformed native identifiers and absent bindings fail closed. Descriptive
attributes never participate in target resolution.

An unrelated semantic change may advance the global generation. Revalidation
can issue a **new** target for the newer observation only after proving that
the selected semantic target and native binding still match exactly. The
original admission freshness stays unchanged and is retained in the result;
the old observation token is not treated as current. A changed observer epoch,
instance signature, target or binding is refused. Admission through the
legacy session-unbound pure constructor remains possible for 01A/B policy
tests, but cannot produce a current execution target.

This is a source/API and observer contract, not a production issuer or
process-authentication proof. `ExecutionObservation::new` constructs policy
input; it grants no capability. No compositor action or postcondition is
implemented in 01D. `CurrentExecutionTarget` describes the instant of its
source observation; it is not an atomic lease on compositor state.

## R13-01E: bounded execution source contract

`execute_bounded` accepts only a `CurrentExecutionTarget`. It obtains a new
coherent execution observation, revalidates the original observer epoch,
Hyprland session, exact semantic target and native binding, and returns
`AlreadySatisfied` without mutation if that exact target is already focused.
Otherwise it issues at most one typed attempt and obtains a fresh observation.
Only an accepted attempt followed by focus on the **same** semantic target and
native binding becomes `Applied`. Backend rejection, uncertain transport,
postcondition mismatch, target disappearance, session change and observation
failure have distinct outcomes. The receipt retains the original admission
fence and both execution observation generations.

The `gnosix-hyprland` adapter has no public constructor. Its source-qualified
translation emits only `hl.dispatch(hl.dsp.focus({ workspace = "N" }))` for a
positive bounded numeric workspace ID or
`hl.dispatch(hl.dsp.focus({ window = "stableid:HEX" }))` for an exact stable
window ID. An unsupported special/negative workspace ID is rejected rather
than guessed. This follows [Hyprland's Lua dispatcher contract](https://wiki.hypr.land/configuring/core/dispatchers/)
and [hyprctl eval contract](https://wiki.hypr.land/configuring/core/advanced-configuration/using-hyprctl/).
The adapter uses the same double-sampled semantic/native observer before and
after the attempt. No title/class/PID/address lookup or raw product command is
accepted.

R13-01E was **source-qualified, not live native-qualified**. Its receipt did
not use a Hyprland mutation to manufacture a PASS. The later, separate
R13-01F qualification below exercises both native operations locally. The
remaining observation-to-action interval is bounded but not compositor-atomic.
Source/API privacy is not process authentication.

## R13-01F local qualification seam

The non-default `r13-live-qualification` feature exists solely for an ignored,
local test in `gnosix-hyprland`. In a debug qualification build, the activation
crate admits a typed request with a private exact-target capability and returns
only `AdmittedActivation`; it never returns the capability itself. The feature
is rejected in release builds. The test drives the real observer, sidecar,
admission, revalidation and bounded adapter, then records exact postconditions
and separate typed restoration. It accepts no command or descriptive target
string. Ordinary GNOS.UX/Gnoshow builds do not enable this seam.

This is deliberately **not** a production issuer or process-authentication
mechanism. A downstream crate deliberately enabling the debug qualification
feature is outside the ordinary source/API boundary; production hosting and
caller authority still require a separate R13/R14 decision and proof.

The source-bound local qualification at `dd854b4755b770553b4885845ba8a02b5d2f0592`
used the exact [typed live receipt](../../evidence/runs/gnosix-r13-01f-live-activation-qualification.json)
and [raw runtime log](../../evidence/logs/gnosix-r13-01f-live-activation.log).
The already-satisfied workspace control issued zero native attempts. An exact
observed workspace changed from native ID 11 to 1 and was restored to 11
through a second typed request. An exact observed application changed focus
from stable ID `18000002` to `18000005` and was restored to `18000002` through
the same pipeline. Each attempted action had a fresh exact semantic
postcondition; each restoration returned the semantic digest to its initial
value. The session signature remained stable. The receipt identifies the test
binary by SHA-256 and correlates selection, admission, revalidation, native
attempt and postcondition by request ID. No manual `hyprctl` mutation was used.

The normal-feature external consumer cannot import the qualification issuer,
and a release build with the feature fails compilation. Those gates are
source/API containment only. R13-01F closes local native typed execution; R13
production activation remains open for a named issuer, production host,
caller/process authority, lifecycle, and any selected transport. The final
precheck-to-compositor-action interval is still **not proven atomic**.

Gnoshow is Gnosix's wallpaper manager and rear-plane renderer. It shares
output/profile/lifecycle contracts where applicable and has no activation
capability issuance or execution role.
