# gnosix architecture

## System model

gnosix separates semantic state, presentation, persistence, and mutation authority instead of letting a shell process implicitly own all four.

The current architecture has four principal planes:

1. **Semantic observation** — compositor facts are normalized into typed desktop state.
2. **Durable state** — logical identities and profiles are stored behind explicit contracts.
3. **Presentation** — `gnos.ux` projects current state into native Wayland/GPUI surfaces.
4. **Mutation authority** — narrowly scoped `in.gnu.os` services admit and execute typed requests.

Passing data between planes does not transfer authority between them.

## Semantic desktop model

`gnosix-desktop-model` is deterministic and authority-neutral. It models outputs, workspaces, surfaces, focus/visibility, associations, spatial relations, and generation-based reconciliation.

The model does not observe the compositor, allocate durable identities, persist state, launch processes, or mutate the desktop. Runtime adapters must supply explicit facts and remain responsible for their own freshness and authority boundaries.

`gnosix-hyprland` supplies the current Hyprland observation/integration layer. Fresh snapshots are runtime truth; event streams are wake-up signals, not substitutes for a complete state read.

## Native product surfaces

`gnos.ux` is the native presentation domain. The current product host is `gnos-ux-scene`, which composes output-bound Bar, Dock, Workspace Envelope and AppStack presentation through Rust/GPUI and Wayland.

Surface topology and geometry are semantic product data, not decoration. Placement, reservation, input regions, focus, visibility, and transition state are kept explicit so paint and input can be checked against the same resolved geometry.

Historical standalone probes and labs remain qualification tools. Their existence does not create additional production owners.

## Durable identity and profiles

`in-gnu-os-output-identity` owns the durable logical output-identity contract. Session-scoped compositor identities remain distinct from durable logical identities; association is explicit and may fail closed.

`in-gnu-os-desktop-profiles` owns the bounded profile document/store contract. Profile CAS, recovery, revisioning, and cross-validation are separate from presentation. `gnos.ux` consumes effective projections and does not receive raw profile-writer authority.

Current live profile-write evidence is bounded to an authenticated isolated profile qualification. Default personal-profile provisioning remains a separate product/deployment obligation.

## Activation and lifecycle

`in-gnu-os-activation` owns typed activation admission and bounded execution. The production caller boundary authenticates the exact user-session unit and invocation, revalidates fresh semantic state, and applies exact typed targets rather than shell strings.

Workspace activation is installed-path qualified through the current `gnos-ux-scene` caller. Application launching uses typed catalogue/launch references and an independent systemd user-manager execution lifetime rather than inheriting the scene process lifetime.

User-session lifecycle ownership is qualified for the current UWSM-based host/session scope. Process existence alone is never treated as readiness.

## Rendering plane

`gnoshow` remains a separate rear-plane/wallpaper renderer domain. Its rendering substrate is independent from desktop semantic and activation authority. Production integration and external control remain separately scoped work.

## Architectural invariants

- Names, package paths, dependencies, and process presence do not grant authority.
- Observation does not imply mutation authority.
- Presentation intent does not imply persistence ownership.
- Durable identities are not inferred from transient compositor tokens or geometry.
- Requests that can mutate the desktop carry typed targets and freshness boundaries.
- Unsupported or stale state fails closed instead of synthesizing a plausible normal state.
- Qualification fixtures may have powers that production code does not.
- Passing CI does not imply whole-product release acceptance.

See [AUTHORITY.md](AUTHORITY.md) for the current authority ledger and [docs/status/STATUS.md](docs/status/STATUS.md) for the bounded product claims.
