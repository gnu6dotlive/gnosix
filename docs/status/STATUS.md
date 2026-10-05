# gnosix — current status

This is the public claim boundary for the current integrated tree. It summarizes current product facts; it is not a mirror of every historical workstream retained in engineering provenance.

**Development state:** Experimental / pre-release. No stable end-user release is claimed.

## State vocabulary

| State | Meaning |
| --- | --- |
| **PROVEN** | The bounded claim has source qualification plus retained runtime or installed-path evidence. |
| **SOURCE-QUALIFIED** | The contract/invariants are exercised in source-level qualification; broader runtime behavior is not implied. |
| **IMPLEMENTED** | Product code exists and is used, but a broader qualification claim is intentionally not made. |
| **EXPERIMENTAL** | Active implementation/research with a limited claim boundary. |
| **OPEN** | A named product, integration, review, or authority obligation remains unresolved. |

## Product and runtime

| Capability | State | Public claim boundary | Evidence |
| --- | --- | --- | --- |
| Native Bar/Dock/Workspace/AppStack scene | **PROVEN** | Current production scene uses native Rust/GPUI + Wayland surfaces; AppStack launch lifetime, pointer and keyboard paths were installed-path qualified. | [public evidence](../evidence/CURRENT.md#native-scene-and-appstack) |
| Current-session multi-output routing | **PROVEN** | Output-local projections, reservations and topology add/remove/reattach are qualified for current-session operation. This is not physical-hardware identity. | [public evidence](../evidence/CURRENT.md#current-session-multi-output) |
| Durable logical output identity core | **SOURCE-QUALIFIED** | Persistent logical identities and explicit durable-to-session association exist; transient compositor IDs are not promoted into durable identity. | [public evidence](../evidence/CURRENT.md#durable-logical-identity) |
| Persistent profile store/projection | **PROVEN** | Profile schema/store/projection is source-qualified; authenticated live Bar-edge write/CAS/restart/restore was qualified on an isolated selected profile. Default personal-profile provisioning is not claimed. | [public evidence](../evidence/CURRENT.md#profile-store-and-bounded-live-write) |
| Typed workspace activation | **PROVEN** | Exact typed workspace activation is installed-path/live qualified through the authenticated `gnos-ux-scene` caller with freshness and replay boundaries. | [public evidence](../evidence/CURRENT.md#typed-workspace-activation) |
| User-session lifecycle | **PROVEN** | Current UWSM/greetd session replacement and selected lazy-start activation lifecycle are qualified for the defined user-session scope. | [public evidence](../evidence/CURRENT.md#user-session-lifecycle) |
| Bar edge + visibility preferences | **PROVEN** | Cardinal relocation plus `persistent`/`showOnHover` consumption are live-qualified within their bounded profile/input contracts. Other donor preferences are not implied complete. | [public evidence](../evidence/CURRENT.md#bar-placement-and-visibility) |
| Global semantic keybinding backend | **EXPERIMENTAL** | Typed arbitration and bounded runtime observation research exist; a complete production input producer/live backend is not claimed. | [public evidence](../evidence/CURRENT.md#semantic-keybinding-observation) |
| `gnoshow` rendering substrate | **IMPLEMENTED** | Native rear-plane rendering substrate exists; production integration, hosting/control, and product admission remain separate work. | [public evidence](../evidence/CURRENT.md#gnoshow) |
| Implemented-product accessibility mechanics | **PROVEN** | Keyboard traversal/activation, focus exclusivity and bounded AT-SPI naming/action paths are qualified for implemented surfaces; whole-product end-user screen-reader usability is not claimed. | [public evidence](../evidence/CURRENT.md#accessibility-mechanics) |
| Whole-product release acceptance | **OPEN** | The prior operator disposition remains not publishable as a stable release; source visibility and passing CI do not close R19. | [public evidence](../evidence/CURRENT.md#release-acceptance) |

## Open product obligations

- **R19 whole-product acceptance remains OPEN / NOT_PUBLISHABLE as a stable release.** Passing CI or individual installed-path gates does not close it.
- End-user screen-reader usability has not received the required whole-product human review.
- Default personal-profile provisioning/selection remains separate from the isolated profile-write qualification.
- `gnoshow` production integration/control remains separate work.
- Broader provider/settings mutation capabilities and additional input/keybinding production authority remain individually scoped work.
- A repository-wide public license has not yet been ratified.

## Repository qualification

The canonical repository check is `cargo xtask ci`. GitHub CI invokes that same contract after provisioning the pinned toolchain/native headers. Host/live checks live under `cargo xtask verify` and must not be conflated with repository-hermetic CI.

See [evidence policy](../evidence/README.md), [architecture](../../ARCHITECTURE.md), and [authority boundaries](../../AUTHORITY.md).
