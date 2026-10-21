# gnosix — upstream systems intake

**2026-10-21 · research index · no product qualification implied**

## Scope

This note tracks upstream changes that can materially affect `gnosix`, `gnos.ux`, `in.gnu.os`, `gnusnip` or `gnoshow`.

### Wayland / workspace identity

- Track `ext-workspace-v1` and presentation/activation protocols as canonical protocol sources.
- Treat Hyprland-specific workspace/toplevel/capture/focus extensions as optional compositor enrichment.
- Probe exact workspace identity, creation/removal, activation, surface association and popup lifetime.
- Never infer physical display identity from transient compositor IDs.

### GPUI / Zed

Track display identity, window↔display association, visibility/suspension, Wayland IME and non-primary-output positioning.

Target probe:

```text
output hotplug
→ window movement
→ non-primary placement
→ tiled/maximized transition
→ hidden/suspended state
→ restore/focus/presentation timing
```

### Hyprland

Track workspace sharing, toplevel mapping, focus-grab/input-capture and pointer-constraint/popup interactions. A compositor extension can enrich the model but cannot redefine the portable desktop semantics.

### Landlock

Define an ABI-aware least-authority policy for worker processes:

```text
model/data        RO
cache/temp        narrowly RW
selected IPC      explicit
network           denied unless a task proves it needs it
namespace/capabilities  denied unless explicitly required
```

The policy must probe the supported ABI and request only supported rights. It must expose what was actually enforced; it is not a blanket sandbox claim.

## Evidence boundary

Record exact upstream revision/protocol version, local availability, compositor/backend, test oracle and negative cases. Distinguish source capability, runtime observation and gnosix qualification.

No upstream change closes a gnosix release obligation by itself. No historical `gnosis.OS` repository or naming is an architecture target.

## Related public intake

See GitHub issue #4 for the live coordination thread. Product status remains governed by `docs/status/STATUS.md`, architecture by `ARCHITECTURE.md`/`AUTHORITY.md`, and the repository-owned qualification command by `cargo xtask ci`.
