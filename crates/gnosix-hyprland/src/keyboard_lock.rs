//! Read-only projection of Hyprland primary-keyboard lock facts.
use crate::observation_transport::{ReadQuery, read_observation};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardLockSnapshot {
    pub caps_lock: bool,
    pub num_lock: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLockObservation {
    Observed(KeyboardLockSnapshot),
    Unavailable,
    Ambiguous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLockError {
    MissingHyprlandInstance,
    ObservationFailed,
    MalformedPayload,
    MissingPrimaryProperties,
}

/// Pure parser of hyprctl -j devices for Hyprland 0.56.
pub fn parse_primary_keyboard_locks(
    json: &[u8],
) -> Result<KeyboardLockObservation, KeyboardLockError> {
    let root: Value =
        serde_json::from_slice(json).map_err(|_| KeyboardLockError::MalformedPayload)?;
    let keyboards = root
        .get("keyboards")
        .and_then(Value::as_array)
        .ok_or(KeyboardLockError::MalformedPayload)?;
    let mut primaries = keyboards
        .iter()
        .filter(|keyboard| keyboard.get("main").and_then(Value::as_bool) == Some(true));
    let Some(primary) = primaries.next() else {
        return Ok(KeyboardLockObservation::Unavailable);
    };
    if primaries.next().is_some() {
        return Ok(KeyboardLockObservation::Ambiguous);
    }
    let caps_lock = primary
        .get("capsLock")
        .and_then(Value::as_bool)
        .ok_or(KeyboardLockError::MissingPrimaryProperties)?;
    let num_lock = primary
        .get("numLock")
        .and_then(Value::as_bool)
        .ok_or(KeyboardLockError::MissingPrimaryProperties)?;
    Ok(KeyboardLockObservation::Observed(KeyboardLockSnapshot {
        caps_lock,
        num_lock,
    }))
}

/// Bounded read-only observer over Hyprland's native request socket. The
/// caller owns sampling; no subprocess, background polling or mutation is
/// created by this module.
pub fn observe_primary_keyboard_locks() -> Result<KeyboardLockObservation, KeyboardLockError> {
    let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| KeyboardLockError::MissingHyprlandInstance)?;
    let payload = read_observation(&instance, ReadQuery::Devices)
        .map_err(|_| KeyboardLockError::ObservationFailed)?;
    parse_primary_keyboard_locks(&payload)
}

/// The same read against an explicit runtime directory and instance, for
/// isolated qualification against a private socket.
pub fn observe_primary_keyboard_locks_at(
    runtime: &std::path::Path,
    instance: &str,
) -> Result<KeyboardLockObservation, KeyboardLockError> {
    let payload =
        crate::observation_transport::read_observation_at(runtime, instance, ReadQuery::Devices)
            .map_err(|_| KeyboardLockError::ObservationFailed)?;
    parse_primary_keyboard_locks(&payload)
}

/// Sampling period of the lock observer.
///
/// Hyprland 0.56 emits no event when a lock toggles; the reference shell gets
/// one only by installing compositor keybinds, which would be a compositor
/// mutation this read-only observer does not own. A primary-keyboard read is
/// one fixed `j/devices` request on the local socket (measured on the
/// reference host: ~3.6 KB, ~21 us median), so 4 reads per second bound the
/// indicator's latency to 250 ms for a negligible cost. Only changes are
/// delivered.
pub const KEYBOARD_LOCK_SAMPLE_PERIOD: std::time::Duration = std::time::Duration::from_millis(250);

/// Fold one read into what the product may know. A failed or malformed read
/// is `Unavailable`: never "both locks off".
pub fn observation_from_read(
    read: Result<KeyboardLockObservation, KeyboardLockError>,
) -> KeyboardLockObservation {
    read.unwrap_or(KeyboardLockObservation::Unavailable)
}

/// Delivers the first observation and then only changes.
#[derive(Debug, Default)]
pub struct KeyboardLockReducer {
    last: Option<KeyboardLockObservation>,
}

impl KeyboardLockReducer {
    pub fn observe(
        &mut self,
        read: Result<KeyboardLockObservation, KeyboardLockError>,
    ) -> Option<KeyboardLockObservation> {
        let observation = observation_from_read(read);
        if self.last == Some(observation) {
            return None;
        }
        self.last = Some(observation);
        Some(observation)
    }
}

/// Sample the primary keyboard for the life of the calling thread.
pub fn run_keyboard_lock_observer<F>(mut on_change: F) -> !
where
    F: FnMut(KeyboardLockObservation),
{
    sample_keyboard_locks(
        observe_primary_keyboard_locks,
        KEYBOARD_LOCK_SAMPLE_PERIOD,
        |observation| {
            on_change(observation);
            std::ops::ControlFlow::Continue(())
        },
    );
    unreachable!("the production lock observer never breaks")
}

/// The sampling loop: read every `period`, deliver the first observation and
/// then only changes, until `on_change` breaks.
pub fn sample_keyboard_locks<R, F>(mut read: R, period: std::time::Duration, mut on_change: F)
where
    R: FnMut() -> Result<KeyboardLockObservation, KeyboardLockError>,
    F: FnMut(KeyboardLockObservation) -> std::ops::ControlFlow<()>,
{
    let mut reducer = KeyboardLockReducer::default();
    loop {
        if let Some(observation) = reducer.observe(read())
            && on_change(observation).is_break()
        {
            return;
        }
        std::thread::sleep(period);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_keyboard_only_not_the_first_or_all_devices() {
        let doc=br#"{"keyboards":[{"main":false,"capsLock":true,"numLock":true},{"main":true,"capsLock":false,"numLock":true}]}"#;
        assert_eq!(
            parse_primary_keyboard_locks(doc),
            Ok(KeyboardLockObservation::Observed(KeyboardLockSnapshot {
                caps_lock: false,
                num_lock: true
            }))
        );
    }
    #[test]
    fn no_primary_is_unavailable_not_unlocked() {
        assert_eq!(
            parse_primary_keyboard_locks(
                br#"{"keyboards":[{"main":false,"capsLock":false,"numLock":false}]}"#
            ),
            Ok(KeyboardLockObservation::Unavailable)
        );
    }
    #[test]
    fn multiple_primaries_are_ambiguous_not_combined() {
        assert_eq!(parse_primary_keyboard_locks(br#"{"keyboards":[{"main":true,"capsLock":true,"numLock":false},{"main":true,"capsLock":false,"numLock":true}]}"#), Ok(KeyboardLockObservation::Ambiguous));
    }
    #[test]
    fn missing_fields_or_bad_payload_fail_explicitly() {
        assert_eq!(
            parse_primary_keyboard_locks(br#"{"keyboards":[{"main":true,"numLock":false}]}"#),
            Err(KeyboardLockError::MissingPrimaryProperties)
        );
        assert_eq!(
            parse_primary_keyboard_locks(b"{}"),
            Err(KeyboardLockError::MalformedPayload)
        );
    }
    #[test]
    fn reducer_delivers_changes_and_never_turns_a_failed_read_into_unlocked() {
        let locked = KeyboardLockObservation::Observed(KeyboardLockSnapshot {
            caps_lock: true,
            num_lock: false,
        });
        let mut reducer = KeyboardLockReducer::default();
        assert_eq!(reducer.observe(Ok(locked)), Some(locked));
        assert_eq!(reducer.observe(Ok(locked)), None);
        assert_eq!(
            reducer.observe(Err(KeyboardLockError::ObservationFailed)),
            Some(KeyboardLockObservation::Unavailable)
        );
        assert_eq!(
            reducer.observe(Err(KeyboardLockError::MalformedPayload)),
            None
        );
        assert_eq!(reducer.observe(Ok(locked)), Some(locked));
        assert_eq!(
            reducer.observe(Ok(KeyboardLockObservation::Ambiguous)),
            Some(KeyboardLockObservation::Ambiguous)
        );
    }
}
