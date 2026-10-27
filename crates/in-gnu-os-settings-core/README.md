# in-gnu-os-settings-core

Pure contracts for the future Gnosix Settings Authority.

This crate is intentionally side-effect free. It does not:

- read or write XDG configuration;
- launch applications;
- talk to polkit;
- expose D-Bus;
- run a daemon;
- collect authentication secrets;
- apply system settings.

It exists so PRODUCT-TOOLING-01 Phase 2 can model installed applications,
default-application targets and setting metadata without inventing identities
that would later conflict with the settings service.

See:

- `../../docs/research/xdg-settings/pre-phase2-research.md`
- `../../docs/workstreams/settings/authority-scaffold.md`
