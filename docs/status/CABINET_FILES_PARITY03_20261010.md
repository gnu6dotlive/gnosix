# CABINET-FILES-PARITY-03 — Adaptive Files and bounded real thumbnails

Date 2026-10-10. Branch feat/cabinet-files-parity-03-20261010, parent 7a6c70f.
Scope: local Files-first E2/E3 technical qualification. Spacedrive visual parity, E4 human acceptance and production release remain OPEN.

## Implemented

- Adaptive GridGeometry uses the actual Files canvas after the inspector splitter reserve. Minimum tile 182px, 8px gap, one to six columns, bounded indices, keyboard cursor scroll uses live column count. The list view remains separately virtualized.
- Native image thumbnails use stock GPUI image rendering. PNG/JPEG/WebP are admitted by both extension and inspected content; no custom Blade renderer or shader was introduced.
- The thumbnail decoder runs on GPUI background execution, never from the paint callback; sources are checked for type, symlinks, size and modification before decode. Limits are 12 MiB source, 8192px source side, 96 MiB decoder allocation, 192x112 output. At most six concurrent decode tasks and 96 reduced images in the resident cache; failed keys are also bounded.
- Inspector and grid use the exact same decoded image. Inspector also displays cached scan metadata and the real file path.
- File filter indices are memoized in an Arc vector and recomputed on dataset, query or dotfiles changes rather than rebuilt on every GPUI render; the scanner remains on a background worker with an epoch guard.
- The standalone Files scanner now admits 32768 entries while the separate XDG FileChooser remains limited to 4096. Limit overflow is explicit; no claim of unbounded paging.

## Evidence obtained

- Native Rust: 68 unit tests PASS, one heavy stress test explicitly ignored in routine test runs; the heavy test itself PASS when invoked independently.
- Image oracle: authentic generated PNG pixels converted to BGRA; malformed, oversized and symlinked image inputs rejected.
- Weston/Xvfb isolated Wayland E2E PASS: application routes/session restart; Files folder navigation/restart; keyboard list focus and Home/Down/Up/Enter; native IME search, Grid and Ctrl+L, real gradient image in grid and inspector, and per-tab restart.
- Visual evidence: /mnt/workbench/evidence/runs/cabinet-files03-final-media-e2e-20261010/native-thumbnail-inspector.png and native-image-thumbnail.png (actual 1100x820 compositor screenshots, synthetic fixture).
- Heavy temporary fixture: /mnt/workbench/evidence/runs/cabinet-files03-heavy-directory-20261010/benchmark.log. One 32769-file corpus took 54 ms to construct and 30 ms to scan (32768 results, truncated=true). These are local per-run scanner timings on a temporary filesystem, NOT user-perceived latency, cold physical disk speed, frame pacing or 100k capability.
- Strict offline locked Clippy, native build and exact binary SHA-256: /mnt/workbench/evidence/runs/cabinet-files-parity03-final-build-20261010/.
- Full repository hermetic CI: **48/48 PASS, zero non-PASS gates**. Receipt: /mnt/workbench/evidence/runs/cabinet-files-parity03-full-ci-20261010/.

## Known limitations and next gates

- E4 reference-hardware screenshot normalization, fluidity frame-present p95/p99, Hyprland operator evaluation and visual parity with Spacedrive remain OPEN.
- PNG image was qualified by real screenshots; JPEG/WebP have a compiled bounded decode path but no independent Wayland image oracle yet. No animated/media/video thumbnails.
- 32768-entry file listings are still collected into one sorted background result, not asynchronously paged. 100k scanner UI latency, query cancellation/backpressure, filesystem watchers and overflow recovery remain OPEN.
- Grid tile geometry has an E2 unit solver and an E3 single-viewport screenshot; multi-DPI and viewport series remain OPEN.
- Accessibility/AT-SPI, large-dataset focus, high-contrast light theme, contextual menus and selection operations remain OPEN.
- Software Store, News, Installer, public deployment, portal configuration, distro integration, merge and push remain outside this scope.
- No Shelly-GPUI GPL code or render lab shader has been imported; licensing decisions remain with the separate worker.

Promotion verdict: Files P03 bounded technical slice, local E2/E3 PASS where explicitly evidenced. E4-E6 NOT QUALIFIED.
