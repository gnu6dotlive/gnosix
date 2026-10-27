# Protocol files

- `hyprland-toplevel-mapping-v1.xml`: from
  [hyprwm/hyprland-protocols](https://github.com/hyprwm/hyprland-protocols),
  tag `v0.8.0` (`044a434e`). The file is identical in `v0.7.0` through `v0.8.0`
  (sha256 `05464521a13c96ed274a8545d84ac03980d94b32bd55a8467c7949ed1a1ee96e`).
  BSD-3-Clause; the licence travels in the file's `<copyright>`.

The capture itself uses the standard `ext-image-copy-capture-v1`,
`ext-image-capture-source-v1` and `ext-foreign-toplevel-list-v1` from the
`wayland-protocols` crate. This file only ties a toplevel to the Hyprland
window it is, so a thumbnail lands on the right window.
