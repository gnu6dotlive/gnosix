# in.gnu.os desktop profiles

This crate contains the R12-B1 profile schema/pure policy core and the R12-B2
identity-checked durable store for the `in.gnu.os` authority. It is not a
service, output observer, activation broker, or runtime projection. GNOS.UX
and Gnoshow do not write profiles.

The durable store lineage remains named `desktop-profiles-v1.json` and stays
separate from the R11 `output-identities-v1.json` registry. The document carries
its own logical `schema_version`: DONOR-11 advances that schema to `2` while
keeping the existing CAS/rotation lineage. Schema-1 documents are decoded
strictly into an in-memory schema-2 compatibility view and are upgraded only on
the next authorised commit; post-v1 fields hidden under `schema_version: 1` are
refused. Older readers therefore fail closed on schema 2 rather than silently
misreading the new preferences. The document associates a durable
`DurableOutputHubId` with a `DesktopProfileId`; it never replaces a session
`OutputHubId`.

A profile models visual (color scheme), motion (reduced motion), accessibility
(text scale), and Bar placement intent. `bar.edge` is an optional typed value
whose stable serialized names are `top`, `right`, `bottom`, and `left`. Older
profiles without this field remain valid; an absent edge resolves to the
product default `Top` in the effective read model. A present malformed edge is
rejected during deserialization and follows the normal explicit recovery path.
These are preferences, not compositor or desktop-portal observations. Text
scale remains a positive basis-point value (10,000 is 1.0x); renderer
consumption and system large-text discovery are not claimed here.

`DesktopProfileDocument::validate` rejects unsupported schema versions,
duplicate IDs/selections, empty names, and selections to missing profiles.
This pure B1 check is independent of R11. `resolve_output` uses only durable
output identity. An output with no selected profile returns `NoSelection`, so
callers preserve their current defaults. Unknown fields are rejected rather
than silently discarded.

## R12-B2 durable store

`DesktopProfileStore` reads exactly one R12-B1 document from an
authority-selected profile directory and is explicitly bound at construction
to the R11 identity-store location selected by its owner. The constructor
chooses neither production path. The opaque `DesktopProfileWriter` is not
constructible by downstream consumers. DONOR-10 adds the feature-gated
`BarProfileWriter` host seam, which can change only the edge of one already
selected profile and never exposes the raw writer. The legacy desired-state
issuer is test-only. Process authentication lives in the separate
`in-gnu-os-profile-service` adapter; this crate alone makes no runtime
authentication claim.

The writer accepts a complete validated candidate and an opaque `ProfileVersion`
CAS token. A current token pairs the document revision with SHA-256 of its
exact persisted bytes; an absent store has a distinct token. The candidate
revision must be exactly one greater than the expected revision. Stale writers
receive a conflict even when recovery installs different bytes at the same
numeric revision. The store serializes cooperating writers with a lock,
writes and syncs a unique pending file in the same directory, rotates the last
known-good document, atomically renames the pending generation to current, and
syncs the directory. A failure before installation leaves the previous
generation recoverable; pending files are never promoted automatically. This
store persists only the B1 profile document and its recovery mechanics; it
does not persist session, scene, focus, runtime, health, or identity-registry
state.

Reads distinguish `Absent`, `Current`, `Recoverable`, and `Unrecoverable`.
Corrupt or missing current content with a valid previous document yields a
recovery candidate and an opaque token containing the exact observed current
bytes. Recovery is a named writer operation that requires that token, a
non-empty audit reference, and a currently valid generation from the R11 store
bound to this profile store, containing each Hub selected by the previous
profile document. It returns a
`ProfileRecoveryReceipt` with the restored and source revisions, damage
classification, identity revision, and caller reference. The authority must
retain that receipt as recovery evidence. `load` never substitutes a previous
generation, and future schemas are not downgraded.

## R11 cross-validation and race contract

Before a profile commit or recovery, every selection is checked against a
locked current R11 snapshot from the store bound at construction; callers
cannot supply a snapshot from another registry. The identity snapshot lock
remains held through the profile CAS, blocking cooperating R11 commit/recovery
for the full check/use interval. Lock order is R11 then profile. The profile
CAS itself is performed under the profile store lock. R11 keeps an issued-ID
ledger across recovery and rejects a Hub ID
that disappeared from the current generation from being allocated again.
Together these rules prevent an R11 rollback from silently making an old
profile selection point at another logical output.

## R12-B3 read-only profile projection

`DesktopProfileStore::project_output` takes only a `DurableOutputHubId` and
requires that ID to exist in the store's bound current R11 registry. It holds
the R11 lock before reading the profile document, validates every document
selection against that same identity generation, and returns both revisions
with the selected profile. The caller must obtain the durable ID through the
R11 binding/observation path; this method performs no physical-output
discovery or fallback matching.

The result distinguishes no profile document, an output with no selection, a
selected profile, recovery required, and unavailable/corrupt profile state.
Previous candidates are never projected automatically. The selected value is
the stored preference, including `FollowSystem`; it does not merge portal or
compositor observations, apply settings, or mutate GNOS.UX/Gnoshow. No session
ID, connector, runtime, focus, scene, or health state is introduced. The
projection is a point-in-time value carrying profile and R11 revisions; a
consumer must revalidate before later use if either authority can change.

If R11 later explicitly recovers to a generation that no longer contains a
profile's selected Hub, `load_for_identity` rejects that association. Profile
bytes remain intact for explicit review and are never mapped to another
output. The R11 lock is released after each operation; consumers must
revalidate against current identity before use.

`load()` is raw storage inspection. A consumer that intends to use
`output_profile_selections` must call `load_for_identity`, which obtains and
holds a point-in-time locked snapshot from the bound R11 store while checking
the document. Consumers must revalidate before later commits or use after the
snapshot is released. `project_output` is the convenience path for one
already-known durable Hub and performs selection while the snapshot remains
locked.

R13 activation, R14 systemd user hosting, R15 external control, and R16
Gnoshow remain distinct program tracks. Their capabilities are not combined
with the R11 identity writer or this profile store. R17 Rail and Overlay remain
probe-only under the ratified exclusion.

## DONOR-10 bounded production host seam

The adapter uses the existing CAS commit/recovery and bound R11 validation;
it does not bootstrap stores or create/select profiles. Its explicit resource
is one profile supplied by deployment configuration. A request echoes the host
epoch, profile ID and exact revision/digest, then sets one typed `BarEdgePreference`.
Exact request replays within the host epoch return the retained terminal receipt;
changed payloads and prior epochs fail closed. A bounded replay ledger never
evicts completed requests to admit new ones. Host restart requires a fresh read
and deliberate new intent; it never silently resends an uncertain write.

The source/staging boundary and live proof still required are documented in
[DONOR-10](../../evidence/research/gnosix-caelestia-donor-10/README.md).
