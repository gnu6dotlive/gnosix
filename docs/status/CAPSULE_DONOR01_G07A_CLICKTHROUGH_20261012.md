# CAPSULE-DONOR-01 / G07-A — Independent native click-through

**12 October 2026 · E3 bounded click-through PASS · E4/operator & dynamic topology OPEN**

## Program and physical test boundary

This gate evaluates whether the native Gnosix Dock Envelope's Wayland input region actually allows clicks into an underlying *native GPUI surface*, not merely whether pointer-region projection computes plausible rectangles. No changes were made to the running Gnosix shell, Hyprland monitor configuration, LayerShell policy or Cabinet.

Exact build tested: `gnos-ux-dock-envelope-qualify` SHA-256 `40ff54bc4715aedca668a27e5fbcf5dab11f79725a1559d4e1fd3582234a2823`, archived as `/mnt/workbench/capsule-donor-01/g07a-archive/g07a-witness-qualified.bin.zst`.

The diagnostic program adds an opt-in full-window witness using a separate LayerShell `wl_surface`, namespace `gnos.ux:g07:input-witness`, Layer::Top. It is opened *before* the overlay in the same process, or in its **own process** with the opt-in `GNOS_ENVELOPE_G07_WITNESS_ONLY` mode. The overlay retains `gnos.ux:dock:atlas-qualification`, the existing `resolve_dock_frame`, original `SharedMotion` and production-side input-region application semantics. No vendor GPUI code was patched for this gate.

Both windows were physically checked in `hyprctl layers -j` before any click: matching 900×560 bounds, same physical monitor, correct namespace, separate `wl_surface` address. Isolated-process tests further require **different PIDs**. The cursor move engine uses bounded `uinput` steps, reads back real cursor positions, and refuses events outside the strictly defined test surface. It restores the cursor and terminates its owned `ydotoold` daemon after every run.

Hardware remains correctly identified:

| Output | Native mode | Transform | Captures |
|---|---|---:|---|
| HDMI-A-1, LG QHD | 2560×1440 at 74.964 Hz | 0 | PNG 2560×1440 |
| DP-2, Acer | 1920×1080 at 164.917 Hz | 1, portrait | PNG 1080×1920 |

## Correct routing across open and hidden states

The overlay's **open** input-region set is main `[30,392,870,496]` plus reveal `[0,496,900,501.3]`; **hidden** accepts only reveal `[0,496,900,501.3]`.

| State | Exact local test point | Intended recipient | Observed |
|---|---|---|---|
| Open | (450,444) main area | Envelope | Envelope exclusively |
| Open | (450,270) outside region but within witness | Underlying witness | Witness exclusively |
| Open | (450,499) reveal strip | Envelope | Envelope exclusively |
| Hidden | (450,444) former main area | Underlying witness | Witness exclusively |
| Hidden | (450,499) reveal strip | Envelope | Envelope exclusively |
| Hidden | (450,270) outside region | Underlying witness | Witness exclusively |

A single diagnostic executable hosting **two distinct Wayland surfaces in one process**, two trials per monitor with reversed state order, provided **24/24 correct first-attempt single-recipient clicks**. An additional test hosted witness and overlay in **two independent GPUI processes**, run on both physical monitors, and provided **12/12 correct first-attempt single-recipient clicks**.

**Total: 36/36 correctly routed native pointer clicks on first injection, zero incorrect dual receivers.** Each case includes exact surface identity, PID, compositor layer position, logged GPUI mouse-down events, pointer coordinates, full-screen screenshot and file digest.

The independently implemented `g07a_oracle.py` replays the log joins and geometry requirements over the 12 run states and rejects mismatched trace/oracle copies. **8/8 negative mutations fail closed** (forged PID, recipient, missing or duplicate event, layer identity, screenshot dimensions, retry masking, namespace).

## Limits / production decision

**E3 static click-through is qualified.** This is stronger than the preceding G07 positive-only input-gate, because it proves delivery into the underlying native receiver rather than treating no overlay event as successful click-through.

It is still **not** an E4 or unrestricted production promotion. This gate does not prove click-through while the shape is moving, full live Fused/Bridged/Detached topology, scaled 1.25×/2× surfaces, optical input-to-photon timing, unique buffer/pixel correspondence, or operator perceptual approval. The earlier sporadic first synthetic DP-2 press loss did not reproduce in these experiments, but it has **not** been root-caused or declared fixed.

Decision: **G07-A E3 PASS**, Envelope motion scheduler remains **ADAPT in isolated staging**, Cabinet stays **HOLD**. Next: **G07-B operator visual review** and a separate dynamic shape-aware click-through experiment before production. No merge, push or live runtime mutation.
