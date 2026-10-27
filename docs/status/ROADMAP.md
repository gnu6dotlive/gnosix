# gnosix roadmap

The roadmap is gate-driven: an item moves when its evidence is complete and,
where required, the author has accepted it. Releases ship twice a year (May and
November) with what is ready.

## After 26.11 « Ardoise »

- **The author's review** of every surface 26.11 carries as QUALIFIED, each
  confirmed or reopened.
- **Module cards**: every Bar module gains a card state, the Deck's appcard,
  so a module never loses material at a corner and can be carried and placed.
- **The scene in layers**: one layer builder per surface, ordered by
  attention — the clean slot a hosted module needs.
- **A threat model** before mods: the authorities, D-Bus caller
  authentication, the keyboard, notifications on the lock screen, thumbnail
  capture.
- **Keyboard on the Bar**: focus, arrows between modules, Enter to open,
  moving a module from the keyboard.
- **One shape for the shell**: one signed distance field for the frame, the
  Bar, every extruded surface and the Deck.
- **Mods**: third-party modules as confined processes that describe their
  interface, drawn by the shell in its own material, with declared needs and
  per-permission grants.

## Release acceptance

A stable release for others is not claimed until whole-product acceptance has
an explicit disposition, including the end-user accessibility review. Public
experimental releases ship before that gate closes and say so.

## Non-goals

No generic shell-command executor, no monolithic settings daemon, no durable
identity inferred from compositor geometry or tokens, and no silent adoption of
a research donor's authority.
