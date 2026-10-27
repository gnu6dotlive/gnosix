# GNOSIX BASELINE-0

Status: CURRENT LOCAL BASELINE
Observed: 2026-09-21

Canonical source root: `<repo>`

Current admitted product crate: `gnosix-desktop-model`.

Ratified names: `gnosix`, `gnos.ux`, `in.gnu.os`, `gnusnip`, `gnoshow`.

Fresh Rust verification:
- `cargo fmt --all -- --check`: PASS
- `cargo test --workspace`: PASS
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS

Gnoshow is the GNOSIX rear-plane / wallpaper rendering runtime product role. Its current implementation is not present in this source root. Hosting, lifecycle, external control, admission, and presentation-intent ownership remain OPEN.

Machine state after cleanup:
- Hyprland is running on DRM.
- `hyprctl configerrors` is empty.
- systemd reports `running` with zero failed system units.
- user systemd reports zero failed units.
- the stale local translation-proxy restart loop is disabled and stopped.
- the dead local notification activation entry was removed.
- `wg-gnu6` is live and handshaking; only its stale failed-state marker was cleared.
- bounded local/system scans show no remaining filesystem path carrying the retired project spelling.

No endpoint, service authority, persistence owner, activation owner, or historical recovery is implied by this baseline.
