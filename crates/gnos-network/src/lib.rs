//! Typed NetworkManager read-side substrate for Gnosix.
//!
//! NETWORK-01 Phase 2 is read-only. There is no `NetworkIntent`, no mutation
//! transport, no profile writer, and no `InteractionLease` decision here. The
//! crate exists to observe NetworkManager and hand back typed facts whose
//! identity, availability and staleness are explicit.

mod model;
mod nm_mutation;
mod nm_observer;
mod reducer;
mod runner;
mod wifi;

pub use model::*;
pub use nm_mutation::*;
pub use nm_observer::*;
pub use reducer::*;
pub use runner::{NetworkObservation, run_observer};
pub use wifi::*;

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn object_reference_is_provider_scoped() {
        let before = NetworkObjectRef::new(":1.7", "/org/freedesktop/NetworkManager/Devices/3");
        let after = NetworkObjectRef::new(":1.42", "/org/freedesktop/NetworkManager/Devices/3");
        assert_ne!(
            before, after,
            "the same path under a new owner is a different object"
        );
        assert!(before.belongs_to(":1.7"));
        assert!(!before.belongs_to(":1.42"));
        assert!(after.belongs_to(":1.42"));
    }
}
