import os
from pathlib import Path

OUT_DIR = Path("/home/tension_atoi/Projects/gnosix/docs/diagrams")
OUT_DIR.mkdir(parents=True, exist_ok=True)

def wrap_v341(title, eyebrow, desc, svg_inner, width=1100, height=580):
    esc_title = title.replace("&", "&amp;")
    esc_desc = desc.replace("&", "&amp;")
    esc_eyebrow = eyebrow.replace("&", "&amp;")
    
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{esc_title} · gnosix_DS V3.4.1</title>
  <link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&display=swap" rel="stylesheet">
  <style>
    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    :root {{
      --gnx-shell-white:              #F7F3ED;
      --gnx-pure-white:               #FFFFFF;
      --gnx-anthracite:               #111418;
      --gnx-slate:                    #667085;
      --gnx-muted:                    #A1A6AD;
      --gnx-hairline:                 #D7DADF;
      --gnx-quiet-divider:            #E6E8EB;
      --gnx-signal-orange:            #FF6A00;
      --gnx-signal-orange-accessible: #C95400;
      --gnx-signal-orange-tint:       #FFE5D2;
      --gnx-beret-green:              #5F7F52;
      --gnx-beret-green-accessible:   #455F3D;
      --gnx-beret-green-tint:         #E8EFE5;
      --gnx-royal-blue:               #4169E1;
      --gnx-royal-blue-accessible:    #2446A8;
      --gnx-royal-blue-tint:          #E8EDFF;
      --font-primary:                 'Space Grotesk', ui-sans-serif, system-ui, sans-serif;
    }}

    body {{
      font-family: var(--font-primary);
      background: var(--gnx-shell-white);
      color: var(--gnx-anthracite);
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 2.5rem 1.5rem;
    }}

    .frame {{
      max-width: {width + 40}px;
      width: 100%;
      background: var(--gnx-shell-white);
    }}

    .eyebrow {{
      font-family: var(--font-primary);
      font-size: 0.72rem;
      font-weight: 600;
      letter-spacing: 0.16em;
      text-transform: uppercase;
      color: var(--gnx-signal-orange-accessible);
      margin-bottom: 0.4rem;
    }}

    h1 {{
      font-family: var(--font-primary);
      font-size: clamp(1.75rem, 2.5vw + 0.8rem, 2.3rem);
      font-weight: 700;
      letter-spacing: -0.02em;
      line-height: 1.15;
      color: var(--gnx-anthracite);
      margin-bottom: 0.4rem;
    }}

    .subtitle {{
      font-family: var(--font-primary);
      font-size: 0.95rem;
      font-weight: 400;
      color: var(--gnx-slate);
      margin-bottom: 1.6rem;
    }}

    svg {{
      width: 100%;
      height: auto;
      display: block;
      background: var(--gnx-shell-white);
      border: 1px solid var(--gnx-hairline);
      border-radius: 8px;
      box-shadow: 0 4px 24px rgba(17,20,24,0.04);
    }}
  </style>
</head>
<body>
  <div class="frame">
    <p class="eyebrow">{esc_eyebrow}</p>
    <h1>{esc_title}</h1>
    <p class="subtitle">{esc_desc}</p>

    <svg viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="dia-title dia-desc">
      <title id="dia-title">{esc_title}</title>
      <desc id="dia-desc">{esc_desc}</desc>
      <defs>
        <pattern id="grid-dots" width="22" height="22" patternUnits="userSpaceOnUse">
          <circle cx="2" cy="2" r="1" fill="rgba(17,20,24,0.06)"/>
        </pattern>
        <marker id="arrow" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
          <polygon points="0 0, 8 3, 0 6" fill="#667085"/>
        </marker>
        <marker id="arrow-orange" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
          <polygon points="0 0, 8 3, 0 6" fill="#FF6A00"/>
        </marker>
        <marker id="arrow-green" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
          <polygon points="0 0, 8 3, 0 6" fill="#5F7F52"/>
        </marker>
        <marker id="arrow-blue" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
          <polygon points="0 0, 8 3, 0 6" fill="#4169E1"/>
        </marker>
      </defs>

      <rect width="{width}" height="{height}" fill="#F7F3ED"/>
      <rect width="{width}" height="{height}" fill="url(#grid-dots)"/>

{svg_inner}
    </svg>
  </div>
</body>
</html>
"""

# 01-platform-vision
d1 = """
      <rect x="50" y="25" width="1000" height="42" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1"/>
      <text x="70" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.1em">GNOSIX PLATFORM ARCHITECTURE</text>
      <text x="340" y="50" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="400" fill="#667085">Decoupled Planes: Presentation · Pure Semantics · Bounded Mutation</text>
      <rect x="910" y="33" width="125" height="26" rx="4" fill="#FFE5D2"/>
      <text x="922" y="50" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">gnosix_DS V3.4.1</text>

      <rect x="50" y="90" width="300" height="360" rx="8" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.8"/>
      <rect x="50" y="90" width="300" height="44" rx="8" fill="#FFE5D2"/>
      <text x="70" y="117" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#111418">gnos.ux · Presentation</text>
      <text x="70" y="129" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400" letter-spacing="0.08em">RUST &amp; GPUI LAYER-SHELL HOST</text>

      <rect x="68" y="152" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="82" y="172" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">gnos-ux-bar</text>
      <text x="82" y="188" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Output-bound status &amp; workspace pills</text>

      <rect x="68" y="210" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="82" y="230" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">gnos-ux-dock</text>
      <text x="82" y="246" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Running app indicators &amp; quick launcher</text>

      <rect x="68" y="268" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="82" y="288" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">Workspace Envelope</text>
      <text x="82" y="304" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Multi-output spatial view bounds</text>

      <rect x="68" y="326" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="82" y="346" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">AppStack Surfaces</text>
      <text x="82" y="362" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Wayland native window geometry projection</text>

      <text x="70" y="420" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="500" fill="#667085">Invariant: Zero implicit mutation powers</text>

      <rect x="400" y="90" width="300" height="360" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
      <rect x="400" y="90" width="300" height="44" rx="8" fill="#E8EFE5"/>
      <text x="420" y="117" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#111418">gnosix-desktop-model</text>
      <text x="420" y="129" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#455F3D" letter-spacing="0.08em">DETERMINISTIC SEMANTICS</text>

      <rect x="418" y="152" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="432" y="172" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">Topology &amp; Outputs</text>
      <text x="432" y="188" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Normalized screen geometry &amp; scales</text>

      <rect x="418" y="210" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="432" y="230" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">Workspace State Tree</text>
      <text x="432" y="246" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Occupancy, active focus, spatial layouts</text>

      <rect x="418" y="268" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="432" y="288" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">Surface Associations</text>
      <text x="432" y="304" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Active, visible, and background windows</text>

      <rect x="418" y="326" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="432" y="346" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">State Reconciliation</text>
      <text x="432" y="362" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Generation-based immutable snapshots</text>

      <text x="420" y="420" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="500" fill="#667085">Authority: Read-only model, zero mutations</text>

      <rect x="750" y="90" width="300" height="360" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
      <rect x="750" y="90" width="300" height="44" rx="8" fill="#E8EDFF"/>
      <text x="770" y="117" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#111418">in.gnu.os · Authorities</text>
      <text x="770" y="129" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#2446A8" letter-spacing="0.08em">SCOPED MUTATION DAEMONS</text>

      <rect x="768" y="152" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="782" y="172" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">in-gnu-os-activation</text>
      <text x="782" y="188" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Typed admission, caller auth &amp; dispatch</text>

      <rect x="768" y="210" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="782" y="230" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">in-gnu-os-output-identity</text>
      <text x="782" y="246" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Durable logical IDs vs transient tokens</text>

      <rect x="768" y="268" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="782" y="288" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">in-gnu-os-desktop-profiles</text>
      <text x="782" y="304" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Atomic CAS profile store &amp; validation</text>

      <rect x="768" y="326" width="264" height="48" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="782" y="346" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="600" fill="#111418">systemd User Scopes</text>
      <text x="782" y="362" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Independent application process lifetime</text>

      <text x="770" y="420" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="500" fill="#667085">Security: Typed targets only, fail-closed</text>

      <path d="M 400 234 L 358 234" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>
      <text x="362" y="222" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">Projections</text>

      <path d="M 350 334 L 392 334" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>
      <text x="355" y="350" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">Interaction</text>

      <path d="M 350 380 Q 550 420 742 380" fill="none" stroke="#FF6A00" stroke-width="1.6" stroke-dasharray="4,3" marker-end="url(#arrow-orange)"/>
      <text x="495" y="415" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400">Typed Activation Request (SO_PEERCRED)</text>

      <rect x="50" y="475" width="1000" height="75" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1"/>
      <text x="70" y="502" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418" letter-spacing="0.08em">RUNTIME HOST &amp; COMPOSITOR REALITY</text>
      <text x="70" y="525" font-family="'Space Grotesk', sans-serif" font-size="11.5" fill="#667085">Wayland Protocol Surfaces · Hyprland Observer Socket · UWSM Session Scope · Linux cgroups v2</text>
      <rect x="850" y="492" width="180" height="42" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
      <text x="865" y="517" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">evidence/runs/</text>
"""
(OUT_DIR / "01-platform-vision.html").write_text(wrap_v341("gnosix · Platform Architecture Vision", "Type: high-level · System Overview", "A local-first Linux computing platform built around native Wayland surfaces, explicit state, and bounded authority.", d1))

# 02-surface-anatomy
d2 = """
      <text x="50" y="45" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">EXPLODED LAYER-SHELL TOPOLOGY</text>

      <g transform="translate(60, 65)">
        <rect width="660" height="60" rx="6" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.8"/>
        <rect x="1" y="1" width="658" height="24" rx="5" fill="#FFE5D2"/>
        <text x="18" y="17" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">LAYER 4 (TOP FOREGROUND) · EXCLUSIVE ZONE: 32px</text>
        <text x="18" y="44" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">gnos-ux-bar</text>
        <text x="130" y="44" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Workspace pill switcher, window breadcrumb, system telemetry</text>
        <rect x="560" y="32" width="85" height="18" rx="9" fill="#FF6A00"/>
        <text x="575" y="44" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#111418" font-weight="700">ZWLR_TOP</text>
      </g>

      <line x1="390" y1="125" x2="390" y2="155" stroke="#FF6A00" stroke-width="1.5" stroke-dasharray="3,3"/>

      <g transform="translate(60, 155)">
        <rect width="660" height="60" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect x="1" y="1" width="658" height="24" rx="5" fill="#F7F3ED"/>
        <text x="18" y="17" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#667085">LAYER 3 (QUICK ACCESS) · BOTTOM ANCHOR OVERLAY: 56px</text>
        <text x="18" y="44" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">gnos-ux-dock</text>
        <text x="140" y="44" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Pinned application launchers, running instance pips, window preview</text>
        <rect x="545" y="32" width="100" height="18" rx="9" fill="#E6E8EB"/>
        <text x="555" y="44" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#111418" font-weight="600">ZWLR_BOTTOM</text>
      </g>

      <line x1="390" y1="215" x2="390" y2="245" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="3,3"/>

      <g transform="translate(60, 245)">
        <rect width="660" height="85" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect x="1" y="1" width="658" height="24" rx="5" fill="#F7F3ED"/>
        <text x="18" y="17" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#667085">LAYER 2 (WINDOW RUNTIME) · CLIENT APPLICATION SURFACES</text>
        <text x="18" y="48" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">AppStack Geometry</text>
        <text x="160" y="48" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Tiled windows (columns/rows), floating dialogs, fullscreens</text>
        <text x="18" y="68" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="600" fill="#C95400">Focus state: Active (keyboard/seat) · Visible (painted) · Background</text>
      </g>

      <line x1="390" y1="330" x2="390" y2="360" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="3,3"/>

      <g transform="translate(60, 360)">
        <rect width="660" height="65" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect x="1" y="1" width="658" height="24" rx="5" fill="#F7F3ED"/>
        <text x="18" y="17" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#667085">LAYER 1 (SPATIAL CANVAS) · WORKSPACE BOUNDS</text>
        <text x="18" y="46" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Workspace Envelope</text>
        <text x="180" y="46" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Virtual desktop canvas, multi-output routing, spatial transitions</text>
      </g>

      <line x1="390" y1="425" x2="390" y2="455" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="3,3"/>

      <g transform="translate(60, 455)">
        <rect width="660" height="65" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <text x="18" y="28" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#A1A6AD">LAYER 0 (BACKGROUND REAR-PLANE)</text>
        <text x="18" y="50" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">gnoshow · Ambient Wallpaper Substrate</text>
        <text x="320" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Decoupled rear rendering pipeline, zero impact on UI shell latency</text>
      </g>

      <g transform="translate(760, 65)">
        <rect width="290" height="455" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="290" height="38" rx="8" fill="#F7F3ED"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Presentation Invariants</text>

        <g transform="translate(20, 55)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#C95400">Single Geometry Authority</text>
          <text x="0" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Paint boundaries and Wayland input</text>
          <text x="0" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">regions share the exact same model.</text>
        </g>

        <line x1="20" y1="120" x2="270" y2="120" stroke="#E6E8EB"/>

        <g transform="translate(20, 135)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Zero Ambient Writer Powers</text>
          <text x="0" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">The UI scene paints pixels and captures</text>
          <text x="0" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">intent, but cannot mutate profiles.</text>
        </g>

        <line x1="20" y1="200" x2="270" y2="200" stroke="#E6E8EB"/>

        <g transform="translate(20, 215)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">GPUI Hardware Acceleration</text>
          <text x="0" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Direct GPU surface rendering with</text>
          <text x="0" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Wayland zero-copy dmabuf buffers.</text>
        </g>

        <line x1="20" y1="280" x2="270" y2="280" stroke="#E6E8EB"/>

        <g transform="translate(20, 295)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Fail-Closed Boundary</text>
          <text x="0" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Stale compositor facts discard pending</text>
          <text x="0" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">actions instead of guessing state.</text>
        </g>

        <rect x="20" y="375" width="250" height="60" rx="4" fill="#FFE5D2" stroke="#FF6A00" stroke-width="1"/>
        <text x="32" y="398" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">HOST RUNTIME VERDICT</text>
        <text x="32" y="418" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="600" fill="#C95400">gnos-ux-scene · qualified main line</text>
      </g>
"""
(OUT_DIR / "02-surface-anatomy.html").write_text(wrap_v341("gnosix · Surface Anatomy & Layer Topology", "Type: exploded · Desktop Presentation", "The layered desktop presentation stack: from rear-plane wallpaper to interactive top-shell surfaces.", d2))

# 03-workspace-envelope
d3 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">MULTI-OUTPUT SPATIAL TOPOLOGY</text>

      <g transform="translate(50, 60)">
        <rect width="480" height="415" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="480" height="42" rx="8" fill="#F7F3ED"/>
        <text x="20" y="26" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Display eDP-1 · Laptop Primary (2560×1600)</text>
        <rect x="340" y="10" width="125" height="22" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="348" y="25" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">output://primary-01</text>

        <g transform="translate(20, 55)">
          <rect width="440" height="155" rx="6" fill="#F7F3ED" stroke="#FF6A00" stroke-width="1.6"/>
          <rect width="440" height="26" rx="6" fill="#FFE5D2"/>
          <text x="15" y="18" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">WORKSPACE 1 · ACTIVE &amp; FOCUSED</text>

          <rect x="15" y="38" width="200" height="100" rx="4" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.2"/>
          <text x="28" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Neovim (main.rs)</text>
          <text x="28" y="76" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">tiled · active seat</text>
          <text x="28" y="120" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#C95400">focused_surface: true</text>

          <rect x="225" y="38" width="200" height="100" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="238" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">cargo xtask ci</text>
          <text x="238" y="76" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">tiled · qualification log</text>
          <text x="238" y="120" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">visible_surface: true</text>
        </g>

        <g transform="translate(20, 222)">
          <rect width="440" height="80" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="15" y="24" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#667085">WORKSPACE 2 · INACTIVE</text>
          <text x="15" y="45" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418">Documentation: Zotero &amp; Architecture Specs</text>
          <text x="15" y="62" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#A1A6AD">surfaces: 2 preserved · rendering: suspended</text>
        </g>

        <g transform="translate(20, 314)">
          <rect width="440" height="80" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="15" y="24" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#667085">WORKSPACE 3 · INACTIVE</text>
          <text x="15" y="45" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418">Diagnostics: Systemd journal &amp; btop</text>
          <text x="15" y="62" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#A1A6AD">surfaces: 1 preserved · rendering: suspended</text>
        </g>
      </g>

      <g transform="translate(570, 60)">
        <rect width="480" height="415" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="480" height="42" rx="8" fill="#F7F3ED"/>
        <text x="20" y="26" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Display DP-1 · Studio 4K (3840×2160)</text>
        <rect x="340" y="10" width="125" height="22" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="348" y="25" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#2446A8">output://studio-4k</text>

        <g transform="translate(20, 55)">
          <rect width="440" height="155" rx="6" fill="#F7F3ED" stroke="#4169E1" stroke-width="1.6"/>
          <rect width="440" height="26" rx="6" fill="#E8EDFF"/>
          <text x="15" y="18" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#2446A8">WORKSPACE 4 · ACTIVE ON DISPLAY 2</text>

          <rect x="15" y="38" width="410" height="100" rx="4" fill="#FFFFFF" stroke="#4169E1" stroke-width="1.2"/>
          <text x="28" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">BrowserOS neo · Web Research Surface</text>
          <text x="28" y="78" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">fullscreen-tile · active on studio monitor</text>
          <text x="28" y="120" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#2446A8">cross-output focus switchable</text>
        </g>

        <g transform="translate(20, 222)">
          <rect width="440" height="80" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="15" y="24" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#667085">WORKSPACE 5 · INACTIVE</text>
          <text x="15" y="45" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418">Communication: Slack &amp; Zulip Client</text>
          <text x="15" y="62" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#A1A6AD">surfaces: 2 preserved</text>
        </g>

        <g transform="translate(20, 314)">
          <rect width="440" height="80" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="15" y="24" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#667085">WORKSPACE 6 · EMPTY ENVELOPE</text>
          <text x="15" y="45" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#A1A6AD">Unallocated Workspace Slot</text>
          <text x="15" y="62" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#A1A6AD">ready for on-demand allocation</text>
        </g>
      </g>

      <g transform="translate(50, 490)">
        <rect width="1000" height="60" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="26" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">DURABLE LOGICAL OUTPUT CONTRACT</text>
        <text x="25" y="45" font-family="'Space Grotesk', sans-serif" font-size="11.5" fill="#111418">Transient Wayland output names (eDP-1, DP-1) map to persistent logical identities via in-gnu-os-output-identity.</text>
        <text x="730" y="38" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#667085">Hotplug resilience: Qualified</text>
      </g>
"""
(OUT_DIR / "03-workspace-envelope.html").write_text(wrap_v341("gnosix · Workspace Envelope & Multi-Output Routing", "Type: nested · Multi-Monitor Layout", "Spatial mapping of workspaces across physical displays and durable logical identities.", d3))

# 04-appstack-window-model
d4 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">WINDOW SEMANTICS &amp; RECONCILIATION FLOW</text>

      <g transform="translate(50, 70)">
        <rect width="180" height="85" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <text x="15" y="28" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#C95400">COMPOSITOR EVENT</text>
        <text x="15" y="50" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Surface Ingress</text>
        <text x="15" y="68" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">openwindow / move</text>
      </g>

      <path d="M 230 112 L 270 112" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>

      <g transform="translate(270, 60)">
        <polygon points="90 0, 180 50, 90 100, 0 50" fill="#F7F3ED" stroke="#111418" stroke-width="1.2"/>
        <text x="48" y="48" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Classify Surface</text>
        <text x="52" y="62" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">Role &amp; Topology</text>
      </g>

      <path d="M 450 85 L 510 60" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>
      <path d="M 450 112 L 510 112" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>
      <path d="M 450 135 L 510 165" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>

      <g transform="translate(510, 35)">
        <rect width="170" height="45" rx="5" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Tiled Surface</text>
        <text x="12" y="37" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">Column / Grid Slots</text>
      </g>

      <g transform="translate(510, 90)">
        <rect width="170" height="45" rx="5" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Floating Utility</text>
        <text x="12" y="37" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">Transient dialog / popover</text>
      </g>

      <g transform="translate(510, 145)">
        <rect width="170" height="45" rx="5" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Fullscreen Surface</text>
        <text x="12" y="37" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">Envelope Override</text>
      </g>

      <g transform="translate(730, 45)">
        <rect width="320" height="175" rx="8" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.5"/>
        <rect width="320" height="28" rx="8" fill="#FFE5D2"/>
        <text x="16" y="19" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">SURFACE FOCUS &amp; ACTIVITY STATES</text>

        <rect x="15" y="38" width="290" height="38" rx="4" fill="#F7F3ED" stroke="#FF6A00" stroke-width="1"/>
        <text x="25" y="55" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">1. Focused (Seat Input Target)</text>
        <text x="25" y="69" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#C95400">receives keyboard &amp; pointer focus</text>

        <rect x="15" y="82" width="290" height="38" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="99" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">2. Visible (Unfocused Viewport)</text>
        <text x="25" y="113" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">painted in active workspace</text>

        <rect x="15" y="126" width="290" height="38" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="143" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">3. Background (Preserved Memory)</text>
        <text x="25" y="157" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#A1A6AD">on inactive workspace, raster skipped</text>
      </g>

      <g transform="translate(50, 260)">
        <rect width="1000" height="270" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="1000" height="36" rx="8" fill="#F7F3ED"/>
        <text x="20" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Generation-Based Reconciliation Loop (gnosix-desktop-model)</text>

        <g transform="translate(30, 55)">
          <circle cx="20" cy="20" r="14" fill="#FF6A00"/>
          <text x="16" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418" font-weight="700">A</text>
          <text x="45" y="17" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Compositor Wakeup</text>
          <text x="45" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Hyprland IPC emits event signal;</text>
          <text x="45" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">adapter fetches atomic fresh snapshot.</text>
        </g>

        <path d="M 280 95 L 320 95" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>

        <g transform="translate(330, 55)">
          <circle cx="20" cy="20" r="14" fill="#5F7F52"/>
          <text x="16" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#FFFFFF" font-weight="700">B</text>
          <text x="45" y="17" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Model Diffing</text>
          <text x="45" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Generations compared (gen_N → gen_N+1);</text>
          <text x="45" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">bounding boxes recalculated.</text>
        </g>

        <path d="M 590 95 L 630 95" fill="none" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>

        <g transform="translate(640, 55)">
          <circle cx="20" cy="20" r="14" fill="#4169E1"/>
          <text x="16" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#FFFFFF" font-weight="700">C</text>
          <text x="45" y="17" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">Presentation Projection</text>
          <text x="45" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">gnos-ux-scene schedules repaint;</text>
          <text x="45" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">surfaces updated in single frame.</text>
        </g>

        <rect x="30" y="150" width="940" height="95" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="50" y="175" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">KEY CONTRACT: OBSERVATION DOES NOT IMPLY MUTATION</text>
        <text x="50" y="196" font-family="'Space Grotesk', sans-serif" font-size="11.5" fill="#111418">gnosix-desktop-model reads state deterministically. It does not possess any ability to move windows, kill processes, or alter Hyprland configs directly. All actions flow through narrowly-scoped in.gnu.os authorities.</text>
        <text x="50" y="222" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#A1A6AD">Qualification Proof: xtask qualified under live compositor stream</text>
      </g>
"""
(OUT_DIR / "04-appstack-window-model.html").write_text(wrap_v341("gnosix · AppStack Surface & Window Semantics", "Type: flowchart · Window Lifecycle", "Deterministic window lifecycle: layout projection, spatial hit-testing, and focus states.", d4))

# 05-dock-launcher-flow
d5 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">APPLICATION LAUNCH PIPELINE</text>

      <g transform="translate(50, 70)">
        <g transform="translate(0, 0)">
          <rect width="180" height="260" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">01 · USER ACTION</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Dock Interaction</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">User clicks app icon</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">or enters query in</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">launcher prompt.</text>

          <rect x="12" y="145" width="156" height="90" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="165" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">ORIGIN HOST</text>
          <text x="20" y="183" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnos-ux-dock</text>
          <text x="20" y="200" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">unprivileged UI</text>
        </g>

        <path d="M 180 130 L 205 130" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(205, 0)">
          <rect width="180" height="260" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">02 · INTENT FORM</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Typed Launch Intent</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Generates typed</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">contract rather than</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">raw shell strings.</text>

          <rect x="12" y="145" width="156" height="90" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="165" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">PAYLOAD</text>
          <text x="20" y="183" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">app_id: "terminal"</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">target_ws: 1</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">freshness_ns: u64</text>
        </g>

        <path d="M 385 130 L 410 130" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(410, 0)">
          <rect width="180" height="260" rx="6" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.6"/>
          <rect width="180" height="34" rx="6" fill="#FFE5D2"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">03 · CALLER AUTH</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">in-gnu-os-activation</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Unix domain socket</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">authenticates caller</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">credentials.</text>

          <rect x="12" y="145" width="156" height="90" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="165" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">SECURITY CHECK</text>
          <text x="20" y="183" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#111418">SO_PEERCRED check</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">session UID verified</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">unit: gnos-ux-scene</text>
        </g>

        <path d="M 590 130 L 615 130" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(615, 0)">
          <rect width="180" height="260" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">04 · CATALOGUE CHECK</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Catalog Lookup</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Validates entry against</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">gnosix application</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">trusted catalog.</text>

          <rect x="12" y="145" width="156" height="90" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="165" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">VERIFICATION</text>
          <text x="20" y="183" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Validated Exec</text>
          <text x="20" y="200" font-family="'Space Grotesk', sans-serif" font-size="8" fill="#667085">no bash eval / injection</text>
        </g>

        <path d="M 795 130 L 820 130" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(820, 0)">
          <rect width="180" height="260" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">05 · DISPATCH SCOPE</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">systemd User Scope</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Launches under</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">user manager with</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">isolated cgroup.</text>

          <rect x="12" y="145" width="156" height="90" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="165" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">EXECUTION SCOPE</text>
          <text x="20" y="183" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#2446A8">app-terminal.scope</text>
          <text x="20" y="200" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Independent lifetime</text>
        </g>
      </g>

      <g transform="translate(50, 365)">
        <rect width="1000" height="150" rx="8" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.5"/>
        <rect width="1000" height="32" rx="8" fill="#FFE5D2"/>
        <text x="20" y="21" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">PROCESS LIFETIME ISOLATION CONTRACT</text>

        <g transform="translate(25, 50)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Why This Architecture Matters for Desktop Reliability:</text>
          <text x="0" y="38" font-family="'Space Grotesk', sans-serif" font-size="11.5" fill="#667085">• In standard shells (GNOME / KDE), launching an app inherits the shell process tree or uses ambient D-Bus daemon forks.</text>
          <text x="0" y="58" font-family="'Space Grotesk', sans-serif" font-size="11.5" fill="#667085">• If the gnos-ux-scene UI restarts or crashes, your launched terminal, code editor, or browser remains 100% unaffected.</text>
          <text x="0" y="78" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">Guaranteed by: in-gnu-os-activation → systemd user manager lifecycle qualification</text>
        </g>
      </g>
"""
(OUT_DIR / "05-dock-launcher-flow.html").write_text(wrap_v341("gnosix · Dock & Application Launch Pipeline", "Type: process · Execution Flow", "From user click in the Dock to independent systemd user scope execution.", d5, 1100, 560))

# 06-bar-system-indicators
d6 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">OUTPUT-BOUND BAR SURFACE ANATOMY</text>

      <g transform="translate(50, 65)">
        <rect width="1000" height="54" rx="27" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.6"/>
        <rect x="2" y="2" width="996" height="50" rx="25" fill="#F7F3ED"/>

        <g transform="translate(16, 10)">
          <rect width="48" height="34" rx="17" fill="#FF6A00"/>
          <text x="24" y="22" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">1</text>

          <rect x="54" width="34" height="34" rx="17" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="71" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418" text-anchor="middle">2</text>

          <rect x="94" width="34" height="34" rx="17" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="111" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418" text-anchor="middle">3</text>

          <rect x="134" width="34" height="34" rx="17" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="151" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#A1A6AD" text-anchor="middle">4</text>
        </g>

        <g transform="translate(360, 12)">
          <rect width="320" height="30" rx="15" fill="#FFFFFF" stroke="#D7DADF"/>
          <circle cx="20" cy="15" r="5" fill="#FF6A00"/>
          <text x="35" y="19" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Neovim</text>
          <text x="88" y="19" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">crates/gnos-ux-bar/src/lib.rs</text>
        </g>

        <g transform="translate(740, 10)">
          <rect width="244" height="34" rx="17" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="22" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="500" fill="#667085">WiFi 5G</text>
          <text x="85" y="22" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="500" fill="#667085">82%</text>
          <text x="125" y="22" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="500" fill="#667085">BAT 96%</text>
          <text x="195" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">10:42</text>
        </g>
      </g>

      <g transform="translate(50, 150)">
        <g transform="translate(0, 0)">
          <rect width="310" height="320" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="310" height="40" rx="8" fill="#F7F3ED"/>
          <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Workspace Pill Reactor</text>

          <g transform="translate(18, 55)">
            <text x="0" y="16" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#C95400">Event-Driven Update Flow</text>
            <text x="0" y="34" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Pill state updates instantly from</text>
            <text x="0" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">desktop model generations.</text>

            <rect y="70" width="274" height="65" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
            <text x="12" y="90" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#111418">Active: solid accent fill</text>
            <text x="12" y="106" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Occupied: outline badge</text>
            <text x="12" y="122" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#A1A6AD">Urgent: high-contrast pulse</text>

            <text x="0" y="160" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Zero Polling Overhead</text>
            <text x="0" y="178" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">No 1-second timers; bar only repaints</text>
            <text x="0" y="193" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">when a workspace transition fires.</text>
          </g>
        </g>

        <g transform="translate(345, 0)">
          <rect width="310" height="320" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="310" height="40" rx="8" fill="#F7F3ED"/>
          <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Context Breadcrumb Engine</text>

          <g transform="translate(18, 55)">
            <text x="0" y="16" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#C95400">Focus Projection</text>
            <text x="0" y="34" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Tracks currently focused surface title</text>
            <text x="0" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">across all active workspace outputs.</text>

            <rect y="70" width="274" height="65" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
            <text x="12" y="90" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#111418">App ID resolution: Catalog</text>
            <text x="12" y="106" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Truncation: Smart middle-ellipsis</text>
            <text x="12" y="122" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#A1A6AD">Media: MPRIS2 stream overlay</text>

            <text x="0" y="160" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Sanitized Titles</text>
            <text x="0" y="178" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Untrusted window title strings never</text>
            <text x="0" y="193" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">inject markup or execute shell code.</text>
          </g>
        </g>

        <g transform="translate(690, 0)">
          <rect width="310" height="320" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="310" height="40" rx="8" fill="#F7F3ED"/>
          <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Modular Telemetry Observers</text>

          <g transform="translate(18, 55)">
            <text x="0" y="16" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#C95400">Pure Passive Subscriptions</text>
            <text x="0" y="34" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">Telemetry observers read hardware facts</text>
            <text x="0" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">without mutator permissions.</text>

            <rect y="70" width="274" height="65" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
            <text x="12" y="90" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#111418">Audio: Pipewire stream state</text>
            <text x="12" y="106" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Power: UPower dbus signals</text>
            <text x="12" y="122" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#A1A6AD">Network: NetworkManager state</text>

            <text x="0" y="160" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Output Anchoring</text>
            <text x="0" y="178" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Dedicated Bar instance per screen;</text>
            <text x="0" y="193" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">geometry adjusts to exact display DPI.</text>
          </g>
        </g>
      </g>

      <g transform="translate(50, 490)">
        <rect width="1000" height="55" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="24" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">LAYER-SHELL EXCLUSIVE ZONE CONTRACT</text>
        <text x="25" y="42" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418">The Bar requests an exclusive 32px top margin. Windows cannot overlap or occlude the bar unless explicitly set to Fullscreen.</text>
      </g>
"""
(OUT_DIR / "06-bar-system-indicators.html").write_text(wrap_v341("gnosix · The Bar Presentation Architecture", "Type: high-level · Edge Surface Design", "Output-bound top edge surface: segmented modules, workspace pills, and native telemetry.", d6))

# 07-input-geometry-interaction
d7 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">INPUT GEOMETRY &amp; HIT-TESTING PIPELINE</text>

      <g transform="translate(50, 70)">
        <rect width="240" height="390" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="240" height="38" rx="8" fill="#F7F3ED"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Wayland Seat Input</text>

        <g transform="translate(18, 55)">
          <rect width="204" height="60" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="12" y="22" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Pointer Events</text>
          <text x="12" y="38" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">wl_pointer motion &amp; button</text>
          <text x="12" y="50" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">scroll &amp; hover coordinates</text>

          <rect y="75" width="204" height="60" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="12" y="22" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Keyboard Events</text>
          <text x="12" y="38" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">wl_keyboard key &amp; modifiers</text>
          <text x="12" y="50" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">xkb common keymap sync</text>

          <rect y="150" width="204" height="60" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="12" y="22" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Touch &amp; Gestures</text>
          <text x="12" y="38" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">pinch, swipe, 3-finger drag</text>
          <text x="12" y="50" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">spatial workspace switch</text>

          <text x="0" y="240" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#A1A6AD">Ingress: Linux evdev → Compositor</text>
        </g>
      </g>

      <path d="M 290 260 L 330 260" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

      <g transform="translate(330, 70)">
        <rect width="440" height="390" rx="8" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.6"/>
        <rect width="440" height="38" rx="8" fill="#FFE5D2"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">gnosix-input-core · Resolved Geometry</text>

        <g transform="translate(20, 55)">
          <rect width="400" height="230" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>

          <rect width="400" height="26" fill="#FFE5D2" stroke="#FF6A00" stroke-width="1"/>
          <text x="10" y="17" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#111418">EXCLUSIVE ZONE: BAR (32px)</text>

          <rect x="15" y="38" width="370" height="135" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Usable Workspace Canvas (AppStack)</text>
          <text x="25" y="76" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Window rects clipped exactly to safe margins</text>

          <rect x="25" y="90" width="160" height="70" rx="3" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="35" y="110" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Window A (Active)</text>
          <text x="35" y="125" font-family="'Space Grotesk', sans-serif" font-size="7.5" font-weight="600" fill="#C95400">receives input</text>

          <rect x="195" y="90" width="185" height="70" rx="3" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="205" y="110" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Window B (Visible)</text>
          <text x="205" y="125" font-family="'Space Grotesk', sans-serif" font-size="7.5" fill="#667085">passive hit-test</text>

          <rect x="100" y="185" width="200" height="35" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="120" y="206" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#667085">DOCK OVERLAY ZONE</text>
        </g>

        <g transform="translate(20, 305)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">INVARIANT: ACCIDENTAL CLICK PROTECTION</text>
          <text x="0" y="32" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Layer-shell input regions strictly match visually painted bounds.</text>
          <text x="0" y="47" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Transparent container padding never traps or absorbs clicks.</text>
        </g>
      </g>

      <path d="M 770 260 L 810 260" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

      <g transform="translate(810, 70)">
        <rect width="240" height="390" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="240" height="38" rx="8" fill="#F7F3ED"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Event Dispatch Targets</text>

        <g transform="translate(18, 55)">
          <rect width="204" height="85" rx="5" fill="#F7F3ED" stroke="#FF6A00" stroke-width="1"/>
          <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Shell Controls</text>
          <text x="12" y="42" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Bar pills, Dock icons, menus</text>
          <text x="12" y="60" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">handled in gnos-ux-scene</text>

          <rect y="105" width="204" height="85" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Client Surfaces</text>
          <text x="12" y="42" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Terminal, Browser, Editor</text>
          <text x="12" y="60" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">forwarded via Wayland wl_seat</text>

          <rect y="210" width="204" height="85" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="12" y="24" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Spatial Gestures</text>
          <text x="12" y="42" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Global swipe between desks</text>
          <text x="12" y="60" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#2446A8">admitted typed switch</text>
        </g>
      </g>

      <g transform="translate(50, 480)">
        <rect width="1000" height="65" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="26" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">SECURITY CONTRACT: EXPLICIT HIT-TESTING BOUNDARY</text>
        <text x="25" y="46" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#111418">Unlike traditional X11/Wayland shells where invisible windows could swallow clicks, gnosix binds input regions strictly to the verified desktop model geometry.</text>
      </g>
"""
(OUT_DIR / "07-input-geometry-interaction.html").write_text(wrap_v341("gnosix · Input Observation & Geometry Reservation", "Type: data-flow · Spatial Hit-Testing", "Input region mapping, margin reservations, and safe hit-testing.", d7))

# 08-workspace-switch-sequence
d8 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">WORKSPACE SWITCH SEQUENCE</text>

      <g transform="translate(50, 60)">
        <g transform="translate(40, 0)">
          <rect width="120" height="36" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="60" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">User / Seat</text>
          <line x1="60" y1="36" x2="60" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(200, 0)">
          <rect width="130" height="36" rx="6" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.5"/>
          <text x="65" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">gnos-ux-bar</text>
          <line x1="65" y1="36" x2="65" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(370, 0)">
          <rect width="150" height="36" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="75" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">in-gnu-os-activation</text>
          <line x1="75" y1="36" x2="75" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(560, 0)">
          <rect width="140" height="36" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="70" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">Hyprland IPC</text>
          <line x1="70" y1="36" x2="70" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(740, 0)">
          <rect width="130" height="36" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="65" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418" text-anchor="middle">Observer Crate</text>
          <line x1="65" y1="36" x2="65" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(900, 0)">
          <rect width="120" height="36" rx="6" fill="#FFFFFF" stroke="#4169E1" stroke-width="1.5"/>
          <text x="60" y="23" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#2446A8" text-anchor="middle">gnos-ux-scene</text>
          <line x1="60" y1="36" x2="60" y2="440" stroke="#D7DADF" stroke-dasharray="4,4"/>
        </g>

        <g transform="translate(100, 70)">
          <line x1="0" y1="0" x2="165" y2="0" stroke="#FF6A00" stroke-width="1.6" marker-end="url(#arrow-orange)"/>
          <text x="80" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400" text-anchor="middle">1. Click Pill "2"</text>
        </g>

        <g transform="translate(265, 115)">
          <line x1="0" y1="0" x2="180" y2="0" stroke="#FF6A00" stroke-width="1.6" marker-end="url(#arrow-orange)"/>
          <text x="90" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400" text-anchor="middle">2. ActivateWorkspace(W2, primary)</text>
        </g>

        <g transform="translate(445, 150)">
          <rect x="-10" y="0" width="130" height="32" rx="4" fill="#FFE5D2" stroke="#FF6A00" stroke-width="1"/>
          <text x="55" y="20" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#111418" text-anchor="middle">Peer Auth (SO_PEERCRED)</text>
        </g>

        <g transform="translate(445, 210)">
          <line x1="0" y1="0" x2="185" y2="0" stroke="#111418" stroke-width="1.5" marker-end="url(#arrow)"/>
          <text x="90" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#111418" text-anchor="middle">3. dispatch workspace 2</text>
        </g>

        <g transform="translate(630, 255)">
          <line x1="0" y1="0" x2="175" y2="0" stroke="#667085" stroke-width="1.5" stroke-dasharray="3,3" marker-end="url(#arrow)"/>
          <text x="85" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085" text-anchor="middle">4. event: workspacev2 2,DP-1</text>
        </g>

        <g transform="translate(805, 290)">
          <line x1="0" y1="0" x2="-175" y2="0" stroke="#667085" stroke-width="1.5" marker-end="url(#arrow)"/>
          <text x="-85" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085" text-anchor="middle">5. read fresh snapshot</text>
        </g>

        <g transform="translate(805, 335)">
          <line x1="0" y1="0" x2="155" y2="0" stroke="#4169E1" stroke-width="1.8" marker-end="url(#arrow-blue)"/>
          <text x="75" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8" text-anchor="middle">6. Projection: Active W2</text>
        </g>

        <g transform="translate(960, 365)">
          <rect x="-10" y="0" width="130" height="32" rx="4" fill="#E8EDFF" stroke="#4169E1" stroke-width="1"/>
          <text x="55" y="20" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#2446A8" text-anchor="middle">Repaint (GPUI Slide)</text>
        </g>

        <g transform="translate(960, 420)">
          <line x1="0" y1="0" x2="-860" y2="0" stroke="#FF6A00" stroke-width="1.4" stroke-dasharray="4,4" marker-end="url(#arrow-orange)"/>
          <text x="-430" y="-8" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400" text-anchor="middle">7. Visual Feedback Complete (W2 Active on Bar &amp; Canvas)</text>
        </g>
      </g>
"""
(OUT_DIR / "08-workspace-switch-sequence.html").write_text(wrap_v341("gnosix · Workspace Switch Sequence", "Type: sequence · Interactive Flow", "Workspace activation sequence: from user click to Hyprland dispatch and GPUI scene update.", d8, 1100, 600))

# 09-display-profiles-lifecycle
d9 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">DISPLAY SETUP &amp; PROFILE PERSISTENCE LIFECYCLE</text>

      <g transform="translate(50, 70)">
        <g transform="translate(0, 0)">
          <rect width="180" height="270" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">PHASE 1 · HARDWARE</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Hotplug Detection</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Monitor connected</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">to DisplayPort / HDMI.</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">DRM uevent received.</text>

          <rect x="12" y="160" width="156" height="85" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="180" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">TRIGGER</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#111418">udev DRM connector</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">hotplug wake-up</text>
        </g>

        <path d="M 180 135 L 205 135" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(205, 0)">
          <rect width="180" height="270" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">PHASE 2 · EXTRACTION</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">EDID Fingerprint</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Extracts immutable</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">hardware properties:</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">vendor, model, serial.</text>

          <rect x="12" y="160" width="156" height="85" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="180" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">HARDWARE ID</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#111418">sha256(edid_bytes)</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">modes &amp; frequencies</text>
        </g>

        <path d="M 385 135 L 410 135" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(410, 0)">
          <rect width="180" height="270" rx="6" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.6"/>
          <rect width="180" height="34" rx="6" fill="#FFE5D2"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">PHASE 3 · BINDING</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Logical Identity</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Maps transient DP-2</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">to persistent durable</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">logical output contract.</text>

          <rect x="12" y="160" width="156" height="85" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="180" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">DURABLE ID</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="700" fill="#C95400">output://studio-4k</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">survives re-enumeration</text>
        </g>

        <path d="M 590 135 L 615 135" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(615, 0)">
          <rect width="180" height="270" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">PHASE 4 · CAS STORE</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Profile Resolution</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Reads profiles in</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">~/.config/gnosix/</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">with atomic CAS.</text>

          <rect x="12" y="160" width="156" height="85" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="180" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">ATOMIC CAS</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#111418">compare-and-swap</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">fail-closed recovery</text>
        </g>

        <path d="M 795 135 L 820 135" fill="none" stroke="#FF6A00" stroke-width="1.8" marker-end="url(#arrow-orange)"/>

        <g transform="translate(820, 0)">
          <rect width="180" height="270" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
          <rect width="180" height="34" rx="6" fill="#F7F3ED"/>
          <text x="14" y="22" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">PHASE 5 · REALIGN</text>
          <text x="14" y="58" font-family="'Space Grotesk', sans-serif" font-size="13" font-weight="700" fill="#111418">Shell Adaptation</text>
          <text x="14" y="76" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">gnos-ux-scene spawns</text>
          <text x="14" y="90" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">Bar on new screen,</text>
          <text x="14" y="104" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#667085">restores workspaces.</text>

          <rect x="12" y="160" width="156" height="85" rx="4" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="20" y="180" font-family="'Space Grotesk', sans-serif" font-size="8" font-weight="600" fill="#A1A6AD">SURFACES LIVE</text>
          <text x="20" y="198" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">Multi-Head Active</text>
          <text x="20" y="213" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">0ms workspace loss</text>
        </g>
      </g>

      <g transform="translate(50, 375)">
        <rect width="1000" height="140" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="1000" height="32" rx="8" fill="#F7F3ED"/>
        <text x="20" y="21" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">FAIL-CLOSED INTEGRITY PRINCIPLE</text>

        <g transform="translate(25, 50)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">No Hallucinated Monitor Profiles:</text>
          <text x="0" y="34" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">• When a new monitor or cable is plugged in, gnosix never synthesizes an arbitrary layout or mutates existing configurations blindly.</text>
          <text x="0" y="52" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">• If a stored profile does not match authenticated hardware fingerprints, gnosix safely defaults to single-output containment.</text>
          <text x="0" y="70" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">Owned by: in-gnu-os-output-identity &amp; in-gnu-os-desktop-profiles</text>
        </g>
      </g>
"""
(OUT_DIR / "09-display-profiles-lifecycle.html").write_text(wrap_v341("gnosix · Display Setup & Profile Persistence", "Type: process · State Lifecycle", "Hotplug detection, durable output binding, and compare-and-swap profile restore.", d9))

# 10-desktop-architecture-comparison
d10 = """
      <text x="50" y="40" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.08em">COMPARATIVE ARCHITECTURE · MONOLITHIC SHELL VS GNOSIX</text>

      <g transform="translate(50, 65)">
        <rect width="470" height="435" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.2"/>
        <rect width="470" height="40" rx="8" fill="#F7F3ED"/>
        <text x="20" y="25" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#111418">Traditional Linux Desktop Shell</text>
        <text x="260" y="25" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#A1A6AD">GNOME / KDE / SWAY</text>

        <g transform="translate(20, 55)">
          <rect width="430" height="180" rx="6" fill="#F7F3ED" stroke="#D7DADF" stroke-width="1.2"/>
          <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#667085">MONOLITHIC SINGLE PROCESS MEMORY SPACE</text>

          <rect x="15" y="40" width="190" height="34" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="61" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">UI Presentation &amp; Panels</text>

          <rect x="220" y="40" width="190" height="34" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="230" y="61" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Wayland Compositor Core</text>

          <rect x="15" y="82" width="190" height="34" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="103" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">JavaScript Extensions</text>

          <rect x="220" y="82" width="190" height="34" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="230" y="103" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Ambient D-Bus System Mutator</text>

          <rect x="15" y="124" width="395" height="34" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="145" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Direct Process Spawner (Fork/Exec Parent of All Apps)</text>
        </g>

        <g transform="translate(20, 255)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#C95400">Architectural Pitfalls:</text>
          <text x="0" y="36" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">❌ A crash in an extension or UI widget kills the entire session.</text>
          <text x="0" y="54" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">❌ All launched applications terminate when the shell restarts.</text>
          <text x="0" y="72" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">❌ Ambient authority: UI widgets have unrestricted root/system writes.</text>
          <text x="0" y="90" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#667085">❌ Transient output names break workspace multi-monitor layout.</text>
        </g>
      </g>

      <g transform="translate(580, 65)">
        <rect width="470" height="435" rx="8" fill="#FFFFFF" stroke="#FF6A00" stroke-width="1.8"/>
        <rect width="470" height="40" rx="8" fill="#FFE5D2"/>
        <text x="20" y="25" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#111418">gnosix Decoupled Native Architecture</text>
        <text x="320" y="25" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#111418">LOCAL-FIRST LINUX</text>

        <g transform="translate(20, 55)">
          <rect width="430" height="45" rx="5" fill="#F7F3ED" stroke="#FF6A00" stroke-width="1.2"/>
          <text x="16" y="20" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Plane 1 · gnos.ux (GPUI Presentation Host)</text>
          <text x="16" y="34" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#C95400">zero writer powers · layer-shell native rendering</text>

          <rect y="52" width="430" height="45" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="16" y="72" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Plane 2 · gnosix-desktop-model (Pure Semantics)</text>
          <text x="16" y="86" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">deterministic geometry &amp; focus · zero compositor mutations</text>

          <rect y="104" width="430" height="45" rx="5" fill="#F7F3ED" stroke="#D7DADF"/>
          <text x="16" y="124" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Plane 3 · in.gnu.os (Bounded Mutation Daemons)</text>
          <text x="16" y="138" font-family="'Space Grotesk', sans-serif" font-size="8.5" fill="#667085">typed admission · SO_PEERCRED auth · fail-closed CAS</text>

          <rect y="156" width="430" height="45" rx="5" fill="#F7F3ED" stroke="#4169E1" stroke-width="1.2"/>
          <text x="16" y="176" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="700" fill="#111418">Plane 4 · systemd User Manager (Process Lifetimes)</text>
          <text x="16" y="190" font-family="'Space Grotesk', sans-serif" font-size="8.5" font-weight="600" fill="#2446A8">apps run in independent scopes · immune to UI crashes</text>
        </g>

        <g transform="translate(20, 268)">
          <text x="0" y="15" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#C95400">gnosix Guarantees:</text>
          <text x="0" y="36" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">✔ Shell UI can restart seamlessly without dropping running apps.</text>
          <text x="0" y="54" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">✔ Zero ambient authority: all system mutations are strictly authenticated.</text>
          <text x="0" y="72" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">✔ Durable logical IDs keep multi-monitor setups stable on hotplug.</text>
          <text x="0" y="90" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">Verified by: cargo xtask ci qualification ledger</text>
        </g>
      </g>

      <g transform="translate(50, 515)">
        <rect width="1000" height="45" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">DELTA VERDICT: ARCHITECTURAL INTEGRITY OVER IMPLICIT CONVENIENCE</text>
      </g>
"""
(OUT_DIR / "10-desktop-architecture-comparison.html").write_text(wrap_v341("Desktop Architecture · Traditional Shells vs gnosix", "Type: architecture-delta · Comparative Architecture", "Contrasting monolithic implicit authority with gnosix's decoupled native planes.", d10))

print("All 10 diagrams regenerated with Gnosix-DS-V3.4.1 tokens!")
