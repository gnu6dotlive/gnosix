# `in-gnu-os-output-identity`

This crate contains the R11 identity domain/binding-policy core and an
identity-only filesystem store for the future `in.gnu.os` authority. It is not
a daemon, output observer, or caller-authorization mechanism. The authority
owner supplies logical IDs, selects/protects the store directory, authorizes
mutations, and retains audit policy. GNOS.UX consumes resolved identity
read-only; the pure `gnosix-desktop-model` does not depend on this crate or
perform matching.
The durable ID and session `OutputHubId` remain distinct types. The separate
`in-gnu-os-output-identity-projection` crate provides a read-only sidecar
association for a current session projection. It does not alter the pure
`gnosix-desktop-model` projection or existing session semantics when authority
is absent or binding is not `Bound`.

## Version 1 registry

`RegistryDocument` is a JSON-serializable document with an explicit
`schema_version`, monotonic `revision`, durable logical OutputHub IDs, binding
records, and an append-only sequence of authority-referenced audit events.
Schema version 1 rejects unknown fields and unsupported versions. It has no
implicit migration. Corrupt, future-version, or structurally invalid documents
must fail closed; a storage adapter must preserve the prior valid document and
surface recovery rather than silently resetting identities.

The schema records opaque 256-bit EDID/serial fingerprints, optional make/model
and physical-size corroboration, connector history, an authority approval
reference, and revision. The adapter is responsible for descriptor acquisition
and fingerprint construction. Raw connector names and descriptors are not
treated as unique identity. Connector history can identify a possible
replacement/conflict but can never produce `Bound` by itself. Geometry, current
workspace, application state, session IDs and current `OutputHubId` values are
not inputs to durable matching.

## Binding policy

Resolution is read-only and returns exactly one of:

- `Bound`: one logical OutputHub matches stable evidence with no contradictory
  descriptor evidence;
- `Ambiguous`: stable evidence claims more than one logical OutputHub;
- `Unbound`: no known match, or no stable fingerprint was observed;
- `DescriptorConflict`: a connector points to old history without stable proof,
  stable descriptors contradict a binding, or previously bound evidence was
  explicitly retired.

No “best candidate” is selected. An explicit bind requires stable fingerprint
evidence and a non-empty approval reference. An explicit rebind names both the
old and new logical OutputHub, uses an optimistic expected revision, retires the
old active binding, records an audit event, and binds the new observation. An
ambiguity can be resolved only by an explicit selected candidate and the exact
candidate set/revision reviewed by the caller. These references record intent;
they are not credentials. The authority adapter must enforce who may submit
those commands.

## R11-D identity-only filesystem store

`FilesystemOutputIdentityStore` implements `OutputIdentityStore` for exactly
one `RegistryDocument`. The `in.gnu.os` owner supplies a private directory; the
crate does not choose a product path or mix profile/configuration data into the
registry. `load()` reports `Absent`, `Current`, `Recoverable`, or
`Unrecoverable`. A previous generation is only a recovery candidate. It is
never silently promoted during load.

`commit(expected_revision, document)` validates the complete schema and audit
history, serializes deterministic JSON, writes a unique pending file in the
same directory, syncs that file, rotates the current generation to one
previous known-good generation, syncs the directory, atomically renames the
pending file to current, then syncs the directory again. An advisory lock
serializes cooperating store writers; it is not caller authentication or
authorization. Files are created with user-only permissions on Unix. The
authority must supply a private local filesystem directory and must not assume
network-filesystem durability semantics from this implementation.

An initialized marker is durably written before the first pending generation.
If both current and previous files later disappear, that marker makes the
condition `Unrecoverable`, not `Absent`. Thus a crash during first initialization
or loss of all known-good generations cannot silently look like a fresh empty
registry.

If current is corrupt or missing while previous is valid, `load()` returns
`Recoverable` with the damage classification and pending-candidate names. A
caller must explicitly invoke `restore_previous` with a non-empty recovery
reference. Recovery appends a `RecoveryRestored` audit event, preserves damaged
current bytes as `output-identities-v1.damaged` where present, and leaves the
previous generation available. If that damaged filename already exists, the
store refuses to overwrite it. Future schema versions are `Unrecoverable` even
when a previous v1 document exists; writes and recovery are refused. Orphaned
pending files are never treated as either a current generation or an empty
store.

The store also maintains a private `output-identities-v1.issued` ledger. New
IDs are durably reserved in this ledger before a registry generation can
publish them. Normal commits cannot remove hubs, and an ID found in the ledger
but absent from current state cannot be registered again. Explicit rollback
recovery leaves the ledger intact, so a hub omitted by the recovered previous
generation remains permanently unavailable for reallocation. If the ledger is
corrupt or absent beside an already initialized registry, mutations and
cross-document snapshots fail closed; no ledger migration is performed
silently. A reservation can survive a crash whose candidate generation did
not become current; the authority must allocate a different ID rather than
reuse it.

`locked_current_snapshot()` returns the current revision and membership view
while retaining the identity-store lock. A consumer can validate durable Hub
references and commit its own document before dropping the snapshot; identity
commit/recovery is blocked during that interval. The snapshot never publishes
session projection state. Consumers must still revalidate after the guard is
dropped before using an association.

The store does not authenticate a caller, run as a service, supervise a
process, accept GNOS.UX writes, or alter the current session projection. R11-E
adds only the crate/API mutation-capability restriction described below;
runtime caller authorization remains unimplemented. R11-F is implemented by
the separate read-only projection adapter and is source-qualified with
authority-absent/non-Bound parity. GNOS.UX continues using its existing
session-scoped projection unchanged until a separately authorized runtime
integration exists.

## R11-E reader and mutation capability

`OutputIdentityReader` is the downstream read seam: it can load the document,
inspect a revision, and resolve an observation. The public
`OutputIdentityStore` trait exposes only `load`. Registry mutation primitives
and the store commit/recovery trait are crate-private. The opaque
`IdentityAuthorityWriter` has private fields and no public constructor; its
issuer is private to the authority module. A caller that is handed this value
can invoke its mutation methods, so distribution of the capability remains
the responsibility of the authority adapter.

This is a Rust/API access restriction for ordinary downstream consumers. It is
not process authentication, caller authorization at an IPC boundary, or a
claim that another process cannot modify the file. The advisory lock only
serializes cooperating writers. `ExplicitBindingApproval::operator(...)` and
recovery references remain audit metadata, never credentials. The future
`in.gnu.os` runtime must enforce caller identity/authorization before issuing
or using mutation capability. No such runtime boundary is implemented here.

## Example

The test `a_known_output_remains_bound_after_registry_cold_start` exercises the
public flow: register a caller-allocated logical ID, explicitly bind stable
evidence, serialize the schema, reload it, and resolve the same output. Other
tests cover unknown outputs, duplicate evidence, connector changes/replug,
descriptor conflicts, explicit ambiguity choice, stale revisions, explicit
replacement, and future schema rejection.
