# GNOS.UX product intent

This crate is the passive client-side boundary between GNOS.UX presentation
and a future authenticated activation transport.

It owns no compositor authority. It contains no executor, capability,
transport, session epoch, retry queue, or authorization decision.

## Vocabulary

The only current intent variants are:

- `ActivateObservedWorkspace`
- `FocusObservedApplication`

Each target is an exact semantic identity paired with the
`DesktopContextGeneration` at which GNOS.UX observed it.

That generation is a **client-observation fence**, not an authority generation.
`gnos-ux-scene` and `in-gnu-os-activationd` each own an independent
`DesktopSemanticRuntime`, so equal numeric generation values across those
processes do not establish freshness. The retained client fence lets the future
transport reject a selected UI reference if GNOS.UX has already advanced to a
newer local projection before send; no such send gate exists in this
source-only slice. The authority must independently establish server-side
freshness from its own current observation and private `ActivationSessionEpoch`.

Display data is deliberately insufficient to construct an actionable target.
Workspace names, application titles, classes, PIDs, connectors, geometry and
focus order are presentation/observation fields, not authority identity.

`ProductIntentRequestId` is correlation-only. It grants no authority and is
not a durable workspace/application identity.

## Authority split

GNOS.UX may construct a product intent from a current read-only semantic
projection. It must not construct or receive `ActivationSessionEpoch`.

The future in.gnu.os boundary is responsible for:

- authenticating the caller;
- authorizing the specific operation;
- obtaining a current authoritative observation;
- combining client observation generation with its private current session;
- issuing any operation-scoped capability internally;
- revalidating immediately before execution;
- returning a typed receipt.
## Dependency rule

This crate depends only on `gnosix-desktop-model`.

It must not depend on:

- `in-gnu-os-activation`;
- `gnosix-hyprland`;
- compositor transports or command adapters;
- systemd/D-Bus/socket implementation crates.

The transport remains a separate R13 production decision.
