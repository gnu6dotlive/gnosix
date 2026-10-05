# gnosix roadmap

The roadmap is gate-driven rather than date-driven. Items move when their bounded evidence is complete and, where required, explicitly accepted by the operator.

## Public-source readiness

- Normalize the public documentation surface and canonical naming.
- Curate public evidence instead of publishing the raw engineering corpus as the product interface.
- Ratify and publish the repository-wide license.
- Verify a fresh clone against the same `cargo xtask ci` contract used in GitHub CI.
- Complete public visual assets/social preview from the current gnosix design system.

## Product integration

- Finish default personal-profile provisioning/selection without widening the existing profile authority.
- Continue provider/status projections with truthful unavailable/stale semantics and separate read/observe from mutation authority.
- Keep global keybinding/input work behind its typed arbitration and producer qualification gates.
- Integrate `gnoshow` only through its independent rendering/control boundary.

## Release acceptance

Repository visibility does not close the release gate. A stable end-user release is not claimed until the current whole-product acceptance obligation receives an explicit disposition, including the required end-user accessibility review.

Public source, experimental builds, and contributor inspection may exist before that stable-release gate closes; they must remain labelled accordingly.

## Non-goals

The roadmap does not authorize a generic shell-command executor, a monolithic settings daemon, inferred durable identity from compositor geometry/tokens, or silent adoption of research-donor authority.
