![gnosix — local-first Linux computing platform](.github/assets/gnosix-hero.png)

A local-first Linux computing platform built around native Wayland surfaces, explicit system state, and bounded authority.

**Development state:** Experimental / pre-release. The source is available for inspection and development; no stable end-user release is claimed.

## What gnosix is

gnosix is a Rust-first desktop systems project. Its current work combines a compositor-independent semantic desktop model, native Wayland/GPUI presentation, typed persistent profile and identity contracts, and narrowly scoped system authorities for actions that must mutate the running desktop.

The repository is intentionally evidence-driven: a claim is not promoted because code exists, a process is running, or a document says it should work. Public status distinguishes implemented code, source qualification, runtime observation, installed-path proof, and still-open product obligations.

## Current status

The integrated `main` line is qualified through the repository-owned `cargo xtask ci` contract. Current product work includes native Bar/Dock/Workspace/AppStack presentation, current-session multi-output routing, durable logical output/profile contracts, installed typed workspace activation, and qualified user-session lifecycle boundaries.

![Current gnosix evidence boundary](.github/assets/gnosix-evidence.png)

No stable end-user release is claimed. Whole-product acceptance remains open, including end-user accessibility review and the explicit release disposition tracked in [current status](docs/status/STATUS.md).

## Architecture

![gnosix current authority map](.github/assets/gnosix-architecture.png)

The main domains are:

- `gnosix-desktop-model` — deterministic, authority-neutral desktop semantics.
- `gnosix-hyprland` — Hyprland observation and bounded integration.
- `gnos.ux` — native product presentation and interaction semantics.
- `in.gnu.os` — narrowly scoped system authorities such as durable identity, profiles, and activation.
- `gnoshow` — independent rear-plane/wallpaper renderer work.

See [ARCHITECTURE.md](ARCHITECTURE.md) and [AUTHORITY.md](AUTHORITY.md) for the boundaries between those domains.

## Build and inspect

The repository pins its Rust toolchain in `rust-toolchain.toml`. On Ubuntu 24.04, CI provisions `build-essential`, `pkg-config`, Wayland/EGL/Vulkan/XKB/Freetype/Fontconfig development packages, and D-Bus development headers before running the same repository-owned contract used locally.

```bash
cargo xtask explain
cargo xtask ci
cargo xtask provenance
```

`cargo xtask ci` is repository-hermetic and is the canonical pre-push qualification entry point. `cargo xtask verify` additionally contains host/live checks; run it only in an environment where those checks and any required host interactions are appropriate and authorized.

## Evidence

Start with [docs/status/STATUS.md](docs/status/STATUS.md) for the current claim boundary and [docs/evidence/README.md](docs/evidence/README.md) for how source, runtime, installed-path, and operator evidence are interpreted.

The large engineering evidence corpus is provenance, not the product interface. Public documentation links only to evidence needed to support a current claim.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and [AGENTS.md](AGENTS.md) before changing architecture-sensitive or authority-sensitive code.

## License

First-party Gnosix code and repository documentation are licensed under **GPL-3.0-or-later**. Bundled third-party material retains its upstream license; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
