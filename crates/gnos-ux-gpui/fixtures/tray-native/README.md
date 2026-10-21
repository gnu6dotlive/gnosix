# PARITY-01G native Tray fixture

This fixture belongs to Gnosix component-lab qualification. It is not copied
from Caelestia, a host theme, or an application.

The SVG artwork is original diagnostic geometry created for this fixture.
Its purpose is to make colour, alpha, selection and overlay mistakes obvious.

The controlled theme is `GnosixTrayFixture`, with one fixed 24 px directory.
The lab injects its root explicitly; deterministic capture does not search the
host's icon themes.

Cases:
- `01-named`: normal themed icon.
- `02-attention`: NeedsAttention with dedicated attention icon.
- `03-attention-pixmap`: NeedsAttention fallback to provider pixmap.
- `04-overlay`: themed base plus themed transparent overlay.
- `05-private`: name resolvable only through per-item IconThemePath.
- `06-missing`: intentionally unresolved name; slot remains empty.

Diagnostic colours:
- normal red: `#d83b35`
- attention amber: `#e9a23b`
- pixmap magenta: `#c026d3`
- overlay blue source: `#2979ff` at 50% alpha
- private-path green: `#2da44e`

The missing case must never acquire a donor glyph or Material Symbols fallback.
Application imagery is preserved; Gnosix only selects, resolves, composites and
lays it out.
