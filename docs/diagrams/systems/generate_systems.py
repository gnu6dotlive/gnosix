import os
from pathlib import Path

OUT_DIR = Path("/home/tension_atoi/Projects/gnosix/docs/diagrams/systems")
OUT_DIR.mkdir(parents=True, exist_ok=True)

def wrap_systems(title, eyebrow, desc, svg_inner, width=1100, height=580):
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

# Header Banner helper
def header_badge(num_str, title_str, subtitle_str):
    esc_sub = subtitle_str.replace("&", "&amp;")
    esc_tit = title_str.replace("&", "&amp;")
    return f"""
      <rect x="50" y="25" width="1000" height="42" rx="6" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1"/>
      <text x="70" y="50" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400" letter-spacing="0.1em">SYSTEMS &amp; AUTHORITY · {num_str}</text>
      <text x="350" y="50" font-family="'Space Grotesk', sans-serif" font-size="11.5" font-weight="400" fill="#667085">{esc_sub}</text>
      <rect x="910" y="33" width="125" height="26" rx="4" fill="#FFE5D2"/>
      <text x="922" y="50" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">gnosix_DS V3.4.1</text>
    """

# -------------------------------------------------------------
# 01-four-plane-architecture
# -------------------------------------------------------------
d1 = header_badge("VOL II · 01/14", "Four-Plane Architecture", "Core System Model · Decoupled Responsibilities") + """
      <!-- Plane 1: Semantic Observation -->
      <g transform="translate(50, 90)">
        <rect width="230" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="230" height="40" rx="8" fill="#E8EDFF"/>
        <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#2446A8">1. SEMANTIC OBSERVATION</text>
        <text x="18" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Compositor Facts</text>
        <text x="18" y="85" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Normalizes runtime reality</text>

        <rect x="15" y="110" width="200" height="60" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnosix-hyprland</text>
        <text x="25" y="152" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Socket 1 snapshots + Socket 2 wakes</text>

        <rect x="15" y="185" width="200" height="70" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="207" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnosix-desktop-model</text>
        <text x="25" y="227" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Pure deterministic state tree</text>
        <text x="25" y="243" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#5F7F52">● Monotonic Generation Clock</text>

        <rect x="15" y="275" width="200" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="295" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">BOUNDS:</text>
        <text x="25" y="315" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">No durable IDs</text>
        <text x="25" y="332" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">No process execution</text>
        <text x="25" y="349" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">No mutation authority</text>
      </g>

      <!-- Plane 2: Durable State -->
      <g transform="translate(305, 90)">
        <rect width="230" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="230" height="40" rx="8" fill="#E8EFE5"/>
        <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#455F3D">2. DURABLE STATE</text>
        <text x="18" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Identities &amp; Profiles</text>
        <text x="18" y="85" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Cryptographic CAS storage</text>

        <rect x="15" y="110" width="200" height="60" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">in-gnu-os-output-identity</text>
        <text x="25" y="152" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Durable EDID hardware mapping</text>

        <rect x="15" y="185" width="200" height="70" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="207" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">in-gnu-os-desktop-profiles</text>
        <text x="25" y="227" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">CAS document store &amp; validation</text>
        <text x="25" y="243" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#5F7F52">● Atomic CAS pointer update</text>

        <rect x="15" y="275" width="200" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="295" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">BOUNDS:</text>
        <text x="25" y="315" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">No presentation UI</text>
        <text x="25" y="332" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">No compositor polling</text>
        <text x="25" y="349" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Isolated profile CAS writes</text>
      </g>

      <!-- Plane 3: Native Presentation -->
      <g transform="translate(560, 90)">
        <rect width="230" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="230" height="40" rx="8" fill="#FFE5D2"/>
        <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#C95400">3. PRESENTATION</text>
        <text x="18" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnos.ux Surface Scene</text>
        <text x="18" y="85" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Wayland + GPUI product host</text>

        <rect x="15" y="110" width="200" height="60" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnos-ux-scene</text>
        <text x="25" y="152" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Bar · Dock · AppStack · Envelope</text>

        <rect x="15" y="185" width="200" height="70" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="207" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnos-ux-product-intent</text>
        <text x="25" y="227" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Local interaction intent generation</text>
        <text x="25" y="243" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#4169E1">● Optimistic 120Hz animations</text>

        <rect x="15" y="275" width="200" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="295" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">BOUNDS:</text>
        <text x="25" y="315" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Zero raw disk authority</text>
        <text x="25" y="332" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Zero raw shell execution</text>
        <text x="25" y="349" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Crash does not kill apps</text>
      </g>

      <!-- Plane 4: Mutation Authority -->
      <g transform="translate(815, 90)">
        <rect width="235" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="235" height="40" rx="8" fill="#111418"/>
        <text x="18" y="25" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#FFFFFF">4. MUTATION AUTHORITY</text>
        <text x="18" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">in.gnu.os Services</text>
        <text x="18" y="85" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Typed admission &amp; execution</text>

        <rect x="15" y="110" width="205" height="60" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">in-gnu-os-caller-auth</text>
        <text x="25" y="152" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">UID · cgroup session qualification</text>

        <rect x="15" y="185" width="205" height="70" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="207" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">in-gnu-os-activation</text>
        <text x="25" y="227" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Typed targets &amp; freshness checks</text>
        <text x="25" y="243" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">● systemd user manager launch</text>

        <rect x="15" y="275" width="205" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="295" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">BOUNDS:</text>
        <text x="25" y="315" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Rejects shell strings</text>
        <text x="25" y="332" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Revalidates fresh state</text>
        <text x="25" y="349" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Fails closed on drift</text>
      </g>

      <!-- Central Architectural Invariant Banner -->
      <g transform="translate(50, 490)">
        <rect width="1000" height="60" rx="8" fill="#111418"/>
        <rect x="12" y="15" width="12" height="30" rx="3" fill="#FF6A00"/>
        <text x="36" y="34" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#FFFFFF" letter-spacing="0.05em">ARCHITECTURAL INVARIANT:</text>
        <text x="245" y="34" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="500" fill="#F7F3ED">Passing data between planes does NOT transfer authority between them.</text>
        <text x="36" y="50" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#A1A6AD">Names, package paths, IPC tokens, and process presence do not grant mutation authority.</text>
      </g>
"""
(OUT_DIR / "01-four-plane-architecture.html").write_text(wrap_systems("gnosix · Four-Plane System Architecture", "Type: high-level · System Model", "Decoupling Semantic Observation, Durable State, Native Presentation, and Mutation Authority.", d1))

# -------------------------------------------------------------
# 02-authority-matrix-ledger
# -------------------------------------------------------------
d2 = header_badge("VOL II · 02/14", "Explicit Authority Ledger", "Authoritative Matrix from AUTHORITY.md") + """
      <g transform="translate(50, 85)">
        <!-- Table Header -->
        <rect width="1000" height="38" rx="6" fill="#111418"/>
        <text x="25" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF" letter-spacing="0.08em">DOMAIN SUBSYSTEM</text>
        <text x="280" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#5F7F52" letter-spacing="0.08em">EXPLICITLY OWNS</text>
        <text x="640" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FF6A00" letter-spacing="0.08em">DOES NOT IMPLY (NON-TRANSFERABLE)</text>

        <!-- Row 1: Hyprland -->
        <g transform="translate(0, 44)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Hyprland Compositor</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Current compositor runtime reality</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Durable identity or profile ownership</text>
        </g>

        <!-- Row 2: desktop-model -->
        <g transform="translate(0, 96)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnosix-desktop-model</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Pure desktop semantics &amp; reconciliation</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Observation, persistence, launch, or mutation</text>
        </g>

        <!-- Row 3: hyprland adapter -->
        <g transform="translate(0, 148)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnosix-hyprland</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Bounded Hyprland observation/integration</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Generic shell-command authority</text>
        </g>

        <!-- Row 4: gnos.ux -->
        <g transform="translate(0, 200)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnos.ux Presentation</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Product presentation &amp; interaction intent</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Durable writer, compositor-wide authority, exec</text>
        </g>

        <!-- Row 5: in.gnu.os profiles -->
        <g transform="translate(0, 252)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in.gnu.os Identity / Profiles</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Durable logical identity &amp; profile store</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Presentation ownership or generic config auth</text>
        </g>

        <!-- Row 6: in.gnu.os activation -->
        <g transform="translate(0, 304)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in.gnu.os Activation</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Typed admission &amp; bounded execution</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Arbitrary command execution or profile mutation</text>
        </g>

        <!-- Row 7: systemd user manager -->
        <g transform="translate(0, 356)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">systemd User Manager</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Independent app process lifetime</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Semantic selection or launch-target authority</text>
        </g>

        <!-- Row 8: gnoshow -->
        <g transform="translate(0, 408)">
          <rect width="1000" height="46" rx="4" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnoshow Substrate</text>
          <text x="280" y="28" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#455F3D">Rear-plane wallpaper &amp; shader rendering</text>
          <rect x="635" y="12" width="345" height="24" rx="4" fill="#F7F3ED"/>
          <text x="645" y="28" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Desktop semantics, activation, or profile mutation</text>
        </g>
      </g>
"""
(OUT_DIR / "02-authority-matrix-ledger.html").write_text(wrap_systems("gnosix · Explicit Authority Ledger", "Type: high-level · Authority Boundaries", "Mapping domain ownership vs non-implication invariants across all core subsystems.", d2))

# -------------------------------------------------------------
# 03-desktop-model-reconciliation
# -------------------------------------------------------------
d3 = header_badge("VOL II · 03/14", "Desktop Model State Reconciliation", "Deterministic Semantic State Tree") + """
      <!-- Step 1: Input Facts -->
      <g transform="translate(60, 110)">
        <rect width="250" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="20" y="32" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#2446A8">INPUT ADAPTER FACTS</text>
        <text x="20" y="52" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">External untrusted telemetry</text>

        <rect x="15" y="75" width="220" height="85" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="98" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Compositor Snapshot</text>
        <text x="25" y="118" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Monitors · Workspaces · Surfaces</text>
        <text x="25" y="136" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Z-Order &amp; Focus ID Tokens</text>

        <rect x="15" y="175" width="220" height="85" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="198" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Durable Profile Projections</text>
        <text x="25" y="218" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Logical Output Associations</text>
        <text x="25" y="236" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Pinned Workspaces &amp; Struts</text>

        <rect x="15" y="275" width="220" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="298" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">ISOLATION INVARIANT:</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">DesktopModel does not poll</text>
        <text x="25" y="335" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Adapters push explicit facts</text>
      </g>

      <!-- Arrow 1 -> 2 -->
      <line x1="310" y1="300" x2="400" y2="300" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Step 2: Pure State Engine -->
      <g transform="translate(410, 110)">
        <rect width="280" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="280" height="40" rx="8" fill="#FFE5D2"/>
        <text x="20" y="25" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#111418">gnosix-desktop-model</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Pure Deterministic Core</text>

        <!-- Generation Clock -->
        <rect x="15" y="85" width="250" height="65" rx="6" fill="#111418"/>
        <text x="25" y="110" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#FF6A00">MONOTONIC GENERATION</text>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="14" font-weight="700" fill="#FFFFFF">Gen ID: #104829</text>

        <!-- Model Graph Nodes -->
        <rect x="15" y="165" width="250" height="110" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="187" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Topological State Graph:</text>
        <text x="25" y="208" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">├─ OutputMap [Primary / Secondary]</text>
        <text x="25" y="226" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">├─ WorkspaceEnvelope [1..N]</text>
        <text x="25" y="244" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">├─ SurfaceStack [Z-Layers, Focus]</text>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#5F7F52">└─ SpatialAdjacency (Left/Right)</text>

        <rect x="15" y="290" width="250" height="75" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="312" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#455F3D">RECONCILIATION:</text>
        <text x="25" y="332" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Diff old vs new generation</text>
        <text x="25" y="348" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Emits minimal typed delta set</text>
      </g>

      <!-- Arrow 2 -> 3 -->
      <line x1="690" y1="300" x2="780" y2="300" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Step 3: Output Projection -->
      <g transform="translate(790, 110)">
        <rect width="250" height="380" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="20" y="32" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#455F3D">IMMUTABLE SNAPSHOT</text>
        <text x="20" y="52" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#667085">Read-only presentation projection</text>

        <rect x="15" y="75" width="220" height="85" rx="6" fill="#E8EFE5" stroke="#D7DADF"/>
        <text x="25" y="98" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">gnos.ux Presentation</text>
        <text x="25" y="118" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Bar &amp; Dock render state</text>
        <text x="25" y="136" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Window slot geometries</text>

        <rect x="15" y="175" width="220" height="85" rx="6" fill="#E8EDFF" stroke="#D7DADF"/>
        <text x="25" y="198" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#111418">Mutation Verifiers</text>
        <text x="25" y="218" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">in-gnu-os-activation checks</text>
        <text x="25" y="236" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Validates target freshness</text>

        <rect x="15" y="275" width="220" height="85" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="298" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">FAIL-CLOSED RULE:</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#C95400">If snapshot generation drifts,</text>
        <text x="25" y="335" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#C95400">pending mutations are rejected.</text>
      </g>
"""
(OUT_DIR / "03-desktop-model-reconciliation.html").write_text(wrap_systems("gnosix · Desktop Model State Reconciliation", "Type: data-flow · State Engine", "Pure deterministic reconciliation of outputs, workspaces, and window surface topologies.", d3))

# -------------------------------------------------------------
# 04-hyprland-observation-adapter
# -------------------------------------------------------------
d4 = header_badge("VOL II · 04/14", "Compositor Observation Adapter", "gnosix-hyprland · Event Triggers vs Snapshot Truth") + """
      <!-- Socket 2: Event Stream -->
      <g transform="translate(60, 110)">
        <rect width="280" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="280" height="38" rx="8" fill="#E8EDFF"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#2446A8">SOCKET 2 · EVENT STREAM</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">ROLE: Wake-Up Signals Only</text>
        <text x="20" y="85" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Never treated as complete state truth</text>

        <rect x="15" y="105" width="250" height="150" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Incoming Push Events:</text>
        <text x="25" y="150" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● workspacev2>>3,DP-1</text>
        <text x="25" y="170" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● activewindow>>code,Visual Studio</text>
        <text x="25" y="190" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● openwindow>>0x55a1b,firefox</text>
        <text x="25" y="210" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● closewindow>>0x55a1b</text>
        <text x="25" y="235" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8">ACTION: Set Dirty Flag (Debounced)</text>

        <rect x="15" y="270" width="250" height="90" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="292" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">WHY NOT DIRECT STATE?</text>
        <text x="25" y="312" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Event drops or out-of-order delivery</text>
        <text x="25" y="330" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">would desynchronize the desktop model.</text>
      </g>

      <!-- Wake-up Transition Arrow -->
      <line x1="340" y1="280" x2="430" y2="280" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>
      <text x="350" y="270" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">Trigger Query</text>

      <!-- Socket 1: Snapshot Read -->
      <g transform="translate(440, 110)">
        <rect width="280" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="280" height="38" rx="8" fill="#E8EFE5"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#455F3D">SOCKET 1 · SNAPSHOT READ</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#5F7F52">ROLE: Runtime Truth</text>
        <text x="20" y="85" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Synchronous, atomic batch query</text>

        <rect x="15" y="105" width="250" height="150" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Fresh Batch Pull Commands:</text>
        <text x="25" y="150" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● j/workspaces (Complete list)</text>
        <text x="25" y="170" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● j/clients (Full window tree)</text>
        <text x="25" y="190" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● j/monitors (Active outputs &amp; modes)</text>
        <text x="25" y="210" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● j/layers (Layer-shell surfaces)</text>
        <text x="25" y="235" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#455F3D">RESULT: Full Compositor Snapshot</text>

        <rect x="15" y="270" width="250" height="90" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="292" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">ATOMIC FRESHNESS:</text>
        <text x="25" y="312" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">All collections read in lockstep</text>
        <text x="25" y="330" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Guarantees zero partial state reads.</text>
      </g>

      <!-- Adapter Arrow -->
      <line x1="720" y1="280" x2="810" y2="280" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>
      <text x="735" y="270" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">Normalize</text>

      <!-- Normalized Desktop State -->
      <g transform="translate(820, 110)">
        <rect width="220" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="220" height="38" rx="8" fill="#111418"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#FFFFFF">NORMALIZED OUTPUT</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnosix-desktop-model</text>

        <rect x="15" y="90" width="190" height="130" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="115" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Emitted Facts:</text>
        <text x="25" y="138" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">✔ Output Geometries</text>
        <text x="25" y="158" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">✔ Workspace Bindings</text>
        <text x="25" y="178" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">✔ Window Coordinates</text>
        <text x="25" y="198" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#5F7F52">✔ Monotonic Gen Bump</text>

        <rect x="15" y="240" width="190" height="120" rx="6" fill="#FFE5D2"/>
        <text x="25" y="265" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">AUTHORITY BOUNDARY:</text>
        <text x="25" y="285" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">gnosix-hyprland is strictly</text>
        <text x="25" y="302" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">an observer. It has NO shell</text>
        <text x="25" y="320" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">dispatch or execute authority.</text>
      </g>
"""
(OUT_DIR / "04-hyprland-observation-adapter.html").write_text(wrap_systems("gnosix · Compositor Observation Adapter", "Type: process · Observation Pipeline", "Event streams as wake-up triggers versus full socket snapshots as runtime truth.", d4))

print("Batch 1 (Diagrams 01-04) generated!")
# -------------------------------------------------------------
# 05-caller-auth-boundary
# -------------------------------------------------------------
d5 = header_badge("VOL II · 05/14", "Caller Authentication Boundary", "in-gnu-os-caller-auth · Peer Verification Sequence") + """
      <!-- Lifeline Headers -->
      <g transform="translate(60, 95)">
        <!-- Lifeline 1: Client -->
        <g transform="translate(0, 0)">
          <rect width="180" height="40" rx="6" fill="#111418"/>
          <text x="25" y="25" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">gnos-ux-scene (UI)</text>
          <line x1="90" y1="40" x2="90" y2="390" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="4 4"/>
        </g>

        <!-- Lifeline 2: D-Bus Broker -->
        <g transform="translate(260, 0)">
          <rect width="180" height="40" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
          <text x="32" y="25" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">dbus-broker (Bus)</text>
          <line x1="90" y1="40" x2="90" y2="390" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="4 4"/>
        </g>

        <!-- Lifeline 3: Caller Auth -->
        <g transform="translate(520, 0)">
          <rect width="200" height="40" rx="6" fill="#FFE5D2" stroke="#FF6A00"/>
          <text x="22" y="25" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">in-gnu-os-caller-auth</text>
          <line x1="100" y1="40" x2="100" y2="390" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="4 4"/>
        </g>

        <!-- Lifeline 4: Target Service -->
        <g transform="translate(790, 0)">
          <rect width="190" height="40" rx="6" fill="#E8EFE5" stroke="#5F7F52"/>
          <text x="20" y="25" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">in-gnu-os-activation</text>
          <line x1="95" y1="40" x2="95" y2="390" stroke="#D7DADF" stroke-width="1.5" stroke-dasharray="4 4"/>
        </g>

        <!-- Step 1: Client sends D-Bus call -->
        <line x1="90" y1="70" x2="350" y2="70" stroke="#111418" stroke-width="2" marker-end="url(#arrow)"/>
        <text x="110" y="63" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#111418">1. InvokeMethod("ActivateWorkspace", target: W3)</text>

        <!-- Step 2: Extract credentials -->
        <line x1="350" y1="110" x2="620" y2="110" stroke="#2446A8" stroke-width="2" marker-end="url(#arrow-blue)"/>
        <text x="365" y="103" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#2446A8">2. SO_PEERCRED (PID: 2841, UID: 1000)</text>

        <!-- Step 3: Auth Checks (Self loop) -->
        <rect x="580" y="135" width="220" height="85" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="590" y="152" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">3. THREE-FOLD VERIFICATION:</text>
        <text x="590" y="170" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ UID matches session user (1000)</text>
        <text x="590" y="188" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ cgroup in app-gnos.ux.slice</text>
        <text x="590" y="206" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ /proc/[PID]/exe matches binary</text>

        <!-- Step 4: Admitted Dispatch -->
        <line x1="620" y1="240" x2="885" y2="240" stroke="#5F7F52" stroke-width="2" marker-end="url(#arrow-green)"/>
        <text x="635" y="233" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#455F3D">4. Admitted: Forward Typed Request</text>

        <!-- Step 5: Execute & Return Result -->
        <line x1="885" y1="280" x2="90" y2="280" stroke="#455F3D" stroke-width="1.5" stroke-dasharray="3 3" marker-end="url(#arrow-green)"/>
        <text x="320" y="273" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#455F3D">5. ActivationResult::Success (Fresh)</text>

        <!-- Fail-Closed Branch -->
        <g transform="translate(0, 310)">
          <rect width="980" height="65" rx="6" fill="#F7F3ED" stroke="#FF6A00" stroke-width="1.5"/>
          <rect x="15" y="12" width="110" height="22" rx="4" fill="#FFE5D2"/>
          <text x="22" y="27" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">FAIL-CLOSED:</text>
          <text x="140" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Unauthorized Peer Rejection</text>
          <text x="140" y="48" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">If cgroup != session slice or UID != desktop user: immediate drop with Error.AccessDenied. No fallback to permissive mode.</text>
        </g>
      </g>
"""
(OUT_DIR / "05-caller-auth-boundary.html").write_text(wrap_systems("gnosix · D-Bus Caller Authentication Boundary", "Type: sequence · Peer Verification", "Verifying peer UID, systemd session slice, and executable provenance before request admission.", d5))

# -------------------------------------------------------------
# 06-typed-activation-protocol
# -------------------------------------------------------------
d6 = header_badge("VOL II · 06/14", "Typed Activation Protocol", "in-gnu-os-activation · Freshness & Type Safety") + """
      <!-- Flowchart Boxes -->
      <!-- Node 1: Inbound Request -->
      <g transform="translate(60, 110)">
        <rect width="210" height="110" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="18" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">INBOUND REQUEST</text>
        <rect x="12" y="40" width="186" height="58" rx="4" fill="#F7F3ED"/>
        <text x="20" y="58" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Target::Workspace(3)</text>
        <text x="20" y="74" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">req_generation: 104828</text>
        <text x="20" y="88" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">caller_pid: 2841</text>
      </g>

      <line x1="270" y1="165" x2="330" y2="165" stroke="#111418" stroke-width="2" marker-end="url(#arrow)"/>

      <!-- Node 2: Type Guard -->
      <g transform="translate(330, 110)">
        <rect width="210" height="110" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="18" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">GATE 1: TYPE CHECK</text>
        <rect x="12" y="40" width="186" height="58" rx="4" fill="#E8EFE5"/>
        <text x="20" y="58" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#455F3D">✔ Strictly Typed Enum</text>
        <text x="20" y="74" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">WorkspaceId(u32)</text>
        <text x="20" y="88" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#C95400">✗ Reject shell strings</text>
      </g>

      <line x1="540" y1="165" x2="600" y2="165" stroke="#111418" stroke-width="2" marker-end="url(#arrow)"/>

      <!-- Node 3: Freshness Guard -->
      <g transform="translate(600, 110)">
        <rect width="210" height="110" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="18" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">GATE 2: FRESHNESS</text>
        <rect x="12" y="40" width="186" height="58" rx="4" fill="#FFE5D2"/>
        <text x="20" y="58" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#C95400">Generation Drift Check</text>
        <text x="20" y="74" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">req.gen &gt;= cur_gen - 1</text>
        <text x="20" y="88" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Prevents stale replay</text>
      </g>

      <line x1="810" y1="165" x2="870" y2="165" stroke="#111418" stroke-width="2" marker-end="url(#arrow)"/>

      <!-- Node 4: Semantic Guard -->
      <g transform="translate(870, 110)">
        <rect width="180" height="110" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <text x="18" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">GATE 3: SEMANTICS</text>
        <rect x="12" y="40" width="156" height="58" rx="4" fill="#E8EDFF"/>
        <text x="18" y="58" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#2446A8">Active Output Map</text>
        <text x="18" y="74" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">Target is routable</text>
        <text x="18" y="88" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Output is connected</text>
      </g>

      <!-- Outcome Branches -->
      <!-- Success Path Down -->
      <line x1="960" y1="220" x2="960" y2="290" stroke="#5F7F52" stroke-width="2" marker-end="url(#arrow-green)"/>
      <g transform="translate(560, 290)">
        <rect width="490" height="110" rx="8" fill="#E8EFE5" stroke="#5F7F52" stroke-width="1.5"/>
        <text x="20" y="32" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#455F3D">ADMITTED &amp; BOUNDED EXECUTION</text>
        <text x="20" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#111418">Dispatches exact protocol call to compositor (zwlr_workspace_v1::activate)</text>
        <text x="20" y="78" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#455F3D">✔ Returns: ActivationResult::Success (Updates monotonic generation to #104830)</text>
        <text x="20" y="96" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Audit logged with caller cgroup and execution timestamp</text>
      </g>

      <!-- Failure Path Down -->
      <line x1="705" y1="220" x2="705" y2="290" stroke="#C95400" stroke-width="2" marker-end="url(#arrow-orange)"/>
      <g transform="translate(60, 290)">
        <rect width="470" height="110" rx="8" fill="#FAF8F5" stroke="#FF6A00" stroke-width="1.5"/>
        <text x="20" y="32" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#C95400">FAIL-CLOSED REJECTION</text>
        <text x="20" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#111418">Target is dropped immediately without performing side-effects.</text>
        <text x="20" y="78" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="600" fill="#C95400">✗ Returns: ActivationResult::StaleTarget or UnsupportedTarget</text>
        <text x="20" y="96" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Prevents desktop race conditions during rapid workspace switching</text>
      </g>

      <!-- Bottom Rule -->
      <g transform="translate(60, 420)">
        <rect width="990" height="42" rx="6" fill="#111418"/>
        <text x="25" y="26" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#FFFFFF">CRITICAL INVARIANT:</text>
        <text x="180" y="26" font-family="'Space Grotesk', sans-serif" font-size="10.5" fill="#F7F3ED">Requests that can mutate the desktop carry typed targets and freshness boundaries.</text>
      </g>
"""
(OUT_DIR / "06-typed-activation-protocol.html").write_text(wrap_systems("gnosix · Typed Activation & Freshness Protocol", "Type: flowchart · Admission Protocol", "Strict typed payload admission, freshness boundary validation, and bounded compositor dispatch.", d6))

# -------------------------------------------------------------
# 07-app-launch-systemd-lifecycle
# -------------------------------------------------------------
d7 = header_badge("VOL II · 07/14", "Application Launch Lifecycle", "Independent Process Lifetimes via systemd User Manager") + """
      <!-- Dock Interaction -->
      <g transform="translate(60, 105)">
        <rect width="210" height="340" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="210" height="36" rx="8" fill="#FFE5D2"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">1. PRESENTATION PLANE</text>
        <text x="18" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnos.ux Dock</text>
        <text x="18" y="78" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">User activates app icon</text>

        <rect x="15" y="100" width="180" height="70" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="122" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">LaunchIntent:</text>
        <text x="25" y="140" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Catalog Desktop ID</text>
        <text x="25" y="156" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#2446A8">"org.wezfurlong.wezterm"</text>

        <rect x="15" y="185" width="180" height="135" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="208" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">LIFETIME BOUNDARY:</text>
        <text x="25" y="228" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">gnos-ux-scene does NOT</text>
        <text x="25" y="244" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">fork or exec the process.</text>
        <text x="25" y="264" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#C95400">Scene restart will NEVER</text>
        <text x="25" y="280" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#C95400">kill running applications.</text>
      </g>

      <!-- Arrow 1 -> 2 -->
      <line x1="270" y1="230" x2="350" y2="230" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Activation Service -->
      <g transform="translate(350, 105)">
        <rect width="260" height="340" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="260" height="36" rx="8" fill="#111418"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">2. ACTIVATION AUTHORITY</text>
        <text x="18" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in-gnu-os-activation</text>
        <text x="18" y="78" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Typed launch admission</text>

        <rect x="15" y="100" width="230" height="100" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="122" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Admission Checks:</text>
        <text x="25" y="142" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ Desktop entry in allowed catalog</text>
        <text x="25" y="160" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ Exec path resolves to verified bin</text>
        <text x="25" y="178" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">✔ Caller credentials authorized</text>

        <rect x="15" y="215" width="230" height="105" rx="6" fill="#E8EDFF" stroke="#D7DADF"/>
        <text x="25" y="238" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#2446A8">systemd D-Bus Call:</text>
        <text x="25" y="258" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">StartTransientUnit(</text>
        <text x="35" y="274" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">  "app-wezterm-2894.scope",</text>
        <text x="35" y="290" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">  "fail", ...)</text>
      </g>

      <!-- Arrow 2 -> 3 -->
      <line x1="610" y1="230" x2="690" y2="230" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- systemd User Manager -->
      <g transform="translate(690, 105)">
        <rect width="350" height="340" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="350" height="36" rx="8" fill="#E8EFE5"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">3. PROCESS LIFETIME OWNER</text>
        <text x="18" y="60" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">systemd User Manager (PID 1)</text>
        <text x="18" y="78" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">UWSM Session Scope cgroup</text>

        <rect x="15" y="100" width="320" height="110" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="122" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Cgroup Subtree Allocation:</text>
        <text x="25" y="145" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">user-1000.slice</text>
        <text x="35" y="163" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">└─ session.slice (UWSM)</text>
        <text x="45" y="181" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">    ├─ app.slice / app-wezterm-2894.scope [APP]</text>
        <text x="45" y="199" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#2446A8">    └─ app.slice / app-gnos.ux.slice [SCENE]</text>

        <rect x="15" y="225" width="320" height="95" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="248" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#5F7F52">CRASH ISOLATION PROOF:</text>
        <text x="25" y="268" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">If app-gnos.ux.slice fails or restarts,</text>
        <text x="25" y="286" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">app-wezterm-2894.scope remains completely</text>
        <text x="25" y="304" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">unaffected and responsive to Wayland.</text>
      </g>

      <!-- Bottom Banner -->
      <g transform="translate(60, 465)">
        <rect width="980" height="42" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="26" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">INVARIANT:</text>
        <text x="110" y="26" font-family="'Space Grotesk', sans-serif" font-size="10" fill="#111418">Process existence alone is never treated as readiness; transient units bound process lifecycles to user sessions.</text>
      </g>
"""
(OUT_DIR / "07-app-launch-systemd-lifecycle.html").write_text(wrap_systems("gnosix · Application Launch & Process Isolation", "Type: process · Process Lifecycle", "Transient systemd user scopes ensuring launched applications outlive presentation crash or restart.", d7))

# -------------------------------------------------------------
# 08-durable-output-identity
# -------------------------------------------------------------
d8 = header_badge("VOL II · 08/14", "Durable Output Identity Resolution", "in-gnu-os-output-identity · Hardware EDID vs Ephemeral Connectors") + """
      <!-- Step 1: Volatile Compositor Reality -->
      <g transform="translate(60, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#FFE5D2"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">VOLATILE HARDWARE TOKENS</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Session Compositor Reality</text>

        <rect x="15" y="85" width="220" height="110" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Transient Names:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● DP-1 (Port connection)</text>
        <text x="25" y="146" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● HDMI-A-1 (Port connection)</text>
        <text x="25" y="164" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Connector ID: 32</text>
        <text x="25" y="182" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#C95400">⚠ Changes across driver reloads!</text>

        <rect x="15" y="210" width="220" height="145" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="232" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">EDID Hardware Facts:</text>
        <text x="25" y="252" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Vendor: DEL (Dell Inc.)</text>
        <text x="25" y="270" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Model: U2720Q</text>
        <text x="25" y="288" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Serial: 8740B1309A</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Checksum: 0x8F4A</text>
        <text x="25" y="335" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#5F7F52">Immutable display identity</text>
      </g>

      <!-- Arrow 1 -> 2 -->
      <line x1="310" y1="290" x2="390" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Step 2: Resolution Core -->
      <g transform="translate(390, 110)">
        <rect width="300" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="300" height="36" rx="8" fill="#111418"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">RESOLUTION CORE</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in-gnu-os-output-identity</text>

        <rect x="15" y="85" width="270" height="135" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Cryptographic Projection Engine:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">1. Ingest hardware EDID bytes</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">2. Compute canonical SHA-256 fingerprint</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">3. Associate with port topology index</text>
        <text x="25" y="184" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">4. Emit Durable Output Handle</text>
        <text x="25" y="202" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#2446A8">Logical ID: "out-del-u2720q-8f4a"</text>

        <rect x="15" y="235" width="270" height="120" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="258" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">AMBIGUITY CHECK (FAIL-CLOSED):</text>
        <text x="25" y="278" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">If two identical monitors share EDID</text>
        <text x="25" y="296" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">and serials are missing: fails closed</text>
        <text x="25" y="314" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">instead of guessing layout assignment.</text>
      </g>

      <!-- Arrow 2 -> 3 -->
      <line x1="690" y1="290" x2="770" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Step 3: Durable Profile Bindings -->
      <g transform="translate(770, 110)">
        <rect width="270" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="270" height="36" rx="8" fill="#E8EFE5"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">BOUND PROFILE CONFIG</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in-gnu-os-desktop-profiles</text>

        <rect x="15" y="85" width="240" height="145" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Stable Configuration Maps:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Bound Workspaces: 1, 2, 3</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Scale Factor: 1.5 (HiDPI)</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Position: (0, 0) Primary</text>
        <text x="25" y="184" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Preferred Mode: 3840x2160@60Hz</text>
        <text x="25" y="210" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#5F7F52">Survives cable re-plug &amp; reboot</text>

        <rect x="15" y="245" width="240" height="110" rx="6" fill="#FFE5D2"/>
        <text x="25" y="268" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">INVARIANT:</text>
        <text x="25" y="288" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">Durable identities are never</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">inferred from transient compositor</text>
        <text x="25" y="324" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">tokens or temporary geometry.</text>
      </g>
"""
(OUT_DIR / "08-durable-output-identity.html").write_text(wrap_systems("gnosix · Durable Output Identity Resolution", "Type: data-flow · Hardware Identity", "Mapping volatile compositor connectors to cryptographic hardware identities with fail-closed resolution.", d8))

# -------------------------------------------------------------
# 09-cas-profile-store
# -------------------------------------------------------------
d9 = header_badge("VOL II · 09/14", "Content-Addressed Profile Store", "in-gnu-os-desktop-profiles · CAS Versioning & Read Projections") + """
      <!-- Document Authoring -->
      <g transform="translate(60, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#E8EDFF"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">1. PROFILE DOCUMENT</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Structured Schema</text>

        <rect x="15" y="85" width="220" height="140" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Profile Content:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Version: "3.4.1"</text>
        <text x="25" y="146" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Pinned Dock Applications</text>
        <text x="25" y="164" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Display Geometry Presets</text>
        <text x="25" y="182" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Accent Palette Tokens</text>
        <text x="25" y="205" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8">Validated against JSON schema</text>

        <rect x="15" y="240" width="220" height="115" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">TRANSACTION RULE:</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Profiles are strictly immutable</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">once committed. Changes create</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">new cryptographic revisions.</text>
      </g>

      <!-- Arrow 1 -> 2 -->
      <line x1="310" y1="290" x2="390" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- CAS Storage Engine -->
      <g transform="translate(390, 110)">
        <rect width="320" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="320" height="36" rx="8" fill="#111418"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">2. CAS REVISION STORAGE</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Content-Addressed Store</text>

        <rect x="15" y="85" width="290" height="140" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Immutable Blob Graph:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">cas/blobs/7f2a... [Rev 1 - Initial]</text>
        <text x="25" y="150" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">cas/blobs/b419... [Rev 2 - Workspace add]</text>
        <text x="25" y="170" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#455F3D">cas/blobs/e982... [Rev 3 - Current HEAD]</text>
        <text x="25" y="195" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#FF6A00">Atomic Pointer: active.json -> e982...</text>
        <text x="25" y="210" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#111418">Guarded via Compare-And-Swap (CAS)</text>

        <rect x="15" y="240" width="290" height="115" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#455F3D">ZERO-RISK RECOVERY:</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Any corrupt write aborts immediately.</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Reverting to any past revision takes &lt;1ms</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">by updating the atomic manifest pointer.</text>
      </g>

      <!-- Arrow 2 -> 3 -->
      <line x1="710" y1="290" x2="790" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Presentation Projection -->
      <g transform="translate(790, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#E8EFE5"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">3. PROJECTION CLIENT</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnos-ux-profile-client</text>

        <rect x="15" y="85" width="220" height="140" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Read-Only Projection:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Memory-mapped read buffers</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Zero filesystem write syscalls</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Fast startup cached render</text>
        <text x="25" y="188" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#455F3D">✔ Verified Read Projection</text>

        <rect x="15" y="240" width="220" height="115" rx="6" fill="#FFE5D2"/>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">AUTHORITY BARRIER:</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">gnos.ux consumes effective</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">projections. It NEVER receives</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">raw profile-writer authority.</text>
      </g>
"""
(OUT_DIR / "09-cas-profile-store.html").write_text(wrap_systems("gnosix · Content-Addressed Profile Store", "Type: process · Immutable Storage", "Cryptographic CAS revisions, transactional pointer updates, and read-only presentation projection.", d9))

print("Batch 2 (Diagrams 05-09) generated!")
# -------------------------------------------------------------
# 10-input-observation-arbitration
# -------------------------------------------------------------
d10 = header_badge("VOL II · 10/14", "Input Observation & Arbitration", "gnosix-input-core · Passive Tap vs Zero Synthetic Injection") + """
      <!-- Device Hardware -->
      <g transform="translate(60, 110)">
        <rect width="240" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="240" height="36" rx="8" fill="#E8EDFF"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">HARDWARE INPUT LAYER</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Kernel evdev Subsystem</text>

        <rect x="15" y="85" width="210" height="120" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Physical Input Devices:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● /dev/input/event0 [Keyboard]</text>
        <text x="25" y="146" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● /dev/input/event1 [Trackpad]</text>
        <text x="25" y="164" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● /dev/input/event2 [Pointer]</text>
        <text x="25" y="185" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8">libinput driver arbitration</text>

        <rect x="15" y="220" width="210" height="135" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="242" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">PRIMARY EVENT ROUTING:</text>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Events deliver directly to the</text>
        <text x="25" y="280" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Wayland Compositor for active</text>
        <text x="25" y="298" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">surface window focus.</text>
        <text x="25" y="325" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#455F3D">✔ Standard Wayland Focus Path</text>
      </g>

      <!-- Fork Lines -->
      <line x1="300" y1="210" x2="390" y2="180" stroke="#5F7F52" stroke-width="2" marker-end="url(#arrow-green)"/>
      <line x1="300" y1="290" x2="390" y2="340" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Top Path: Compositor Normal Focus -->
      <g transform="translate(390, 110)">
        <rect width="320" height="160" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="320" height="32" rx="8" fill="#E8EFE5"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">WAYLAND COMPOSITOR DISPATCH</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Direct Focused Surface Dispatch</text>
        <text x="18" y="75" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">wl_keyboard, wl_pointer events sent to active window client</text>
        <rect x="15" y="90" width="290" height="55" rx="4" fill="#F7F3ED"/>
        <text x="25" y="112" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#455F3D">Zero Snooping By Default:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Unfocused windows receive 0 keyboard keystrokes.</text>
      </g>

      <!-- Bottom Path: gnosix-input-observation -->
      <g transform="translate(390, 290)">
        <rect width="320" height="190" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="320" height="32" rx="8" fill="#FFE5D2"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">gnosix-input-observation</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Bounded Passive Observation</text>
        <text x="18" y="75" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Passive gesture / chord recognition (swipe gestures, hotkeys)</text>

        <rect x="15" y="90" width="290" height="85" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="110" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Recognized Gestures:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">● 3-Finger Horizontal Swipe -> Workspace Switch Intent</text>
        <text x="25" y="144" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">● 4-Finger Pinch -> AppStack Overview Intent</text>
        <text x="25" y="162" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#FF6A00">Emits semantic intent signal only</text>
      </g>

      <!-- Arrow to Barrier -->
      <line x1="710" y1="385" x2="770" y2="385" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Right Box: Injection Prohibition Barrier -->
      <g transform="translate(770, 110)">
        <rect width="270" height="370" rx="8" fill="#111418"/>
        <text x="20" y="32" font-family="'Space Grotesk', sans-serif" font-size="12" font-weight="700" fill="#FF6A00">SECURITY BARRIER</text>
        <text x="20" y="52" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="600" fill="#FFFFFF">Zero Synthetic Injection</text>

        <rect x="15" y="75" width="240" height="130" rx="6" fill="#20242B"/>
        <text x="25" y="98" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#FF6A00">ABSOLUTE PROHIBITION:</text>
        <text x="25" y="120" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#F7F3ED">✗ NO /dev/uinput access</text>
        <text x="25" y="140" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#F7F3ED">✗ NO synthetic key injection</text>
        <text x="25" y="160" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#F7F3ED">✗ NO fake pointer clicks</text>
        <text x="25" y="180" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#F7F3ED">✗ NO accessibility hook replay</text>

        <rect x="15" y="220" width="240" height="135" rx="6" fill="#20242B"/>
        <text x="25" y="242" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#FFFFFF">WHY IT MATTERS:</text>
        <text x="25" y="264" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#A1A6AD">A compromised dock or panel</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#A1A6AD">cannot inject keystrokes into a</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#A1A6AD">terminal, password prompt, or</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#A1A6AD">cryptographic authentication modal.</text>
      </g>
"""
(OUT_DIR / "10-input-observation-arbitration.html").write_text(wrap_systems("gnosix · Input Observation & Arbitration Architecture", "Type: data-flow · Input Isolation", "Passive evdev tap without synthetic event injection authority.", d10))

# -------------------------------------------------------------
# 11-presentation-intent-pipeline
# -------------------------------------------------------------
d11 = header_badge("VOL II · 11/14", "Presentation Intent Pipeline", "gnos-ux-product-intent · Zero-Latency Optimism & Mutation Sync") + """
      <!-- User Input Trigger -->
      <g transform="translate(60, 110)">
        <rect width="210" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="210" height="36" rx="8" fill="#111418"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">USER INTERACTION</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Dock Click / Gesture</text>

        <rect x="15" y="85" width="180" height="110" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Trigger Event:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Click Workspace '4'</text>
        <text x="25" y="146" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Target output: DP-1</text>
        <text x="25" y="164" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Current active: '1'</text>
        <text x="25" y="185" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400">Emits ProductIntent</text>

        <rect x="15" y="210" width="180" height="145" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="232" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">DUAL DISPATCH:</text>
        <text x="25" y="252" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">gnos.ux splits the intent</text>
        <text x="25" y="270" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">into an instantaneous local</text>
        <text x="25" y="288" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">spring animation and an</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">asynchronous typed IPC</text>
        <text x="25" y="324" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">mutation request.</text>
      </g>

      <!-- Fork Lines -->
      <line x1="270" y1="180" x2="350" y2="180" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>
      <line x1="270" y1="310" x2="350" y2="310" stroke="#2446A8" stroke-width="2" marker-end="url(#arrow-blue)"/>

      <!-- Path A: Optimistic Local Presentation -->
      <g transform="translate(350, 110)">
        <rect width="360" height="150" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="360" height="32" rx="8" fill="#FFE5D2"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">PATH A · OPTIMISTIC LOCAL PRESENTATION (0ms)</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">GPUI Spring Animation Pipeline</text>
        <text x="18" y="75" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Immediately starts workspace slide curve (60/120 FPS fluid render)</text>
        <rect x="15" y="90" width="330" height="48" rx="4" fill="#F7F3ED"/>
        <text x="25" y="110" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="600" fill="#455F3D">✔ Perceived Latency: Absolute Zero (Instant UI response)</text>
        <text x="25" y="126" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Indicator pill snaps; envelope transition eases</text>
      </g>

      <!-- Path B: Authoritative Mutation -->
      <g transform="translate(350, 280)">
        <rect width="360" height="200" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="360" height="32" rx="8" fill="#E8EDFF"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">PATH B · AUTHORITATIVE MUTATION (~12ms)</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">in-gnu-os-activation Dispatch</text>

        <rect x="15" y="70" width="330" height="70" rx="4" fill="#F7F3ED"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Execution Flow:</text>
        <text x="25" y="106" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">1. Peer qualification -> 2. Freshness verify -> 3. Compositor commit</text>
        <text x="25" y="122" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#455F3D">4. Hyprland updates workspace mapping -> Emits event</text>

        <rect x="15" y="150" width="330" height="38" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="172" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#2446A8">gnosix-hyprland fetches fresh snapshot (Socket 1)</text>
      </g>

      <!-- Reconcile Line -->
      <line x1="710" y1="290" x2="790" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Reconcile / Convergence Box -->
      <g transform="translate(790, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#E8EFE5"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">RECONCILIATION</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">State Convergence</text>

        <rect x="15" y="85" width="220" height="130" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#455F3D">Normal Path (Match):</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Fresh compositor state arrives.</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Confirmed workspace == '4'.</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Local animation seamlessly</text>
        <text x="25" y="184" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">locks to authoritative state.</text>

        <rect x="15" y="225" width="220" height="135" rx="6" fill="#FFE5D2"/>
        <text x="25" y="248" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">Mismatch (Rollback):</text>
        <text x="25" y="268" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">If mutation was rejected,</text>
        <text x="25" y="286" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">gnos.ux smoothly eases the</text>
        <text x="25" y="304" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">spring animation back to '1'.</text>
        <text x="25" y="330" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#C95400">Zero Ghost State</text>
      </g>
"""
(OUT_DIR / "11-presentation-intent-pipeline.html").write_text(wrap_systems("gnosix · Dual-Path Presentation Intent Pipeline", "Type: flowchart · Latency & Authority", "Optimistic local spring animations paired with asynchronous typed mutation and authoritative reconciliation.", d11))

# -------------------------------------------------------------
# 12-settings-read-mutation-model
# -------------------------------------------------------------
d12 = header_badge("VOL II · 12/14", "Settings Store Architecture", "in-gnu-os-settings-core · Read Model vs Mutation Gate") + """
      <!-- Settings Schema -->
      <g transform="translate(60, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#E8EDFF"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">1. SETTINGS SCHEMAS</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">in-gnu-os-settings-core</text>

        <rect x="15" y="85" width="220" height="140" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Strict Schema Types:</text>
        <text x="25" y="128" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● key: "ui.accent_color"</text>
        <text x="25" y="146" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">  type: Enum(Orange, Blue, Green)</text>
        <text x="25" y="164" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● key: "workspaces.count"</text>
        <text x="25" y="182" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">  type: u32, range: 1..10</text>
        <text x="25" y="205" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8">Validated compile-time types</text>

        <rect x="15" y="240" width="220" height="115" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">MUTATION POLICY:</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Unknown keys are rejected.</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Out-of-range values fail closed.</text>
        <text x="25" y="318" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Zero arbitrary key/value bags.</text>
      </g>

      <!-- Split Arrow -->
      <line x1="310" y1="210" x2="390" y2="180" stroke="#455F3D" stroke-width="2" marker-end="url(#arrow-green)"/>
      <line x1="310" y1="310" x2="390" y2="340" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Read Model -->
      <g transform="translate(390, 110)">
        <rect width="320" height="165" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="320" height="32" rx="8" fill="#E8EFE5"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">REACTIVE READ MODEL</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">in-gnu-os-settings-read-model</text>
        <text x="18" y="75" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">Lock-free in-memory cache exposed over D-Bus Properties</text>

        <rect x="15" y="90" width="290" height="60" rx="4" fill="#F7F3ED"/>
        <text x="25" y="112" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#455F3D">Pub/Sub Broadcasting:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Emits org.freedesktop.DBus.Properties.PropertiesChanged</text>
      </g>

      <!-- Mutation Gate -->
      <g transform="translate(390, 290)">
        <rect width="320" height="190" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="320" height="32" rx="8" fill="#FFE5D2"/>
        <text x="18" y="22" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">MUTATION ADMISSION GATE</text>
        <text x="18" y="55" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Bounded Write Authority</text>

        <rect x="15" y="70" width="290" height="105" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Admission Checks:</text>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">1. Peer UID == session desktop user</text>
        <text x="25" y="124" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">2. Value validates against schema constraint</text>
        <text x="25" y="140" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">3. Write to durable disk file via atomic rename</text>
        <text x="25" y="156" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#455F3D">4. Notify Read Model of revision bump</text>
      </g>

      <!-- Right Arrow -->
      <line x1="710" y1="290" x2="790" y2="290" stroke="#FF6A00" stroke-width="2" marker-end="url(#arrow-orange)"/>

      <!-- Right Box: UI Client -->
      <g transform="translate(790, 110)">
        <rect width="250" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="250" height="36" rx="8" fill="#111418"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">CLIENT CONSUMER</text>
        <text x="18" y="62" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">gnos.ux Presentation</text>

        <rect x="15" y="85" width="220" height="140" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">Zero File Syscalls:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">UI components never call</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">open(), read(), or write()</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">on config files directly.</text>
        <text x="25" y="195" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#455F3D">✔ Pure D-Bus IPC Binding</text>

        <rect x="15" y="240" width="220" height="115" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="262" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">INVARIANT:</text>
        <text x="25" y="282" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">Presentation intent does</text>
        <text x="25" y="300" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">NOT imply persistence ownership.</text>
      </g>
"""
(OUT_DIR / "12-settings-read-mutation-model.html").write_text(wrap_systems("gnosix · Settings Store: Read Model vs Mutation Gate", "Type: data-flow · Configuration Architecture", "Typed schema contracts, reactive read-model projections, and isolated mutation admission.", d12))

# -------------------------------------------------------------
# 13-rear-plane-gnoshow-isolation
# -------------------------------------------------------------
d13 = header_badge("VOL II · 13/14", "gnoshow Rear-Plane Isolation", "Decoupled Wallpaper Substrate vs Presentation Planes") + """
      <!-- Left Box: gnoshow Substrate -->
      <g transform="translate(60, 110)">
        <rect width="360" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="360" height="36" rx="8" fill="#111418"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">gnoshow REAR-PLANE SUBSTRATE</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Wallpaper &amp; Canvas Renderer</text>

        <rect x="15" y="85" width="330" height="125" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Graphics Plane Architecture:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Wayland zwlr_layer_shell_v1 (Layer: Background)</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Standalone Vulkan / OpenGL shader pipeline</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Zero inter-process state sharing with UI scene</text>
        <text x="25" y="188" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="600" fill="#2446A8">Isolated process lifetime in app-gnoshow.service</text>

        <rect x="15" y="225" width="330" height="135" rx="6" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="248" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#C95400">WHAT gnoshow CANNOT SEE OR DO:</text>
        <text x="25" y="270" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">✗ Cannot observe windows, titles, or active workspaces</text>
        <text x="25" y="288" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">✗ Cannot intercept keyboard, pointer, or gesture input</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">✗ Cannot activate workspaces or mutate desktop profiles</text>
        <text x="25" y="335" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="700" fill="#5F7F52">Pure visual output sink</text>
      </g>

      <!-- Center Crash Isolation Wall -->
      <g transform="translate(450, 110)">
        <rect width="200" height="370" rx="8" fill="#FAF8F5" stroke="#FF6A00" stroke-width="2"/>
        <rect width="200" height="36" rx="8" fill="#FFE5D2"/>
        <text x="18" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">ISOLATION BARRIER</text>
        <text x="18" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Crash Containment</text>

        <rect x="15" y="90" width="170" height="110" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="112" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#C95400">FAULT INJECTION:</text>
        <text x="25" y="132" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">If gnoshow crashes</text>
        <text x="25" y="150" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">(SIGSEGV / GPU hang):</text>
        <text x="25" y="175" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#5F7F52">ZERO IMPACT ON DESKTOP</text>

        <rect x="15" y="215" width="170" height="145" rx="6" fill="#FFFFFF" stroke="#D7DADF"/>
        <text x="25" y="235" font-family="'Space Grotesk', sans-serif" font-size="10" font-weight="700" fill="#111418">DESKTOP SURVIVES:</text>
        <text x="25" y="255" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#455F3D">✔ Bar remains responsive</text>
        <text x="25" y="272" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#455F3D">✔ Dock remains clickable</text>
        <text x="25" y="289" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#455F3D">✔ Window surfaces intact</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#455F3D">✔ Input routing unaffected</text>
        <text x="25" y="335" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#667085">systemd auto-restarts plane</text>
      </g>

      <!-- Right Box: Desktop Core Plane -->
      <g transform="translate(680, 110)">
        <rect width="360" height="370" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="360" height="36" rx="8" fill="#E8EFE5"/>
        <text x="20" y="24" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">DESKTOP CORE PLANES</text>
        <text x="20" y="65" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#111418">Wayland Compositor + gnos.ux + in.gnu.os</text>

        <rect x="15" y="85" width="330" height="125" rx="6" fill="#F7F3ED" stroke="#D7DADF"/>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Independent Authority Domains:</text>
        <text x="25" y="130" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● Wayland Compositor (Hyprland): Layer Shell arbiter</text>
        <text x="25" y="148" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● gnos-ux-scene: GPUI surfaces on Overlay/Top layers</text>
        <text x="25" y="166" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#667085">● in-gnu-os-activation: Typed D-Bus mutation service</text>
        <text x="25" y="188" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="600" fill="#5F7F52">All isolated behind separate process boundaries</text>

        <rect x="15" y="225" width="330" height="135" rx="6" fill="#FFE5D2"/>
        <text x="25" y="248" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#C95400">ARCHITECTURAL RULE:</text>
        <text x="25" y="270" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">gnoshow remains a separate rear-plane domain.</text>
        <text x="25" y="288" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">Its rendering substrate is independent from</text>
        <text x="25" y="306" font-family="'Space Grotesk', sans-serif" font-size="9.5" fill="#111418">desktop semantic and activation authority.</text>
      </g>
"""
(OUT_DIR / "13-rear-plane-gnoshow-isolation.html").write_text(wrap_systems("gnosix · gnoshow Rear-Plane Substrate Isolation", "Type: architecture-delta · Rendering Isolation", "Decoupled wallpaper/substrate rendering independent from desktop semantics and shell state.", d13))

# -------------------------------------------------------------
# 14-fail-closed-invariants
# -------------------------------------------------------------
d14 = header_badge("VOL II · 14/14", "Architectural Invariants & Fail-Closed Guardrails", "Operational Verification of the 8 gnosix Invariants") + """
      <!-- Grid of 4 Failure Mode Checks -->
      <!-- Card 1: Stale Generation Mismatch -->
      <g transform="translate(60, 105)">
        <rect width="470" height="170" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="470" height="34" rx="8" fill="#FFE5D2"/>
        <text x="18" y="23" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#C95400">INVARIANT 1: STALE STATE FAIL-CLOSED</text>
        <text x="18" y="58" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Discrepancy: Request Generation &lt; Compositor Generation</text>

        <rect x="15" y="70" width="440" height="85" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Behavior: Halts Mutation · Drops Replay · Triggers Full Read</text>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Unsupported or stale state fails closed instead of synthesizing a plausible normal state.</text>
        <text x="25" y="125" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Prevents destructive workspace desynchronization during rapid switching.</text>
        <text x="25" y="143" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#C95400">Outcome: Error.StaleGeneration emitted; desktop state remains stable.</text>
      </g>

      <!-- Card 2: Unauthenticated D-Bus Peer -->
      <g transform="translate(570, 105)">
        <rect width="470" height="170" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="470" height="34" rx="8" fill="#111418"/>
        <text x="18" y="23" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FFFFFF">INVARIANT 2: PEER AUTHENTICATION BARRIER</text>
        <text x="18" y="58" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Discrepancy: Caller cgroup != app-gnos.ux.slice or UID Mismatch</text>

        <rect x="15" y="70" width="440" height="85" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Behavior: Drops Connection · Zero Shell Dispatch · Security Audit</text>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Names, package paths, and process presence do not grant authority.</text>
        <text x="25" y="125" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Arbitrary processes on the session bus cannot forge activation requests.</text>
        <text x="25" y="143" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#2446A8">Outcome: Immediate Error.AccessDenied rejection.</text>
      </g>

      <!-- Card 3: Ambiguous Hardware Identity -->
      <g transform="translate(60, 295)">
        <rect width="470" height="170" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="470" height="34" rx="8" fill="#E8EDFF"/>
        <text x="18" y="23" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#2446A8">INVARIANT 3: HARDWARE AMBIGUITY FAIL-CLOSED</text>
        <text x="18" y="58" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Discrepancy: Dual Monitors Share Identical EDID &amp; Port Ambiguity</text>

        <rect x="15" y="70" width="440" height="85" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Behavior: Blocks Profile Auto-Apply · Enforces Explicit User Prompt</text>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Durable identities are never inferred from transient compositor tokens.</text>
        <text x="25" y="125" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">System refuses to guess monitor roles, avoiding display layout churn.</text>
        <text x="25" y="143" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#455F3D">Outcome: Safe fallback to mirrored mode until explicit association.</text>
      </g>

      <!-- Card 4: Presentation Crash Containment -->
      <g transform="translate(570, 295)">
        <rect width="470" height="170" rx="8" fill="#FFFFFF" stroke="#D7DADF" stroke-width="1.5"/>
        <rect width="470" height="34" rx="8" fill="#E8EFE5"/>
        <text x="18" y="23" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#455F3D">INVARIANT 4: PROCESS DECOUPLING &amp; SURVIVABILITY</text>
        <text x="18" y="58" font-family="'Space Grotesk', sans-serif" font-size="10.5" font-weight="700" fill="#111418">Discrepancy: gnos-ux-scene Terminated by Fatal Signal</text>

        <rect x="15" y="70" width="440" height="85" rx="4" fill="#FAF8F5" stroke="#E6E8EB"/>
        <text x="25" y="90" font-family="'Space Grotesk', sans-serif" font-size="9.5" font-weight="700" fill="#111418">Behavior: User Apps Survive · UWSM Re-executes Scene · Clean State</text>
        <text x="25" y="108" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Applications run under systemd user manager scopes, not scene child processes.</text>
        <text x="25" y="125" font-family="'Space Grotesk', sans-serif" font-size="9" fill="#667085">Fresh scene reads authoritative compositor facts upon restart.</text>
        <text x="25" y="143" font-family="'Space Grotesk', sans-serif" font-size="9" font-weight="600" fill="#5F7F52">Outcome: Zero data loss for open editors, browsers, and terminals.</text>
      </g>

      <!-- Bottom Banner -->
      <g transform="translate(60, 480)">
        <rect width="980" height="45" rx="6" fill="#111418"/>
        <text x="25" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" font-weight="700" fill="#FF6A00">SYSTEMIC PRINCIPLE:</text>
        <text x="175" y="28" font-family="'Space Grotesk', sans-serif" font-size="11" fill="#FFFFFF">gnosix prioritizes architectural integrity and bounded safety over speculative convenience.</text>
      </g>
"""
(OUT_DIR / "14-fail-closed-invariants.html").write_text(wrap_systems("gnosix · Architectural Invariants & Fail-Closed Guardrails", "Type: decision-tree · System Safety Invariants", "Operational enforcement of the 8 gnosix architectural invariants under adverse runtime conditions.", d14))

print("All 14 Volume II systems diagrams generated successfully!")
