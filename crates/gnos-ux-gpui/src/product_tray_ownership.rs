//! Tray interaction identity and bus name.
//!
//! ## Why this file is this small
//!
//! The cross-process lease and the within-process host-claim rule were
//! originally implemented here and, deliberately duplicated, in
//! `product_mpris_ownership.rs`. EXTERNAL-RUNTIME-01 compared the two as
//! behaviour and extracted only what both had independently proven into
//! [`crate::interaction_lease`]. See
//! `docs/workstreams/external-runtime/01/invariant-matrix.md`.
//!
//! Two pieces of the old file did not move:
//!
//! * `TRAY_GATE_NAME` was declared here and used nowhere -- the tray gate
//!   arbitrates on the interaction lease it already holds. A named lease with no
//!   requester proves nothing, so it was removed rather than carried forward.
//! * `ProcessInteractionClaim` was declared here and used nowhere. Dropped.
//!
//! Tray interaction is deliberately a separate lease from Now Playing: a user
//! may legitimately click a tray icon while media plays, and one product
//! interaction holding the other would make two unrelated surfaces contend.

pub use crate::interaction_lease::{OwnershipClaim, output_owns_interaction};

/// Records a claim by one tray host within this process.
///
/// The state is per interaction rather than a shared process-global; see
/// [`crate::interaction_lease::HostClaim`] for why that distinction is a hard
/// requirement and not a style choice.
static HOST_CLAIM: crate::interaction_lease::HostClaim = crate::interaction_lease::HostClaim::new();

pub fn claim_host(output: Option<&str>) -> OwnershipClaim {
    HOST_CLAIM.claim(output)
}

/// Well-known bus name identifying the process that owns tray interaction.
///
/// Distinct from the Now Playing owner by design.
pub const TRAY_INTERACTION_OWNER_NAME: &str = "org.gnosix.TrayInteractionOwner";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_owner_name_is_a_valid_bus_name() {
        assert!(TRAY_INTERACTION_OWNER_NAME.starts_with("org."));
        assert!(!TRAY_INTERACTION_OWNER_NAME.contains('/'));
        for element in TRAY_INTERACTION_OWNER_NAME.split('.') {
            assert!(!element.is_empty(), "no empty bus-name element");
        }
    }

    #[test]
    fn the_two_product_interactions_have_distinct_leases() {
        assert_ne!(
            TRAY_INTERACTION_OWNER_NAME,
            crate::product_mpris_ownership::INTERACTION_OWNER_NAME,
            "tray and Now Playing must not contend for one lease"
        );
    }
}
