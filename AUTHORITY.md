# Authority boundaries

Authority in gnosix is explicit. A crate name, dependency edge, service file, process, package path, or UI control never establishes permission to mutate another domain.

## Current owners

| Domain | Owns | Does not imply |
| --- | --- | --- |
| Hyprland | Current compositor/runtime reality | durable identity or profile ownership |
| `gnosix-desktop-model` | Pure desktop semantics and reconciliation | observation, persistence, launch, or mutation |
| `gnosix-hyprland` | Bounded Hyprland observation/integration | generic shell-command authority |
| `gnos.ux` | Product presentation, interaction intent, local semantic generation | durable writer, compositor-wide authority, or arbitrary execution |
| `in.gnu.os` identity/profile components | Selected durable identity/profile contracts | presentation ownership or generic configuration authority |
| `in.gnu.os` activation | Typed admission, freshness revalidation, bounded execution | arbitrary command execution or profile mutation |
| systemd user manager | Qualified independent application process lifetime | semantic selection or launch-target authority |
| `gnoshow` | Rear-plane rendering | desktop semantics, activation, or profile mutation |

## Admitted boundaries

Current-session output/workspace/surface observation and multi-output routing are qualified without turning transient compositor tokens into durable identities.

The durable logical output identity core is source-qualified with explicit association to session state. The profile store/projection path is source-qualified; an authenticated profile write has also been live-qualified against an isolated selected profile. Those facts do not silently select a default personal profile.

Typed workspace activation is installed-path and live qualified through the authenticated production caller. The authority revalidates the requested typed workspace target and does not accept display labels or shell strings as authority-bearing input.

The selected activation lifecycle is lazy-start/session-resident and has been qualified across real user-session replacement. Application processes launched from the product are owned independently by the user service manager so stopping the scene does not implicitly kill an admitted launched application.

## Still separate or open

- Whole-product release acceptance and end-user accessibility review.
- Default personal-profile provisioning/selection for ordinary daily use.
- Broader Settings/provider mutation capabilities beyond individually admitted operations.
- Production `gnoshow` integration/control boundaries.
- Any global keybinding/input producer backend beyond the currently qualified bounded observation/research scopes.
- New external-control or cross-process transports unless explicitly ratified.

## Rule

Every new mutation path must identify its state owner, caller trust boundary, exact operation set, freshness/replay semantics, failure behavior, and closure evidence. If those facts are missing, the authority remains unselected.
