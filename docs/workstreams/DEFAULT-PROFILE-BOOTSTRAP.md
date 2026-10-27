# 🪟 Gnosix Architecture Workstream — Default Personal-Profile Provisioning & Bootstrap

> **Document Type:** Autonomous Agent Handoff Specification  
> **Repository:** `~/Projects/gnosix`  
> **Status:** APPROVED / IN PROGRESS  
> **Ratified Date:** 2026-10-27  
> **Target Subsystem:** `in-gnu-os-desktop-profiles`, `in-gnu-os-profile-service`, `gnos-ux-scene`  

---

## 1. Context & Architectural Problem

In Gnosix's current status ([`docs/status/STATUS.md`](../status/STATUS.md)), the persistent profile store/projection is classified as **PROVEN**, but bears a critical open obligation:
> **"Default personal-profile provisioning/selection remains separate from the isolated profile-write qualification."**

### The Core Tension
1. **Isolated Qualification:** Bounded profile CAS mutations (e.g., live Bar-edge placement, cardinal visibility) were qualified on an isolated, authenticated profile under test fixtures.
2. **First-Boot Gap:** On a fresh session or clean operator workstation, no `desktop-profiles-v1.json` exists in the user profile directory. Currently, there is no autonomous mechanism to bootstrap a ratified personal profile without violating Gnosix's fundamental invariants (such as inventing fake durable IDs or bypassing CAS tokens).

---

## 2. Workstream Objective

**Primary Goal:** Architect and implement the idempotent first-boot personal profile bootstrap lifecycle in `in-gnu-os-desktop-profiles` and `in-gnu-os-profile-service`.

When a session starts without an initialized profile:
1. Validate the system profile template seed (`share/gnosix/profiles/default-profile-seed.json`).
2. Perform an atomic, authority-guarded initialization CAS transaction creating `desktop-profiles-v1.json`.
3. Associate physical/session output monitors with durable logical identities via `in-gnu-os-output-identity` without guessing or synthesizing identities.
4. Provide the effective read-only projection to `gnos-ux-scene` with zero presentation-to-storage authority leaks.

---

## 3. Strict Invariants & Anti-Goals

> [!CAUTION]
> The Gnosix architecture strictly separates semantic state, presentation, persistence, and mutation authority. Agents implementing this workstream MUST enforce these boundaries.

* **ANTI-GOAL 1 (Presentation Never Persists):** `gnos.ux` and `gnos-ux-scene` must never write profile JSON directly to disk, nor receive raw writer handles. Presentation components only receive read-only projections and submit typed mutation intents to `in-gnu-os-profile-service`.
* **ANTI-GOAL 2 (No Synthesized Identities):** Never infer or synthesize a durable logical output identity from transient compositor tokens (Wayland output names, DRM connectors, or ephemeral geometry). Association must be explicit, verifiable, or fail closed.
* **ANTI-GOAL 3 (No Fallback-on-Conflict Synthesis):** If a CAS generation conflict occurs during profile commit, the store must fail closed (`ProfileError::CasGenerationMismatch`), never silently overwriting concurrent writes with a synthesized default.
* **ANTI-GOAL 4 (No Host Workstation Mutation Outside Scope):** Build, test, and qualification code must never alter the operator's real `~/.config/gnosix/` during automated tests; tests must execute inside isolated directories under `.ci-cache/` or hermetic test roots.

---

## 4. Architectural State Machine

```
                  ┌──────────────────────────────┐
                  │      User Session Starts     │
                  │   (UWSM / gnos-ux-scene)     │
                  └──────────────┬───────────────┘
                                 │
                 Query Active Profile Projection
                                 │
                                 ▼
                  ┌──────────────────────────────┐
                  │ Does desktop-profiles-v1     │
                  │       exist on disk?         │
                  └──────┬────────────────┬──────┘
                         │ YES            │ NO
                         │                ▼
                         │     ┌───────────────────────────────────┐
                         │     │  Atomic Bootstrap Initialization  │
                         │     │  1. Read default seed template    │
                         │     │  2. Acquire exclusive store lock  │
                         │     │  3. Associate Output Hub Identity │
                         │     │  4. Commit Gen-1 CAS Document     │
                         │     │  5. Touch '...initialized' marker │
                         │     └──────────────────┬────────────────┘
                         │                        │
                         ▼                        ▼
                  ┌────────────────────────────────────────┐
                  │ Emit Effective Read-Only Projection    │
                  │ (Bar Placement, Dock, Workspace Caps)  │
                  └──────────────────┬─────────────────────┘
                                     │
                                     ▼
                  ┌────────────────────────────────────────┐
                  │ gnos-ux-scene Paints Native Wayland UI │
                  └────────────────────────────────────────┘
```

---

## 5. Implementation Phases & Milestones

### Phase 1: Canonical Seed Template & Deserialization Invariants
* Location: `crates/in-gnu-os-desktop-profiles/src/seed.rs`
* **Deliverable:** Define the hermetic `DesktopProfileSeed` structure, embedding default ratified surfaces (Bar on top/bottom, default workspace topology, AppStack keybindings).
* **Gate:** Source-level qualification proving `DesktopProfileSeed::into_document()` passes all schema validations and produces a valid Gen-1 CAS hash.

### Phase 2: Idempotent First-Boot Provisioner
* Location: `crates/in-gnu-os-desktop-profiles/src/store.rs`
* **Deliverable:** Implement `DesktopProfileStore::initialize_default_if_absent(&self, seed: &DesktopProfileSeed, identity_snapshot: &LockedIdentitySnapshot) -> Result<DesktopProfileDocument, StoreError>`.
* **Gate:** Concurrency test proving that 10 concurrent threads racing to initialize the store result in exactly one successful commit, nine clean CAS rejections/re-reads, and zero corrupt `.pending` files.

### Phase 3: Service-Level Provisioning & Projection Export
* Location: `crates/in-gnu-os-profile-service/src/lib.rs`
* **Deliverable:** Wire the bootstrap sequence into the session startup handshake before `gnos-ux-scene` acquires initial surface reservations.
* **Gate:** Integration test proving `gnos-ux-scene` receives a valid `DesktopProfileProjection` on an empty test environment without manual profile seeding.

---

## 6. Verification & Evidence Requirements

Autonomous agents taking on this workstream must qualify their changes according to the repository evidence contract:

1. **Repository Check:** `cargo xtask ci` must pass with zero warnings and all tests green.
2. **Crash-Consistency Check:** Simulate process kill during profile initialization (`kill -9` between `.pending` write and atomic rename); verify the store recovers cleanly without reading partial state.
3. **Evidence Record:** Commit raw test output and qualification JSON to `evidence/runs/gnosix-profile-bootstrap-<timestamp>/run.json`.
