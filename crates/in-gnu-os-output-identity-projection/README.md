# `in-gnu-os-output-identity-projection`

This crate is a read-only `in.gnu.os` adapter between R11 durable bindings and
the existing current-session `DesktopOutputProjection`. It depends on both
contracts; `gnosix-desktop-model` remains unchanged and has no dependency on
either identity crate.

The consumer passes the existing projection, a per-output observation paired
with its session `OutputHubId`, and optionally an `OutputIdentityReader`. The
adapter returns a separate `DurableSessionOutputAssociation` only when the
reader resolves that observation as `Bound`. The sidecar retains both distinct
ID types, the registry revision, and matched stable evidence. It does not
modify or replace the session projection or identifier.

No reader (authority absent), an absent/uninitialized store, `Unbound`,
`Ambiguous`, `DescriptorConflict`, recoverable or unrecoverable storage state
produce no durable association. A reader error is returned as an error and
cannot produce an association. An observation paired with a different
session projection is rejected.

This is an in-process read-only mapping seam. It does not create a runtime,
service, endpoint, observer, writer capability, caller authentication, or
process-authorization claim. The observer must ensure that descriptor evidence
was collected from the session output named by the pairing.
