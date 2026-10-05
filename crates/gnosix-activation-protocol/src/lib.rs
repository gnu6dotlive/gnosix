#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Passive, authority-free wire constants shared by GNOS.UX and in.gnu.os.

pub const ACTIVATION_BUS_NAME: &str = "in.gnu.os.Activation1";
pub const ACTIVATION_OBJECT_PATH: &str = "/in/gnu/os/Activation1";
pub const ACTIVATION_INTERFACE: &str = "in.gnu.os.Activation1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ActivationReceiptCode {
    Applied = 1,
    AlreadySatisfied = 2,
    Stale = 3,
    UnknownTarget = 4,
    NotAuthorized = 5,
    TargetDisappeared = 6,
    SessionUnavailable = 7,
    BackendRejected = 8,
    ObservationFailure = 9,
    ProtocolFailure = 10,
    TransportFailure = 11,
    PostconditionMismatch = 12,
    ExecutionBindingChanged = 13,
}

impl TryFrom<u32> for ActivationReceiptCode {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Applied,
            2 => Self::AlreadySatisfied,
            3 => Self::Stale,
            4 => Self::UnknownTarget,
            5 => Self::NotAuthorized,
            6 => Self::TargetDisappeared,
            7 => Self::SessionUnavailable,
            8 => Self::BackendRejected,
            9 => Self::ObservationFailure,
            10 => Self::ProtocolFailure,
            11 => Self::TransportFailure,
            12 => Self::PostconditionMismatch,
            13 => Self::ExecutionBindingChanged,
            _ => return Err(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_identity_is_in_gnu_os_and_versioned_from_v1() {
        assert_eq!(ACTIVATION_BUS_NAME, "in.gnu.os.Activation1");
        assert_eq!(ACTIVATION_INTERFACE, "in.gnu.os.Activation1");
        assert_eq!(ACTIVATION_OBJECT_PATH, "/in/gnu/os/Activation1");
        assert!(!ACTIVATION_BUS_NAME.contains("gnu6"));
    }

    #[test]
    fn every_defined_receipt_code_round_trips() {
        for value in 1..=13 {
            let code = ActivationReceiptCode::try_from(value).unwrap();
            assert_eq!(code as u32, value);
        }
        assert!(ActivationReceiptCode::try_from(0).is_err());
        assert!(ActivationReceiptCode::try_from(14).is_err());
    }
}
