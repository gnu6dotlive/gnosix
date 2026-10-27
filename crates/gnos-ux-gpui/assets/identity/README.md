# Identity assets

`gnosix-cube-64.png` and `gnosix-cube-128.png` are the Gnosix cube from the Gnosix design system V3.4.1 (`assets/identity/gnosix-cube-alpha.svg`). That file is an exact-fidelity SVG wrapper around a 1254 × 1254 alpha PNG, not a vector. These two files are that PNG resampled (Lanczos, alpha kept) by `tools/identity_assets.py`, so the 1.3 MB master is not committed.

- Master SVG sha256: `cb8deeeaf5b5f442d86dd585b60cb0e8fd8e6b932c232ed66eb1de87ad0934cc`
- `gnosix-cube-64.png` sha256: `be22a823942221e2f384201294321584c78932241e38ef965f92f46954d3ded6`
- `gnosix-cube-128.png` sha256: `f491851e083c9ddb6d4e2d515551dd564ce103648628ae6ac275da129d13cf2d`

**DS rule ID-004:** the alpha master may only be composited on a neutral substrate. The shell's anthracite material is one. Over Signal Orange, Royal Blue or Beret Green, use the opaque master instead.

**Used by:** the Session button on the Bar, and its row in the "…" panel (`src/identity.rs`).
