//! Typed tray interaction vocabulary.
//!
//! UI callbacks in this module never see a bus service string, an object
//! path, an interface name, a method name, or arbitrary coordinates. Those
//! remain provider and transport implementation details.
//!
//! Every intent carries an [`ExactTrayItemRef`], which binds enough live
//! identity to distinguish the observed instance of an item, so that a stale
//! rendered icon can never act on a newly recycled bus identity.

use crate::product_tray::{TrayItemId, TrayItemSnapshot, TraySnapshot};

/// StatusNotifierItem exposes activation only for one exact observed item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactTrayItemRef {
    /// Stable, human-meaningful item identifier advertised by the exporter.
    pub id: TrayItemId,
    /// Bus address the observation came from.
    pub service: String,
    /// Object path the observation came from.
    pub path: String,
    /// Unique-name owner resolved at observation time.
    ///
    /// A well-known service name can be re-requested by a different process
    /// while a rendered icon is still on screen. Comparing the current owner
    /// against this value is what detects that substitution.
    pub owner: String,
}

impl ExactTrayItemRef {
    /// Binds the identity of an observed item, including live owner
    /// provenance.
    pub fn from_observed(item: &TrayItemSnapshot, owner: impl Into<String>) -> Self {
        Self {
            id: item.id.clone(),
            service: item.service.clone(),
            path: item.path.clone(),
            owner: owner.into(),
        }
    }
}

/// Why a rendered interaction could not be dispatched to its intended target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaleReason {
    /// The item is no longer a member of the current tray.
    MembershipLost,
    /// The bus address behind the rendered identity now has a different owner.
    OwnerReplaced,
    /// The menu layout moved on, so the rendered menu item no longer denotes
    /// the same semantic action.
    MenuRevisionChanged,
    /// The rendered menu item does not exist in the current revision.
    MenuItemAbsent,
}

/// Scroll direction, kept typed so a widget cannot smuggle an arbitrary
/// protocol string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayScrollAxis {
    Horizontal,
    Vertical,
}

/// Bounded scroll amount, in StatusNotifierItem wheel units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayScrollDelta(i32);

impl TrayScrollDelta {
    /// StatusNotifierItem scroll deltas are conventionally multiples of 120,
    /// and the protocol has no defined behaviour beyond a modest range.
    pub const MAX: i32 = 120 * 10;

    pub fn new(delta: i32) -> Self {
        Self(delta.clamp(-Self::MAX, Self::MAX))
    }

    pub fn get(self) -> i32 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// The only menu event this slice exposes. The transport builds the protocol
/// payload internally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuEvent {
    Clicked,
}

impl TrayMenuEvent {
    pub fn protocol_id(self) -> &'static str {
        match self {
            Self::Clicked => "clicked",
        }
    }
}

/// A menu action bound to the exact item, menu entry, and layout revision it
/// was rendered from.
///
/// Menu identifiers are only meaningful inside one layout revision. Binding
/// the revision is what prevents a click on a stale surface from being
/// delivered to a semantically different menu entry that happens to reuse
/// the same integer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuEventBinding {
    pub item: ExactTrayItemRef,
    pub menu_id: i32,
    pub revision: u32,
    pub event: TrayMenuEvent,
    /// Label observed when the action was rendered, used to detect that an
    /// entry was repurposed across a revision boundary.
    pub rendered_label: Option<String>,
}

impl TrayMenuEventBinding {
    /// Checks that this binding still denotes its intended semantic target.
    pub fn revalidate(
        &self,
        menu: &crate::product_tray_menu::TrayMenuSnapshot,
    ) -> Result<(), StaleReason> {
        if menu.revision != self.revision {
            return Err(StaleReason::MenuRevisionChanged);
        }
        match menu.find_item(self.menu_id) {
            None => Err(StaleReason::MenuItemAbsent),
            Some(item) if item.label != self.rendered_label => Err(StaleReason::MenuItemAbsent),
            Some(_) => Ok(()),
        }
    }
}

/// Screen-coordinate hint for operations that accept one.
///
/// The values are derived from the clicked item's resolved geometry and the
/// owning output's origin. A widget supplies geometry; it never fabricates a
/// screen coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayInteractionAnchor {
    /// Logical origin of the output the interaction happened on.
    pub output_origin: (i32, i32),
    /// Point within that output, in logical pixels.
    pub local: (f32, f32),
    /// Resulting screen-coordinate hint, in logical pixels.
    pub screen: (i32, i32),
}

impl TrayInteractionAnchor {
    /// Derives the screen hint from a local point and an output origin.
    pub fn from_local_point(output_origin: (i32, i32), local: (f32, f32)) -> Option<Self> {
        if !local.0.is_finite() || !local.1.is_finite() {
            return None;
        }
        let screen = (
            output_origin.0.saturating_add(local.0.round() as i32),
            output_origin.1.saturating_add(local.1.round() as i32),
        );
        Some(Self {
            output_origin,
            local,
            screen,
        })
    }

    /// Derives the hint from the centre of a resolved item rectangle, which is
    /// the point a click on that rectangle actually targeted.
    pub fn from_item_rect(
        output_origin: (i32, i32),
        rect: crate::product_tray_geometry::ResolvedItemRect,
    ) -> Option<Self> {
        Self::from_local_point(output_origin, rect.center())
    }
}

/// Typed semantic intent. No protocol vocabulary crosses this boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum TrayIntent {
    Activate {
        item: ExactTrayItemRef,
        anchor: TrayInteractionAnchor,
    },
    SecondaryActivate {
        item: ExactTrayItemRef,
        anchor: TrayInteractionAnchor,
    },
    OpenMenu {
        item: ExactTrayItemRef,
        anchor: TrayInteractionAnchor,
    },
    Scroll {
        item: ExactTrayItemRef,
        axis: TrayScrollAxis,
        delta: TrayScrollDelta,
    },
    MenuEvent {
        binding: TrayMenuEventBinding,
        anchor: TrayInteractionAnchor,
    },
}

/// The protocol operation a resolved intent maps to. This is the last point at
/// which a transport may look at protocol vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayOperation {
    Activate,
    SecondaryActivate,
    ContextMenu,
    ScrollHorizontal,
    ScrollVertical,
    MenuAboutToShow,
    MenuEventClicked,
}

impl TrayOperation {
    /// Whether this slice can express the operation for a given exporter
    /// surface. Callers still consult the observed item, because support is a
    /// property of the remote object rather than of the intent alone.
    pub fn requires_exporter_support(self) -> bool {
        matches!(
            self,
            Self::ScrollHorizontal | Self::ScrollVertical | Self::SecondaryActivate
        )
    }

    /// Stable token used in cross-process relay receipts.
    ///
    /// A deliberate wire vocabulary rather than `Debug`, so a receipt cannot
    /// change shape because a diagnostic string was reworded.
    pub fn token(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::SecondaryActivate => "secondary_activate",
            Self::ContextMenu => "context_menu",
            Self::ScrollHorizontal => "scroll_horizontal",
            Self::ScrollVertical => "scroll_vertical",
            Self::MenuAboutToShow => "menu_about_to_show",
            Self::MenuEventClicked => "menu_event_clicked",
        }
    }
}

impl TrayIntent {
    /// The protocol operation this intent resolves to, once policy has been
    /// applied.
    pub fn operation(&self) -> TrayOperation {
        match self {
            Self::Activate { .. } => TrayOperation::Activate,
            Self::SecondaryActivate { .. } => TrayOperation::SecondaryActivate,
            Self::OpenMenu { .. } => TrayOperation::ContextMenu,
            Self::Scroll { axis, .. } => match axis {
                TrayScrollAxis::Horizontal => TrayOperation::ScrollHorizontal,
                TrayScrollAxis::Vertical => TrayOperation::ScrollVertical,
            },
            Self::MenuEvent { .. } => TrayOperation::MenuEventClicked,
        }
    }

    /// The exact item this intent is bound to.
    pub fn item(&self) -> &ExactTrayItemRef {
        match self {
            Self::Activate { item, .. }
            | Self::SecondaryActivate { item, .. }
            | Self::OpenMenu { item, .. }
            | Self::Scroll { item, .. } => item,
            Self::MenuEvent { binding, .. } => &binding.item,
        }
    }
}

/// What a primary interaction on an item should actually do.
///
/// This decision belongs to the typed policy layer rather than to a widget,
/// because an item may declare that it is itself a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryTrayInteraction {
    Activate,
    OpenMenu,
}

/// Decides what a primary interaction means for one observed item.
pub fn primary_interaction(item: &TrayItemSnapshot) -> PrimaryTrayInteraction {
    primary_interaction_policy(item.item_is_menu, item.menu_path.is_some())
}

/// Decides what a primary interaction means from the observed facts the
/// decision depends on.
///
/// `ItemIsMenu` wins even when no menu path is exported: the product must not
/// silently fall back to a blind `Activate` on an item that declared itself a
/// menu. That case is refused downstream, where the reason is visible, rather
/// than being quietly rewritten into an activation here.
///
/// `has_menu` does not change the primary decision. It is still an argument,
/// because presentation and the dispatch boundary both need it, and keeping it
/// named documents that the absence of a menu is not a reason to activate.
pub fn primary_interaction_policy(item_is_menu: bool, has_menu: bool) -> PrimaryTrayInteraction {
    let _ = has_menu;
    if item_is_menu {
        PrimaryTrayInteraction::OpenMenu
    } else {
        PrimaryTrayInteraction::Activate
    }
}

/// Typed result of an intent. No D-Bus error object reaches rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayIntentOutcome {
    /// The protocol operation was dispatched.
    Completed { operation: TrayOperation },
    /// The rendered target no longer denotes its intended subject.
    RefusedStale {
        reason: StaleReason,
        operation: TrayOperation,
    },
    /// The provider is not currently observing a tray.
    Unavailable,
    /// The exporter does not offer the requested operation.
    Unsupported { operation: TrayOperation },
    /// The dispatch itself failed.
    TransportFailure { operation: TrayOperation },
}

impl TrayIntentOutcome {
    /// The operation this outcome describes, for receipts and feedback.
    pub fn operation(&self) -> TrayOperation {
        match self {
            Self::Completed { operation }
            | Self::RefusedStale { operation, .. }
            | Self::Unsupported { operation }
            | Self::TransportFailure { operation } => *operation,
            Self::Unavailable => TrayOperation::Activate,
        }
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Product feedback tone for the receipt.
    pub fn feedback_tone(&self) -> TrayOutcomeTone {
        match self {
            Self::Completed { .. } => TrayOutcomeTone::Success,
            Self::RefusedStale { .. } => TrayOutcomeTone::Warning,
            Self::Unavailable => TrayOutcomeTone::Neutral,
            Self::Unsupported { .. } => TrayOutcomeTone::Neutral,
            Self::TransportFailure { .. } => TrayOutcomeTone::Danger,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayOutcomeTone {
    Success,
    Warning,
    Neutral,
    Danger,
}

/// Confirms an intent still denotes its observed subject before dispatch.
///
/// Membership and owner provenance are both re-checked. A stale interaction is
/// refused rather than heuristically retargeted, and there is deliberately no
/// lookup by title, icon, or index.
pub fn revalidate_against(
    intent: &TrayIntent,
    snapshot: &TraySnapshot,
    current_owner: &dyn Fn(&str) -> Option<String>,
) -> Result<(), StaleReason> {
    let target = intent.item();
    let live = snapshot
        .items()
        .iter()
        .find(|item| {
            item.id == target.id && item.service == target.service && item.path == target.path
        })
        .ok_or(StaleReason::MembershipLost)?;

    if live.id != target.id {
        return Err(StaleReason::MembershipLost);
    }

    match current_owner(&target.service) {
        Some(owner) if owner == target.owner => Ok(()),
        Some(_) => Err(StaleReason::OwnerReplaced),
        None => Err(StaleReason::MembershipLost),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_tray::TrayCategory;

    fn item(id: &str, item_is_menu: bool) -> TrayItemSnapshot {
        TrayItemSnapshot {
            id: TrayItemId::new(id),
            service: format!("org.kde.StatusNotifierItem-{id}"),
            path: "/StatusNotifierItem".to_owned(),
            title: id.to_owned(),
            status: crate::product_tray::TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            icons: crate::product_tray::TrayIconFacts::named("probe"),
            tooltip: None,
            menu_path: Some("/MenuBar".to_owned()),
            item_is_menu,
            owner: ":1.donor15".to_owned(),
        }
    }

    /// Resolver shape used by `revalidate_against`.
    type OwnerProbe = Box<dyn Fn(&str) -> Option<String>>;

    fn snapshot_of(items: Vec<TrayItemSnapshot>) -> TraySnapshot {
        TraySnapshot::from_items(items)
    }

    fn anchor() -> TrayInteractionAnchor {
        TrayInteractionAnchor::from_local_point((1930, 390), (10.0, 5.0)).unwrap()
    }

    fn owner_of(name: &str) -> OwnerProbe {
        let owned = name.to_owned();
        Box::new(move |service: &str| (service == owned).then(|| ":1.42".to_owned()))
    }

    #[test]
    fn scroll_delta_is_bounded() {
        assert_eq!(TrayScrollDelta::new(0).get(), 0);
        assert_eq!(TrayScrollDelta::new(120).get(), 120);
        assert_eq!(TrayScrollDelta::new(-120).get(), -120);
        assert_eq!(
            TrayScrollDelta::new(i32::MAX).get(),
            TrayScrollDelta::MAX,
            "an unbounded wheel delta must be clamped"
        );
        assert_eq!(TrayScrollDelta::new(i32::MIN).get(), -TrayScrollDelta::MAX);
    }

    #[test]
    fn item_is_menu_routes_primary_interaction_to_the_menu() {
        assert_eq!(
            primary_interaction(&item("plain", false)),
            PrimaryTrayInteraction::Activate
        );
        assert_eq!(
            primary_interaction(&item("menuish", true)),
            PrimaryTrayInteraction::OpenMenu,
            "an item that declares itself a menu must not be blindly activated"
        );
    }

    #[test]
    fn anchor_is_derived_from_output_origin_not_fabricated() {
        let anchor = TrayInteractionAnchor::from_local_point((-630, 590), (12.0, 8.0)).unwrap();
        assert_eq!(anchor.output_origin, (-630, 590));
        assert_eq!(anchor.local, (12.0, 8.0));
        assert_eq!(
            anchor.screen,
            (-618, 598),
            "negative origins must be honoured"
        );

        assert!(
            TrayInteractionAnchor::from_local_point((0, 0), (f32::NAN, 0.0)).is_none(),
            "a non-finite local point must not become a coordinate hint"
        );
    }

    #[test]
    fn fresh_intent_passes_membership_and_owner_revalidation() {
        let snapshot = snapshot_of(vec![item("steam", false)]);
        let observed = snapshot.items()[0].clone();
        let intent = TrayIntent::Activate {
            item: ExactTrayItemRef::from_observed(&observed, ":1.42"),
            anchor: anchor(),
        };

        assert_eq!(
            revalidate_against(
                &intent,
                &snapshot,
                &owner_of("org.kde.StatusNotifierItem-steam")
            ),
            Ok(())
        );
    }

    #[test]
    fn disappeared_item_is_refused_not_retargeted() {
        let observed = item("steam", false);
        let intent = TrayIntent::Activate {
            item: ExactTrayItemRef::from_observed(&observed, ":1.42"),
            anchor: anchor(),
        };

        // The tray is now empty: the same rendered icon must not act.
        let empty = snapshot_of(vec![]);
        assert_eq!(
            revalidate_against(
                &intent,
                &empty,
                &owner_of("org.kde.StatusNotifierItem-steam")
            ),
            Err(StaleReason::MembershipLost)
        );
    }

    #[test]
    fn replaced_bus_owner_is_refused() {
        let snapshot = snapshot_of(vec![item("steam", false)]);
        let observed = snapshot.items()[0].clone();
        let intent = TrayIntent::Activate {
            item: ExactTrayItemRef::from_observed(&observed, ":1.42"),
            anchor: anchor(),
        };

        let recycled = Box::new(|_: &str| Some(":1.99".to_owned()));
        assert_eq!(
            revalidate_against(&intent, &snapshot, &recycled),
            Err(StaleReason::OwnerReplaced),
            "a re-requested well-known name must not inherit the old identity"
        );
    }

    #[test]
    fn identity_is_not_matched_by_title_or_index() {
        // A different item now occupies the same slot with the same title.
        let snapshot = snapshot_of(vec![item("steam", false)]);
        let intent = TrayIntent::Activate {
            item: ExactTrayItemRef {
                id: TrayItemId::new("steam"),
                service: "org.kde.StatusNotifierItem-other".to_owned(),
                path: "/StatusNotifierItem".to_owned(),
                owner: ":1.42".to_owned(),
            },
            anchor: anchor(),
        };

        assert_eq!(
            revalidate_against(
                &intent,
                &snapshot,
                &owner_of("org.kde.StatusNotifierItem-other")
            ),
            Err(StaleReason::MembershipLost),
            "identity is the bound bus entry, never the label"
        );
    }
}
