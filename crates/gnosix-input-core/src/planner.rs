//! Dry-run planner has no apply operation or host-write effects.
use crate::{
    model::{
        Binding, BindingContext, BindingResolution, Gesture, Modifier, ProviderRef, SemanticAction,
    },
    resolution::{
        BrowserRoleResolution, Conflict, ObservedGestureCandidate,
        ObservedGestureConflictCandidate, conflicts, observed_gesture_conflicts,
    },
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RoleReadiness {
    NotRelevant,
    Consistent {
        #[serde(with = "crate::model::desktop_entry_id")]
        desktop_entry: in_gnu_os_settings_core::DesktopEntryId,
    },
    Conflict,
    Unavailable,
    Invalid,
    ProviderUnavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedBinding {
    pub binding: Binding,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredAuthority {
    NoneForDryRun,
    FutureSettingsAuthority,
    FutureCompositorConfigurationAuthority,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredCapability {
    SuperTapArbitration,
    CompositorBindingProjection,
    DefaultWebBrowserRoleResolution,
    ProviderOperationProof,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReversibleOperation {
    PreservePriorBindingValue,
    WriteProposedBinding,
    ValidateEffectiveRegistration,
    RestorePriorBindingValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DryRunPlan {
    pub evidence_class: &'static str,
    pub current: Vec<Binding>,
    pub proposed: Binding,
    pub conflicts: Vec<Conflict>,
    pub observed_conflict_candidates: Vec<ObservedGestureConflictCandidate>,
    pub affected_sources: Vec<String>,
    pub required_authority: Vec<RequiredAuthority>,
    pub required_capabilities: Vec<RequiredCapability>,
    pub reversible_operations_that_would_be_needed: Vec<ReversibleOperation>,
    pub would_apply: bool,
    pub resolution: BindingResolution,
    pub valid: bool,
}
pub fn role_readiness(
    role: SemanticAction,
    evidence: Option<&BrowserRoleResolution>,
) -> RoleReadiness {
    if role != SemanticAction::OpenDefaultWebBrowser {
        return RoleReadiness::NotRelevant;
    }
    match evidence {
        Some(BrowserRoleResolution::Consistent { id }) => {
            in_gnu_os_settings_core::DesktopEntryId::parse(id.clone())
                .map(|desktop_entry| RoleReadiness::Consistent { desktop_entry })
                .unwrap_or(RoleReadiness::Invalid)
        }
        Some(BrowserRoleResolution::Conflict { .. }) => RoleReadiness::Conflict,
        Some(BrowserRoleResolution::Invalid { .. }) => RoleReadiness::Invalid,
        _ => RoleReadiness::Unavailable,
    }
}
pub fn plan(
    current: Vec<Binding>,
    proposed: Binding,
    source_evidence: Vec<String>,
    role: RoleReadiness,
) -> DryRunPlan {
    plan_with_observed_gestures(current, proposed, source_evidence, role, &[])
}

pub fn plan_with_observed_gestures(
    current: Vec<Binding>,
    proposed: Binding,
    source_evidence: Vec<String>,
    role: RoleReadiness,
    observed_gestures: &[ObservedGestureCandidate],
) -> DryRunPlan {
    let mut all = current.clone();
    all.push(proposed.clone());
    let found = conflicts(&all);
    let proposed_index = all.len() - 1;
    let conflicts = found
        .into_iter()
        .filter(|c| match c {
            Conflict::ExactGesture { left, right, .. } => {
                *left == proposed_index || *right == proposed_index
            }
            Conflict::TapChordOverlap { tap, chord, .. } => {
                *tap == proposed_index || *chord == proposed_index
            }
            Conflict::ProviderUnsupported { binding } => *binding == proposed_index,
        })
        .collect::<Vec<_>>();
    let observed_conflict_candidates = observed_gesture_conflicts(&proposed, observed_gestures);
    let association_block = proposed.semantic_action == SemanticAction::OpenDefaultWebBrowser
        && matches!(
            role,
            RoleReadiness::Conflict | RoleReadiness::Unavailable | RoleReadiness::Invalid
        );
    let browser_provider_unavailable = proposed.semantic_action
        == SemanticAction::OpenDefaultWebBrowser
        && matches!(role, RoleReadiness::ProviderUnavailable);
    let tap_backend_block = matches!(
        proposed.gesture,
        Gesture::ModifierTap {
            modifier: Modifier::Super | Modifier::SuperLeft | Modifier::SuperRight
        }
    );
    let enabled = proposed.enabled;
    let validation = proposed.validate();
    let valid = enabled
        && validation.is_ok()
        && conflicts.is_empty()
        && observed_conflict_candidates.is_empty()
        && !association_block
        && !browser_provider_unavailable
        && !tap_backend_block;
    let resolution = if !enabled {
        BindingResolution::Disabled
    } else if let Err(error) = validation {
        BindingResolution::InvalidContext {
            reason: format!("invalid binding: {error:?}"),
        }
    } else if association_block {
        BindingResolution::AssociationConflict {
            role: "WebBrowser".into(),
        }
    } else if browser_provider_unavailable {
        BindingResolution::ProviderUnavailable {
            provider:
                "resolved WebBrowser desktop entry is not admitted by the bounded launch planner"
                    .into(),
        }
    } else if tap_backend_block {
        BindingResolution::UnsupportedGesture {
            gesture: "no qualified 01A backend for isolated Super tap".into(),
        }
    } else if !conflicts.is_empty() || !observed_conflict_candidates.is_empty() {
        BindingResolution::Conflict {
            reasons: conflicts
                .iter()
                .map(|c| format!("{c:?}"))
                .chain(
                    observed_conflict_candidates
                        .iter()
                        .map(|c| format!("registered gesture candidate {c:?}")),
                )
                .collect(),
        }
    } else {
        BindingResolution::Ready
    };
    let mut required_authority = vec![RequiredAuthority::FutureSettingsAuthority];
    if matches!(
        proposed.gesture,
        Gesture::ModifierTap { .. } | Gesture::KeyChord { .. } | Gesture::PointerChord { .. }
    ) {
        required_authority.push(RequiredAuthority::FutureCompositorConfigurationAuthority);
    }
    let mut required_capabilities = vec![RequiredCapability::CompositorBindingProjection];
    if matches!(
        proposed.gesture,
        Gesture::ModifierTap {
            modifier: Modifier::Super | Modifier::SuperLeft | Modifier::SuperRight
        }
    ) {
        required_capabilities.push(RequiredCapability::SuperTapArbitration);
    }
    if proposed.semantic_action == SemanticAction::OpenDefaultWebBrowser {
        required_capabilities.push(RequiredCapability::DefaultWebBrowserRoleResolution);
    }
    if matches!(
        proposed.provider,
        ProviderRef::DesktopEntry {
            operation: crate::model::DesktopProviderOperation::Toggle
                | crate::model::DesktopProviderOperation::Show
                | crate::model::DesktopProviderOperation::Hide,
            ..
        }
    ) {
        required_capabilities.push(RequiredCapability::ProviderOperationProof);
    }
    let reversible_operations_that_would_be_needed = vec![
        ReversibleOperation::PreservePriorBindingValue,
        ReversibleOperation::WriteProposedBinding,
        ReversibleOperation::ValidateEffectiveRegistration,
        ReversibleOperation::RestorePriorBindingValue,
    ];
    DryRunPlan {
        evidence_class: "PROPOSED_GNOSIX_MAPPING",
        current,
        proposed,
        conflicts,
        observed_conflict_candidates,
        affected_sources: source_evidence,
        required_authority,
        required_capabilities,
        reversible_operations_that_would_be_needed,
        would_apply: false,
        resolution,
        valid,
    }
}
pub fn sample_appstack_toggle() -> Binding {
    Binding {
        gesture: Gesture::ModifierTap {
            modifier: Modifier::Super,
        },
        semantic_action: SemanticAction::ToggleApplicationList,
        context: BindingContext::Global,
        provider: ProviderRef::Builtin {
            provider: crate::model::BuiltinProvider::GnosixAppStack,
        },
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DefaultApplicationRole;
    #[test]
    fn appstack_super_plan_exposes_unqualified_backend_and_no_apply_path() {
        let p = plan(
            vec![],
            sample_appstack_toggle(),
            vec!["~/.config/hypr/hyprland.lua".into()],
            RoleReadiness::NotRelevant,
        );
        assert!(!p.valid);
        assert!(!p.would_apply);
        assert!(matches!(
            p.resolution,
            BindingResolution::UnsupportedGesture { .. }
        ));
        assert!(
            p.required_authority
                .contains(&RequiredAuthority::FutureSettingsAuthority)
        );
        assert!(
            p.required_authority
                .contains(&RequiredAuthority::FutureCompositorConfigurationAuthority)
        );
        assert!(
            p.required_capabilities
                .contains(&RequiredCapability::SuperTapArbitration)
        );
        assert_eq!(p.reversible_operations_that_would_be_needed.len(), 4);
    }
    #[test]
    fn browser_conflict_blocks_dry_run_without_selecting_a_desktop_entry() {
        let binding = Binding {
            gesture: Gesture::KeyChord {
                modifiers: vec![Modifier::Super],
                key: crate::model::KeyId::parse("W").unwrap(),
                trigger: crate::model::KeyTrigger::Press,
            },
            semantic_action: SemanticAction::OpenDefaultWebBrowser,
            context: BindingContext::Global,
            provider: ProviderRef::DefaultApplicationRole {
                role: DefaultApplicationRole::WebBrowser,
            },
            enabled: true,
        };
        let p = plan(vec![], binding, vec![], RoleReadiness::Conflict);
        assert!(!p.valid);
        assert_eq!(
            p.resolution,
            BindingResolution::AssociationConflict {
                role: "WebBrowser".into()
            }
        );
        assert!(matches!(
            p.proposed.provider,
            ProviderRef::DefaultApplicationRole { .. }
        ));
    }

    #[test]
    fn unavailable_browser_provider_is_not_an_association_conflict() {
        let binding = Binding {
            gesture: Gesture::KeyChord {
                modifiers: vec![Modifier::Super],
                key: crate::model::KeyId::parse("W").unwrap(),
                trigger: crate::model::KeyTrigger::Press,
            },
            semantic_action: SemanticAction::OpenDefaultWebBrowser,
            context: BindingContext::Global,
            provider: ProviderRef::DefaultApplicationRole {
                role: DefaultApplicationRole::WebBrowser,
            },
            enabled: true,
        };
        let p = plan(vec![], binding, vec![], RoleReadiness::ProviderUnavailable);
        assert!(!p.valid);
        assert!(matches!(
            p.resolution,
            BindingResolution::ProviderUnavailable { .. }
        ));
        assert!(!p.would_apply);
    }
    #[test]
    fn invalid_proposed_binding_fails_closed_in_dry_run() {
        let mut binding = sample_appstack_toggle();
        binding.context = BindingContext::CompositorSubmap { name: " ".into() };
        let p = plan(vec![], binding, vec![], RoleReadiness::NotRelevant);
        assert!(!p.valid);
        assert!(matches!(
            p.resolution,
            BindingResolution::InvalidContext { .. }
        ));
        assert!(!p.would_apply);
    }
    #[test]
    fn unmapped_registered_gestures_still_block_a_super_tap_candidate() {
        let candidate = crate::resolution::ObservedGestureCandidate {
            runtime_index: 41,
            gesture: Gesture::KeyChord {
                modifiers: vec![Modifier::Super],
                key: crate::model::KeyId::parse("Q").unwrap(),
                trigger: crate::model::KeyTrigger::Press,
            },
            context: BindingContext::Global,
            semantic_action: None,
        };
        let p = plan_with_observed_gestures(
            vec![],
            sample_appstack_toggle(),
            vec![],
            RoleReadiness::NotRelevant,
            &[candidate],
        );
        assert!(!p.valid);
        assert!(matches!(
            p.resolution,
            BindingResolution::UnsupportedGesture { .. }
        ));
        assert_eq!(p.observed_conflict_candidates.len(), 1);
        assert_eq!(p.observed_conflict_candidates[0].runtime_index, 41);
        assert!(!p.would_apply);
    }
}
