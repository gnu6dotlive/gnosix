use crate::{
    product_preferences::EffectiveProductPreferences, product_scene::ProductWorkspaceContent,
};
use gnos_ux_product_intent::ObservedWorkspaceReference;
use in_gnu_os_settings_read_model::WorkspaceDisplayMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceBarPresentation {
    pub mode: WorkspaceDisplayMode,
    pub targets: Vec<ProductWorkspaceContent>,
}

/// Pure Bar presentation projection. It may select a bounded view over already
/// observed workspace targets, but it never manufactures or retargets one.
pub fn workspace_bar_presentation(
    targets: &[ProductWorkspaceContent],
    active: Option<ObservedWorkspaceReference>,
    preferences: &EffectiveProductPreferences,
) -> WorkspaceBarPresentation {
    let mode = preferences
        .bar_overrides
        .workspace_display
        .unwrap_or(WorkspaceDisplayMode::Text);

    let Some(capacity) = preferences.bar_overrides.workspace_count else {
        return WorkspaceBarPresentation {
            mode,
            targets: targets.to_vec(),
        };
    };

    let capacity = usize::from(capacity.get()).min(targets.len());
    if capacity == targets.len() {
        return WorkspaceBarPresentation {
            mode,
            targets: targets.to_vec(),
        };
    }

    let start = active
        .and_then(|active| targets.iter().position(|target| target.reference == active))
        .map(|active_index| {
            let end = (active_index + 1).clamp(capacity, targets.len());
            end - capacity
        })
        .unwrap_or(0);

    WorkspaceBarPresentation {
        mode,
        targets: targets[start..start + capacity].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        product_preferences::{
            ProductPreferenceDefaults, SystemProductPreferences, resolve_without_profile_authority,
        },
        product_scene::ProductWorkspaceContent,
    };
    use gnos_ux_product_intent::ObservedWorkspaceReference;
    use gnosix_desktop_model::{DesktopContextGeneration, WorkspaceId};
    use in_gnu_os_settings_read_model::{WorkspaceDisplayCount, WorkspaceDisplayMode};

    fn workspace(byte: u8, label: &str) -> ProductWorkspaceContent {
        ProductWorkspaceContent {
            reference: ObservedWorkspaceReference::new(
                WorkspaceId::from_bytes([byte; 16]),
                DesktopContextGeneration(7),
            ),
            label: label.to_owned(),
        }
    }

    fn preferences() -> crate::product_preferences::EffectiveProductPreferences {
        resolve_without_profile_authority(
            ProductPreferenceDefaults::default(),
            SystemProductPreferences::default(),
        )
    }

    #[test]
    fn absent_overrides_preserve_all_targets_and_existing_text_presentation() {
        let targets = vec![workspace(1, "1"), workspace(2, "2"), workspace(3, "3")];
        let projected =
            workspace_bar_presentation(&targets, Some(targets[1].reference), &preferences());

        assert_eq!(projected.mode, WorkspaceDisplayMode::Text);
        assert_eq!(projected.targets, targets);
    }

    #[test]
    fn count_one_keeps_the_active_workspace_represented_without_retargeting_it() {
        let targets = vec![workspace(1, "1"), workspace(2, "2"), workspace(3, "3")];
        let mut prefs = preferences();
        prefs.bar_overrides.workspace_count = Some(WorkspaceDisplayCount::try_from(1_i64).unwrap());

        let projected = workspace_bar_presentation(&targets, Some(targets[2].reference), &prefs);

        assert_eq!(projected.targets, vec![targets[2].clone()]);
        assert_eq!(projected.targets[0].reference, targets[2].reference);
    }

    #[test]
    fn bounded_window_matches_donor_active_retention_without_manufacturing_workspaces() {
        let targets = (1..=6)
            .map(|n| workspace(n, &n.to_string()))
            .collect::<Vec<_>>();
        let mut prefs = preferences();
        prefs.bar_overrides.workspace_count = Some(WorkspaceDisplayCount::try_from(3_i64).unwrap());

        let projected = workspace_bar_presentation(&targets, Some(targets[4].reference), &prefs);

        assert_eq!(
            projected
                .targets
                .iter()
                .map(|target| target.reference)
                .collect::<Vec<_>>(),
            vec![
                targets[2].reference,
                targets[3].reference,
                targets[4].reference
            ]
        );
    }

    #[test]
    fn capacity_twenty_never_adds_targets_and_duplicate_labels_keep_exact_identity() {
        let targets = vec![workspace(1, "same"), workspace(9, "same")];
        let mut prefs = preferences();
        prefs.bar_overrides.workspace_count =
            Some(WorkspaceDisplayCount::try_from(20_i64).unwrap());

        let projected = workspace_bar_presentation(&targets, Some(targets[0].reference), &prefs);

        assert_eq!(projected.targets.len(), 2);
        assert_eq!(projected.targets[0].label, projected.targets[1].label);
        assert_ne!(
            projected.targets[0].reference,
            projected.targets[1].reference
        );
    }

    #[test]
    fn symbolic_display_mode_is_consumed_without_affecting_target_selection() {
        let targets = vec![workspace(1, "1"), workspace(2, "2")];
        for mode in [
            WorkspaceDisplayMode::Shapes,
            WorkspaceDisplayMode::Text,
            WorkspaceDisplayMode::Icons,
        ] {
            let mut prefs = preferences();
            prefs.bar_overrides.workspace_display = Some(mode);
            let projected =
                workspace_bar_presentation(&targets, Some(targets[0].reference), &prefs);
            assert_eq!(projected.mode, mode);
            assert_eq!(projected.targets, targets);
        }
    }
}
