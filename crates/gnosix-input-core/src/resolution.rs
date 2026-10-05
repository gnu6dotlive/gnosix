//! Association consistency and provider support are independent facts.
use crate::model::{
    AssociationObservation, Binding, BrowserAssociation, BrowserAssociationEvidence,
    BuiltinProvider, CapabilityEvidence, DefaultApplicationRole, DesktopProviderOperation, Gesture,
    ProviderRef, SemanticAction,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum BrowserRoleResolution {
    Consistent {
        id: String,
    },
    Conflict {
        associations: BTreeMap<BrowserAssociation, String>,
    },
    Invalid {
        association: BrowserAssociation,
        raw: String,
        detail: String,
    },
    Unavailable {
        missing: Vec<BrowserAssociation>,
    },
}
pub fn resolve_browser_role(evidence: &[BrowserAssociationEvidence]) -> BrowserRoleResolution {
    let mut values = BTreeMap::new();
    let mut missing = Vec::new();
    for kind in BrowserAssociation::REQUIRED {
        let mut matching = evidence.iter().filter(|e| e.association == kind);
        let first = matching.next().map(|e| &e.observation);
        if matching.next().is_some() {
            return BrowserRoleResolution::Invalid {
                association: kind,
                raw: "duplicate observation".into(),
                detail: "association evidence must contain exactly one observation per source"
                    .into(),
            };
        }
        match first {
            Some(AssociationObservation::Observed { id }) => {
                values.insert(kind, id.as_str().to_owned());
            }
            Some(AssociationObservation::Invalid { raw, detail }) => {
                return BrowserRoleResolution::Invalid {
                    association: kind,
                    raw: raw.clone(),
                    detail: detail.clone(),
                };
            }
            Some(AssociationObservation::Unavailable { .. }) | None => missing.push(kind),
        }
    }
    let unique = values.values().collect::<std::collections::BTreeSet<_>>();
    if unique.len() > 1 {
        BrowserRoleResolution::Conflict {
            associations: values,
        }
    } else if !missing.is_empty() {
        BrowserRoleResolution::Unavailable { missing }
    } else if let Some(id) = values.values().next() {
        BrowserRoleResolution::Consistent { id: id.clone() }
    } else {
        BrowserRoleResolution::Unavailable {
            missing: BrowserAssociation::REQUIRED.to_vec(),
        }
    }
}
pub fn supports(provider: &ProviderRef, action: SemanticAction) -> bool {
    match (provider, action) {
        (
            ProviderRef::Builtin {
                provider: BuiltinProvider::GnosixAppStack,
            },
            SemanticAction::ToggleApplicationList,
        ) => true,
        (
            ProviderRef::DesktopEntry {
                operation:
                    DesktopProviderOperation::Toggle
                    | DesktopProviderOperation::Show
                    | DesktopProviderOperation::Hide,
                ..
            },
            _,
        ) => false,
        (
            ProviderRef::DesktopEntry {
                operation: DesktopProviderOperation::Launch,
                ..
            },
            SemanticAction::OpenDefaultWebBrowser,
        ) => true,
        (
            ProviderRef::DefaultApplicationRole {
                role: DefaultApplicationRole::WebBrowser,
            },
            SemanticAction::OpenDefaultWebBrowser,
        ) => true,
        (
            ProviderRef::Compositor {
                capability: crate::model::CompositorCapability::MoveWindow,
            },
            SemanticAction::MoveWindow,
        ) => true,
        (
            ProviderRef::Compositor {
                capability: crate::model::CompositorCapability::ResizeWindow,
            },
            SemanticAction::ResizeWindow,
        ) => true,
        (
            ProviderRef::Gnosix {
                capability: crate::model::GnosixCapability::CaptureScreenshot,
            },
            SemanticAction::CaptureScreenshot,
        ) => true,
        _ => false,
    }
}
pub fn external_operation_is_advertisable(
    operation: DesktopProviderOperation,
    evidence: CapabilityEvidence,
) -> bool {
    operation == DesktopProviderOperation::Launch || evidence == CapabilityEvidence::Verified
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Conflict {
    ExactGesture {
        left: usize,
        right: usize,
        incompatible_actions: bool,
        context_sensitive: bool,
    },
    TapChordOverlap {
        tap: usize,
        chord: usize,
        context_sensitive: bool,
    },
    ProviderUnsupported {
        binding: usize,
    },
}

/// A registered runtime gesture with no asserted product meaning. It is kept
/// separate from `Binding` so an opaque compositor callback cannot acquire a
/// made-up semantic action merely to participate in conflict analysis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedGestureCandidate {
    pub runtime_index: usize,
    pub gesture: Gesture,
    pub context: crate::model::BindingContext,
    pub semantic_action: Option<crate::model::SemanticAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedGestureRelation {
    ExactGesture,
    TapChordOverlap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedGestureConflictCandidate {
    pub runtime_index: usize,
    pub relation: ObservedGestureRelation,
    pub context_sensitive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedGesturePairConflict {
    pub left_runtime_index: usize,
    pub right_runtime_index: usize,
    pub relation: ObservedGestureRelation,
    pub context_sensitive: bool,
    /// `None` means one or both callbacks have no defensible semantic mapping.
    pub incompatible_actions: Option<bool>,
}

pub fn observed_gesture_pair_conflicts(
    observed: &[ObservedGestureCandidate],
) -> Vec<ObservedGesturePairConflict> {
    let mut conflicts = Vec::new();
    for (left_index, left) in observed.iter().enumerate() {
        for right in &observed[left_index + 1..] {
            let relation = if same_gesture(&left.gesture, &right.gesture) {
                Some(ObservedGestureRelation::ExactGesture)
            } else if tap_overlap(&left.gesture, &right.gesture)
                || tap_overlap(&right.gesture, &left.gesture)
            {
                Some(ObservedGestureRelation::TapChordOverlap)
            } else {
                None
            };
            let Some(relation) = relation else { continue };
            conflicts.push(ObservedGesturePairConflict {
                left_runtime_index: left.runtime_index,
                right_runtime_index: right.runtime_index,
                relation,
                context_sensitive: left.context != right.context,
                incompatible_actions: left
                    .semantic_action
                    .zip(right.semantic_action)
                    .map(|(a, b)| a != b),
            });
        }
    }
    conflicts
}

pub fn observed_gesture_conflicts(
    proposed: &Binding,
    observed: &[ObservedGestureCandidate],
) -> Vec<ObservedGestureConflictCandidate> {
    observed
        .iter()
        .filter_map(|candidate| {
            let relation = if same_gesture(&proposed.gesture, &candidate.gesture) {
                Some(ObservedGestureRelation::ExactGesture)
            } else if tap_overlap(&proposed.gesture, &candidate.gesture)
                || tap_overlap(&candidate.gesture, &proposed.gesture)
            {
                Some(ObservedGestureRelation::TapChordOverlap)
            } else {
                None
            }?;
            Some(ObservedGestureConflictCandidate {
                runtime_index: candidate.runtime_index,
                relation,
                context_sensitive: proposed.context != candidate.context,
            })
        })
        .collect()
}

pub fn conflicts(bindings: &[Binding]) -> Vec<Conflict> {
    let mut out = Vec::new();
    for (i, a) in bindings.iter().enumerate() {
        if a.enabled
            && !matches!(a.provider, ProviderRef::UnresolvedObserved)
            && !supports(&a.provider, a.semantic_action)
        {
            out.push(Conflict::ProviderUnsupported { binding: i });
        }
        for (offset, b) in bindings[i + 1..].iter().enumerate() {
            let j = i + 1 + offset;
            if !a.enabled || !b.enabled {
                continue;
            }
            let context_sensitive = a.context != b.context;
            if same_gesture(&a.gesture, &b.gesture) {
                out.push(Conflict::ExactGesture {
                    left: i,
                    right: j,
                    incompatible_actions: a.semantic_action != b.semantic_action,
                    context_sensitive,
                });
            }
            if tap_overlap(&a.gesture, &b.gesture) {
                out.push(Conflict::TapChordOverlap {
                    tap: i,
                    chord: j,
                    context_sensitive,
                });
            }
            if tap_overlap(&b.gesture, &a.gesture) {
                out.push(Conflict::TapChordOverlap {
                    tap: j,
                    chord: i,
                    context_sensitive,
                });
            }
        }
    }
    out
}
fn tap_overlap(a: &Gesture, b: &Gesture) -> bool {
    use crate::model::Modifier;
    let is_super = |m: &Modifier| {
        matches!(
            m,
            Modifier::Super | Modifier::SuperLeft | Modifier::SuperRight
        )
    };
    matches!((a,b),(Gesture::ModifierTap{modifier},Gesture::KeyChord{modifiers,..}|Gesture::PointerChord{modifiers,..}) if is_super(modifier)&&modifiers.iter().any(is_super))
}
fn same_gesture(a: &Gesture, b: &Gesture) -> bool {
    match (a, b) {
        (Gesture::ModifierTap { modifier: a }, Gesture::ModifierTap { modifier: b }) => a == b,
        (
            Gesture::KeyChord {
                modifiers: a,
                key: ak,
                trigger: at,
            },
            Gesture::KeyChord {
                modifiers: b,
                key: bk,
                trigger: bt,
            },
        ) => {
            let (mut a, mut b) = (a.clone(), b.clone());
            a.sort();
            b.sort();
            a == b && ak == bk && at == bt
        }
        (
            Gesture::PointerChord {
                modifiers: a,
                button: ab,
                trigger: at,
            },
            Gesture::PointerChord {
                modifiers: b,
                button: bb,
                trigger: bt,
            },
        ) => {
            let (mut a, mut b) = (a.clone(), b.clone());
            a.sort();
            b.sort();
            a == b && ab == bb && at == bt
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use in_gnu_os_settings_core::DesktopEntryId;
    fn e(a: BrowserAssociation, id: &str) -> BrowserAssociationEvidence {
        BrowserAssociationEvidence {
            association: a,
            observation: AssociationObservation::Observed {
                id: DesktopEntryId::parse(id).unwrap(),
            },
        }
    }
    #[test]
    fn browser_disagreement_is_reported_as_conflict_not_selected() {
        let got = resolve_browser_role(&[
            e(
                BrowserAssociation::DefaultWebBrowser,
                "appimagetool.desktop",
            ),
            e(BrowserAssociation::Http, "google-chrome-beta.desktop"),
            e(BrowserAssociation::Https, "google-chrome-beta.desktop"),
            e(BrowserAssociation::TextHtml, "google-chrome-beta.desktop"),
        ]);
        assert!(matches!(got, BrowserRoleResolution::Conflict { .. }));
    }
    #[test]
    fn incomplete_browser_evidence_never_becomes_consistent() {
        let got = resolve_browser_role(&[e(BrowserAssociation::Http, "x.desktop")]);
        assert!(matches!(got, BrowserRoleResolution::Unavailable { .. }));
    }
    #[test]
    fn duplicate_browser_role_evidence_is_invalid_not_first_wins() {
        let got = resolve_browser_role(&[
            e(BrowserAssociation::Http, "x.desktop"),
            e(BrowserAssociation::Http, "y.desktop"),
        ]);
        assert!(matches!(got, BrowserRoleResolution::Invalid { .. }));
    }
    #[test]
    fn identical_bindings_conflict_but_disabled_ones_do_not() {
        let b = Binding {
            gesture: Gesture::ModifierTap {
                modifier: Modifier::Super,
            },
            semantic_action: SemanticAction::ToggleApplicationList,
            context: BindingContext::Global,
            provider: ProviderRef::Builtin {
                provider: BuiltinProvider::GnosixAppStack,
            },
            enabled: true,
        };
        assert_eq!(conflicts(&[b.clone(), b.clone()]).len(), 1);
        let mut off = b.clone();
        off.enabled = false;
        assert!(conflicts(&[b, off]).is_empty());
    }
    #[test]
    fn reversed_modifier_order_is_the_same_physical_gesture() {
        let mk = |modifiers| Binding {
            gesture: Gesture::KeyChord {
                modifiers,
                key: crate::model::KeyId::parse("W").unwrap(),
                trigger: crate::model::KeyTrigger::Press,
            },
            semantic_action: SemanticAction::OpenDefaultWebBrowser,
            context: crate::model::BindingContext::Global,
            provider: ProviderRef::UnresolvedObserved,
            enabled: true,
        };
        assert!(
            conflicts(&[
                mk(vec![
                    crate::model::Modifier::Super,
                    crate::model::Modifier::Shift
                ]),
                mk(vec![
                    crate::model::Modifier::Shift,
                    crate::model::Modifier::Super
                ])
            ])
            .iter()
            .any(|c| matches!(c, Conflict::ExactGesture { .. }))
        );
    }
    #[test]
    fn opaque_registered_pairs_report_candidates_without_inventing_actions() {
        let gesture = Gesture::KeyChord {
            modifiers: vec![Modifier::Super],
            key: crate::model::KeyId::parse("W").unwrap(),
            trigger: crate::model::KeyTrigger::Press,
        };
        let candidates = [
            ObservedGestureCandidate {
                runtime_index: 3,
                gesture: gesture.clone(),
                context: BindingContext::Global,
                semantic_action: None,
            },
            ObservedGestureCandidate {
                runtime_index: 8,
                gesture,
                context: BindingContext::CompositorSubmap { name: "vm".into() },
                semantic_action: None,
            },
        ];
        let conflicts = observed_gesture_pair_conflicts(&candidates);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].relation, ObservedGestureRelation::ExactGesture);
        assert!(conflicts[0].context_sensitive);
        assert_eq!(conflicts[0].incompatible_actions, None);
    }
}
