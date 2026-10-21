# gnosix — current status

The public claim boundary for Gnosix 26.11 « Ardoise », the first release.
It says what the shell does and how far each claim has been proven; it is not
a mirror of the engineering record.

**Development state:** Experimental. 26.11 is the first public release of a
desktop shell in daily use by its author; it is not offered as a finished
desktop for others yet.

## State vocabulary

| State | Meaning |
| --- | --- |
| **PROVEN** | Qualified in source and observed working in the author's live session, confirmed by the author. |
| **QUALIFIED** | Qualified in source and in an isolated nested Wayland session (real compositor, private services); deployed in the author's session; the author's confirmation is pending. |
| **SOURCE-QUALIFIED** | Contracts and invariants exercised in source; broader runtime behavior is not implied. |
| **EXPERIMENTAL** | Active work with a limited claim. |
| **OPEN** | A named obligation remains unresolved. |

## The shell

| Capability | State | Claim boundary |
| --- | --- | --- |
| The Bar | **QUALIFIED** | A persistent Bar on any edge of any output; it travels between edges along the frame, its modules riding with it and turning with it; modules reorder by drag; edge, density, thickness and visibility are durable settings. |
| The Deck and the AppStack | **QUALIFIED** | A dock of appcards (pins, running applications) with magnification; the AppStack in it searches the application catalogue (recent, A–Z, categories, search modes) and launches through the launch authority; pins are durable. |
| Utilities and OSD | **QUALIFIED** | Quick toggles (do not disturb, keep awake, microphone, Wi-Fi, Bluetooth), brightness (backlight and DDC/CI) and volume, with an on-screen display. |
| Session | **QUALIFIED** | Lock, suspend, log out, reboot and power off through logind, heavy actions held, blockers shown. |
| Dashboard | **QUALIFIED** | Calendar, media controls (MPRIS) and resources from the Bar's clock. |
| Workspaces | **QUALIFIED** | A workspace manager extruded from the Bar, with live window thumbnails (memory only, never while the session is locked, an application deny-list). |
| Notifications | **PROVEN** | Gnosix is the session's notification service (`org.freedesktop.Notifications`), with per-application rules and do not disturb. |
| Lock screen | **QUALIFIED** | Session lock with PAM, idle lock and its settings. |
| Settings | **QUALIFIED** | One Settings surface: Bar, Deck, surfaces, displays (resolution, rate, scale, rotation, position, with keep-or-revert), default applications, notifications, language, wallpaper, advanced values. |
| Desktop | **QUALIFIED** | The wallpaper and desktop modules (clock, audio visualizer) drawn by Gnosix on each output. |
| Languages | **PROVEN** | English and French throughout the shell and the lock screen, chosen in Settings, with a coverage gate. |
| Icons | **PROVEN** | Tabler Icons, stroked natively. |
| Fluidity | **PROVEN** | No redraw at rest; motion on 4 ms deadlines, measured at the output's refresh rate (75 and 165 Hz). |

## The system underneath

| Capability | State | Claim boundary |
| --- | --- | --- |
| Current-session multi-output routing | **PROVEN** | Output-local surfaces and topology add/remove/reattach for the current session; not physical-hardware identity. |
| Durable logical output identity | **SOURCE-QUALIFIED** | Persistent logical identities with explicit association to session outputs. |
| Profile store | **PROVEN** | The durable profile behind every setting, written through its authenticated authority. Default personal-profile provisioning is not claimed. |
| Typed workspace activation | **PROVEN** | Exact workspace activation through the authenticated scene caller. |
| User-session lifecycle | **PROVEN** | UWSM/greetd session start and selected lazy-start activation. |
| Accessibility mechanics | **SOURCE-QUALIFIED** | Keyboard traversal, focus and bounded AT-SPI naming on implemented surfaces; end-user screen-reader usability is not claimed. |
| `gnoshow` rendering substrate | **EXPERIMENTAL** | A rear-plane renderer; production integration is separate work. |

## Open obligations

- **Whole-product acceptance** (R19) is open: 26.11 is a public experimental release, not a stable desktop for others.
- **The author's review** of the QUALIFIED surfaces above (MEGA-REVIEW-02); each moves to PROVEN when confirmed.
- **End-user screen-reader usability** has not had its human review.
- **Keyboard on the Bar** (focus, arrows, Enter) is designed, not built.
- **Mods**: third-party modules as confined processes are designed, not built; a threat model comes first.

## License

MIT for first-party code and documentation; bundled third-party material keeps
its own terms ([THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)).

## Repository qualification

The canonical check is `cargo xtask ci`; GitHub CI runs the same contract.
Host and live checks live under `cargo xtask verify` and are not part of it.
Every release is cut from the private engineering tree and proven alone in the
runner's environment before it is published.

See [evidence policy](../evidence/README.md), [architecture](../../ARCHITECTURE.md),
and [authority boundaries](../../AUTHORITY.md).
