#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MotionPreference {
    #[default]
    NoPreference,
    Reduce,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MotionPreferenceState {
    cli_override: bool,
    system: MotionPreference,
}

impl MotionPreference {
    pub(super) fn from_portal_setting(version: Option<u32>, value: Option<u32>) -> Self {
        if !version.is_some_and(|version| version >= 2) {
            return Self::NoPreference;
        }
        match value {
            Some(1) => Self::Reduce,
            _ => Self::NoPreference,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NoPreference => "no_preference",
            Self::Reduce => "reduce",
        }
    }

    pub(super) const fn as_system_observation(self) -> Option<bool> {
        match self {
            Self::NoPreference => None,
            Self::Reduce => Some(true),
        }
    }
}

impl MotionPreferenceState {
    pub(super) fn new(cli_override: bool) -> Self {
        Self {
            cli_override,
            system: MotionPreference::NoPreference,
        }
    }

    pub(super) fn effective_reduced_motion(self) -> bool {
        self.cli_override || self.system == MotionPreference::Reduce
    }

    pub(super) fn update_system(&mut self, preference: MotionPreference) -> bool {
        let was_reduced = self.effective_reduced_motion();
        self.system = preference;
        was_reduced != self.effective_reduced_motion()
    }

    pub(super) fn cli_override_enabled(self) -> bool {
        self.cli_override
    }
}

#[cfg(test)]
mod tests {
    use super::{MotionPreference, MotionPreferenceState};
    use crate::product_scene::reactive_motion_needs_frame;

    #[test]
    fn portal_values_follow_reduced_motion_setting_contract() {
        assert_eq!(
            MotionPreference::from_portal_setting(None, Some(1)),
            MotionPreference::NoPreference
        );
        assert_eq!(
            MotionPreference::from_portal_setting(Some(1), Some(1)),
            MotionPreference::NoPreference
        );
        assert_eq!(
            MotionPreference::from_portal_setting(Some(2), None),
            MotionPreference::NoPreference
        );
        assert_eq!(
            MotionPreference::from_portal_setting(Some(2), Some(0)),
            MotionPreference::NoPreference
        );
        assert_eq!(
            MotionPreference::from_portal_setting(Some(2), Some(1)),
            MotionPreference::Reduce
        );
        assert_eq!(
            MotionPreference::from_portal_setting(Some(2), Some(7)),
            MotionPreference::NoPreference
        );
    }

    #[test]
    fn command_line_override_wins_over_live_system_changes() {
        let mut state = MotionPreferenceState::new(true);
        assert!(state.effective_reduced_motion());
        state.update_system(MotionPreference::NoPreference);
        assert!(state.effective_reduced_motion());
        state.update_system(MotionPreference::Reduce);
        assert!(state.effective_reduced_motion());
    }

    #[test]
    fn live_system_preference_changes_effective_state_only_when_needed() {
        let mut state = MotionPreferenceState::new(false);
        assert!(!state.effective_reduced_motion());
        assert!(state.update_system(MotionPreference::Reduce));
        assert!(state.effective_reduced_motion());
        assert!(!state.update_system(MotionPreference::Reduce));
        assert!(state.update_system(MotionPreference::NoPreference));
        assert!(!state.effective_reduced_motion());
    }

    #[test]
    fn system_reduced_motion_resolves_endpoint_without_arming_motion_frames() {
        let mut state = MotionPreferenceState::new(false);
        assert!(state.update_system(MotionPreference::Reduce));
        assert!(state.effective_reduced_motion());
        assert!(!reactive_motion_needs_frame(
            None,
            true,
            state.effective_reduced_motion()
        ));
    }
}
