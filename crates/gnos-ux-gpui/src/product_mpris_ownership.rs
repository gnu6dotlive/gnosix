//! Now Playing interaction identity and bus names.
//!
//! ## Why this file is this small
//!
//! The cross-process lease and the within-process host-claim rule were
//! originally implemented here and, deliberately duplicated, in
//! `product_tray_ownership.rs`. EXTERNAL-RUNTIME-01 compared the two as
//! behaviour and extracted only what both had independently proven into
//! [`crate::interaction_lease`]. See
//! `docs/workstreams/external-runtime/01/invariant-matrix.md`.
//!
//! What remains is Now Playing specific: its bus names, and the fact that Now
//! Playing interaction is a lease of its own. The old `ProcessInteractionClaim`
//! enum was dropped rather than carried across: it was declared in both
//! verticals and used in neither, so it was vocabulary that had never been
//! exercised, not behaviour that had survived.
//!
//! Re-exporting the shared rule keeps every call site reading
//! `product_mpris_ownership::…`, so the vertical stays the place a reader looks
//! for Now Playing ownership.

pub use crate::interaction_lease::{OwnershipClaim, output_owns_interaction};

/// Records a claim by one Now Playing host within this process.
///
/// The state is per interaction rather than a shared process-global; see
/// [`crate::interaction_lease::HostClaim`] for why that distinction is a hard
/// requirement and not a style choice.
static HOST_CLAIM: crate::interaction_lease::HostClaim = crate::interaction_lease::HostClaim::new();

pub fn claim_host(output: Option<&str>) -> OwnershipClaim {
    HOST_CLAIM.claim(output)
}

/// Well-known bus name that identifies the process owning Now Playing
/// interaction.
///
/// A bus name is the only arbitration primitive available across processes, and
/// the session bus already mediates every other provider in this vertical. The
/// name is per output, so its owner is unambiguous and its loss on exit is the
/// signal that ownership ended.
pub const INTERACTION_OWNER_NAME: &str = "org.gnosix.NowPlayingInteractionOwner";

/// Bus name identifying the process currently running an MPRIS qualification
/// gate.
///
/// The interaction owner decides who may install a dispatch sink; the gate
/// runner decides which process may drive scripted gate dispatches. Both must
/// agree: a gate runner that does not own the interaction would leave the
/// journal with calls from a process whose surface never claimed ownership.
pub const QUALIFICATION_GATE_NAME: &str = "org.gnosix.NowPlayingGateRunner";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_now_playing_names_are_distinct_and_valid() {
        // The arbitration primitive is a bus name, so each must be one the
        // daemon will actually accept, and the interaction lease and the gate
        // lease must not collide.
        assert_ne!(INTERACTION_OWNER_NAME, QUALIFICATION_GATE_NAME);
        for name in [INTERACTION_OWNER_NAME, QUALIFICATION_GATE_NAME] {
            assert!(name.starts_with("org."));
            assert!(!name.contains('/'));
            for element in name.split('.') {
                assert!(!element.is_empty(), "no empty bus-name element in {name}");
            }
        }
    }
}
