# Changelog

All notable changes to Gnosix are recorded here, one section per release.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The product is versioned `YY.MM` with a codename; the crates keep their own
semantic versions (see [the release policy](docs/reference/RELEASE-POLICY.md)).
The canonical product version is [`VERSION`](VERSION).

## [26.11 « Ardoise »] — unreleased

The first public release: a desktop shell on Hyprland, drawn natively in
Rust and GPUI, in daily use by its author. Experimental; see
[current status](docs/status/STATUS.md) for how far each part is proven.

### The Bar
- A persistent Bar on any edge of any output, travelling between edges
  along the frame; its modules ride and turn with it, and a new target
  mid-travel slows the Bar to a stop before it turns.
- Modules reorder by drag; edge, density, thickness, visibility and the
  order are durable settings.

### The Deck and the AppStack
- A dock of appcards (pins, then running applications) with magnification,
  pinning and reordering by drag, and a sheet of each application's actions.
- The AppStack: recent, A–Z and categories, a search with modes, launching
  through the launch authority.

### Panels from the Bar
- Utilities and the OSD: do not disturb, keep awake, microphone, Wi-Fi,
  Bluetooth, brightness (backlight and DDC/CI) and volume.
- Session: lock, suspend, log out, reboot, power off, with blockers shown.
- Dashboard: calendar, media controls and resources.
- Workspaces: each workspace with its windows and live thumbnails.

### The session
- Notifications: Gnosix serves `org.freedesktop.Notifications`, with rules
  per application and do not disturb.
- The lock screen, with PAM and idle lock.
- Wallpaper and desktop modules (clock, audio visualizer) on each output.

### Settings
- One Settings surface: Bar, Deck, surfaces, displays with keep-or-revert,
  default applications, notifications, language, wallpaper, advanced values.
- English and French throughout.

### Under the hood
- No redraw at rest; motion on 4 ms deadlines at the output's refresh rate.
- Tabler Icons, stroked natively.
- One text editor for every field, by grapheme, with input methods.
- The scene split into modules (a proven pure move).
