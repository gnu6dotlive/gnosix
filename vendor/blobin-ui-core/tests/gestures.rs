use blobin_ui_core::gestures::{
    CancelReason, GestureAction, GestureCoordinator, GestureError, Translation,
};
use blobin_ui_core::input::{EventKind, InputEvent, InputReceipt, InputRouter, Key, Point, Units};
use blobin_ui_core::layout::{
    compute, Axis, LayoutSnapshot, LayoutStyles, Length, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};

struct Case {
    tree: UiTree,
    styles: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    surface: SurfaceId,
    a: NodeId,
    router: InputRouter,
}
impl Case {
    fn new(n: u64) -> Self {
        let mut tree = UiTree::new();
        let surface = SurfaceId(n);
        let mut root_id = None;
        let mut a = None;
        let mut b = None;
        tree.transact(|tx| {
            tx.open_surface(surface)?;
            let root = tx.append_root(surface, NodeProperties::new(Role::Container, "canvas"))?;
            root_id = Some(root);
            a = Some(tx.append_child(
                root,
                NodeProperties::new(Role::Custom("draggable".into()), "A"),
            )?);
            b = Some(tx.append_child(
                root,
                NodeProperties::new(Role::Custom("draggable".into()), "B"),
            )?);
            Ok(())
        })
        .unwrap();
        let root = root_id.unwrap();
        let a = a.unwrap();
        let b = b.unwrap();
        let mut styles = LayoutStyles::new();
        let mut style = Style::fill();
        style.axis = Axis::Row;
        style.clip_children = true;
        styles.set(&tree.snapshot(), root, style).unwrap();
        for child in [a, b] {
            styles
                .set(
                    &tree.snapshot(),
                    child,
                    Style {
                        width: Length::Px(100.0),
                        height: Length::Px(60.0),
                        ..Style::default()
                    },
                )
                .unwrap();
        }
        let viewport = Viewport::new(Size::new(240.0, 90.0), 1.5, 1);
        let layout = compute(&tree.snapshot(), &styles, surface, viewport).unwrap();
        Self {
            tree,
            styles,
            layout,
            viewport,
            surface,
            a,
            router: InputRouter::new(),
        }
    }

    fn deliver(&mut self, n: u64, kind: EventKind) -> (InputEvent, InputReceipt) {
        let event = InputEvent::for_layout(&self.layout, n, n * 1000, kind);
        let receipt = self
            .router
            .dispatch(
                &self.tree.snapshot(),
                &self.styles,
                &self.layout,
                self.viewport,
                event.clone(),
            )
            .unwrap();
        (event, receipt)
    }
    fn observe(
        &self,
        gestures: &mut GestureCoordinator,
        event: &InputEvent,
        receipt: &InputReceipt,
    ) -> Vec<GestureAction> {
        gestures
            .observe(
                &self.tree.snapshot(),
                &self.styles,
                &self.layout,
                self.viewport,
                event,
                receipt,
            )
            .unwrap()
    }
    fn step(
        &mut self,
        gestures: &mut GestureCoordinator,
        n: u64,
        kind: EventKind,
    ) -> Vec<GestureAction> {
        let (event, receipt) = self.deliver(n, kind);
        self.observe(gestures, &event, &receipt)
    }
    fn resize(&mut self) {
        self.viewport.geometry_epoch += 1;
        self.layout = compute(
            &self.tree.snapshot(),
            &self.styles,
            self.surface,
            self.viewport,
        )
        .unwrap();
    }
}

fn down(pointer: u64, x: f32, units: Units) -> EventKind {
    EventKind::PointerDown {
        pointer,
        button: 1,
        position: Point::new(x, 30.0),
        units,
    }
}
fn move_to(pointer: u64, x: f32, units: Units) -> EventKind {
    EventKind::PointerMove {
        pointer,
        position: Point::new(x, 30.0),
        units,
    }
}
fn up(pointer: u64, x: f32, units: Units) -> EventKind {
    EventKind::PointerUp {
        pointer,
        button: 1,
        position: Point::new(x, 30.0),
        units,
    }
}
fn cancel_kind(pointer: u64) -> EventKind {
    EventKind::PointerCancel { pointer }
}
fn escape() -> EventKind {
    EventKind::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    }
}
fn has_cancel(actions: &[GestureAction], reason: CancelReason) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, GestureAction::Cancel { reason: r, .. } if *r == reason))
}
fn committed(actions: &[GestureAction]) -> Option<(NodeId, Translation)> {
    actions.iter().find_map(|a| match a {
        GestureAction::Commit { target, delta, .. } => Some((*target, *delta)),
        _ => None,
    })
}

#[test]
fn absolute_preview_and_one_cow_document_commit() {
    let mut c = Case::new(1);
    let mut g = GestureCoordinator::new();
    let before = c.tree.snapshot();
    let revision = before.revision();
    let begin = c.step(&mut g, 1, down(8, 30.0, Units::Logical));
    assert!(matches!(&begin[..], [GestureAction::Begin { target, .. }] if *target == c.a));
    let p = c.step(&mut g, 2, move_to(8, 40.0, Units::Logical));
    assert!(matches!(&p[..], [GestureAction::Preview { delta, .. }] if delta.x == 10.0));
    let p = c.step(&mut g, 3, move_to(8, 55.0, Units::Logical));
    assert!(matches!(&p[..], [GestureAction::Preview { delta, .. }] if delta.x == 25.0));
    assert_eq!(
        c.tree.snapshot().revision(),
        revision,
        "previews must not mutate UI-G02"
    );
    let receipt = c.step(&mut g, 4, up(8, 70.0, Units::Logical));
    let (id, delta) = committed(&receipt).unwrap();
    assert_eq!((id, delta.x, delta.y), (c.a, 40.0, 0.0));
    // An independent consumer applies exactly one semantic transaction on Commit.
    let previous = c.tree.snapshot().get(id).unwrap().properties().clone();
    let mut updated = previous.clone();
    updated.label = format!("A translated: {}px", delta.x);
    let published = c
        .tree
        .transact(|tx| tx.set_properties(id, updated))
        .unwrap();
    assert!(published.changed());
    assert_eq!(published.revision, revision + 1);
    assert_eq!(before.get(id).unwrap().properties().label, "A");
    assert_eq!(
        c.tree.snapshot().get(id).unwrap().properties().label,
        "A translated: 40px"
    );
    assert_eq!(g.active_count(), 0);
    // The consumer owns undo; applying the saved baseline is one reversal.
    c.tree
        .transact(|tx| tx.set_properties(id, previous))
        .unwrap();
    assert_eq!(c.tree.snapshot().get(id).unwrap().properties().label, "A");
}

#[test]
fn click_never_commits_and_cancel_keeps_tree_unchanged() {
    let mut c = Case::new(2);
    let mut g = GestureCoordinator::new();
    let revision = c.tree.snapshot().revision();
    c.step(&mut g, 1, down(1, 10.0, Units::Logical));
    let actions = c.step(&mut g, 2, up(1, 10.0, Units::Logical));
    assert!(has_cancel(&actions, CancelReason::NoTravel));
    assert_eq!(c.tree.snapshot().revision(), revision);
    c.step(&mut g, 3, down(2, 10.0, Units::Logical));
    c.step(&mut g, 4, move_to(2, 15.0, Units::Logical));
    assert!(has_cancel(
        &c.step(&mut g, 5, cancel_kind(2)),
        CancelReason::PointerCancelled
    ));
    assert_eq!(c.tree.snapshot().revision(), revision);
    assert_eq!(g.active_count(), 0);
}

#[test]
fn pointer_capture_allows_preview_and_commit_outside_canvas() {
    let mut c = Case::new(3);
    let mut g = GestureCoordinator::new();
    c.step(&mut g, 1, down(1, 20.0, Units::Logical));
    let a = c.step(&mut g, 2, move_to(1, 350.0, Units::Logical));
    assert!(
        matches!(&a[..], [GestureAction::Preview { target, delta, .. }] if *target == c.a && delta.x == 330.0)
    );
    let a = c.step(&mut g, 3, up(1, 360.0, Units::Logical));
    assert_eq!(committed(&a).map(|v| v.1.x), Some(340.0));
}

#[test]
fn physical_pixels_normalize_through_ui_g03_dpr() {
    let mut c = Case::new(4);
    let mut g = GestureCoordinator::new();
    let start = c.step(&mut g, 1, down(1, 30.0, Units::Physical));
    assert!(
        matches!(&start[..], [GestureAction::Begin { origin, .. }] if origin.x == 20.0 && origin.y == 20.0)
    );
    let final_action = c.step(&mut g, 2, up(1, 60.0, Units::Physical));
    assert_eq!(committed(&final_action).map(|v| v.1.x), Some(20.0));
}

#[test]
fn duplicate_or_mismatched_receipts_rejected_without_duplicating_commit() {
    let mut c = Case::new(5);
    let mut g = GestureCoordinator::new();
    let (event, receipt) = c.deliver(1, down(1, 20.0, Units::Logical));
    assert_eq!(c.observe(&mut g, &event, &receipt).len(), 1);
    let err = g
        .observe(
            &c.tree.snapshot(),
            &c.styles,
            &c.layout,
            c.viewport,
            &event,
            &receipt,
        )
        .unwrap_err();
    assert_eq!(err, GestureError::ReplayedReceipt);
    let mut mismatched = receipt.clone();
    mismatched.serial += 1;
    assert_eq!(
        g.observe(
            &c.tree.snapshot(),
            &c.styles,
            &c.layout,
            c.viewport,
            &event,
            &mismatched
        )
        .unwrap_err(),
        GestureError::MismatchedReceipt
    );
    let actions = c.step(&mut g, 2, up(1, 50.0, Units::Logical));
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, GestureAction::Commit { .. }))
            .count(),
        1
    );
}

#[test]
fn escape_and_window_blur_cancel_all_pointers_but_only_on_target_surface() {
    let mut a = Case::new(6);
    let mut b = Case::new(7);
    let mut g = GestureCoordinator::new();
    a.step(&mut g, 1, down(1, 20.0, Units::Logical));
    a.step(&mut g, 2, down(2, 130.0, Units::Logical));
    b.step(&mut g, 1, down(1, 20.0, Units::Logical));
    assert_eq!(g.active_count(), 3);
    let cancelled = a.step(&mut g, 3, escape());
    assert_eq!(cancelled.len(), 2);
    assert!(cancelled.iter().all(|v| matches!(
        v,
        GestureAction::Cancel {
            reason: CancelReason::Escape,
            ..
        }
    )));
    assert_eq!(g.active_count(), 1);
    assert!(has_cancel(
        &b.step(&mut g, 2, EventKind::WindowBlur),
        CancelReason::FocusLost
    ));
    assert_eq!(g.active_count(), 0);
}

#[test]
fn resize_cancels_preexisting_gesture_instead_of_rebasing() {
    let mut c = Case::new(8);
    let mut g = GestureCoordinator::new();
    c.step(&mut g, 1, down(1, 20.0, Units::Logical));
    c.resize();
    let actions = c.step(&mut g, 2, move_to(1, 60.0, Units::Logical));
    assert!(has_cancel(&actions, CancelReason::LayoutChanged));
    assert_eq!(
        committed(&c.step(&mut g, 3, up(1, 90.0, Units::Logical))),
        None
    );
}

#[test]
fn invalid_layout_or_event_never_commits_and_returns_error() {
    let mut c = Case::new(9);
    let mut g = GestureCoordinator::new();
    let (ev, receipt) = c.deliver(1, down(1, 20.0, Units::Logical));
    let mut forged = ev.clone();
    forged.geometry_epoch = 99;
    assert_eq!(
        g.observe(
            &c.tree.snapshot(),
            &c.styles,
            &c.layout,
            c.viewport,
            &forged,
            &receipt
        )
        .unwrap_err(),
        GestureError::StaleEvent
    );
    assert_eq!(g.active_count(), 0);
    c.observe(&mut g, &ev, &receipt);
    let (ev2, receipt2) = c.deliver(2, up(1, 40.0, Units::Logical));
    c.resize();
    assert_eq!(
        g.observe(
            &c.tree.snapshot(),
            &c.styles,
            &c.layout,
            c.viewport,
            &ev2,
            &receipt2
        )
        .unwrap_err(),
        GestureError::StaleEvent
    );
    assert_eq!(g.active_count(), 1);
    assert!(has_cancel(
        &g.close_surface(c.surface),
        CancelReason::SurfaceClosed
    ));
}

#[test]
fn explicit_close_resets_only_that_surface_ledger() {
    let mut a = Case::new(10);
    let mut b = Case::new(11);
    let mut g = GestureCoordinator::new();
    a.step(&mut g, 1, down(1, 10.0, Units::Logical));
    b.step(&mut g, 1, down(1, 10.0, Units::Logical));
    assert_eq!(g.close_surface(a.surface).len(), 1);
    assert_eq!(g.active_count(), 1);
    // The caller may create a new surface lifetime with the same numeric ID.
    a.tree
        .transact(|tx| {
            tx.close_surface(a.surface)?;
            tx.open_surface(a.surface)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(a.tree.snapshot().surface_epoch(a.surface), Some(2));
    assert_eq!(g.active_count(), 1, "different surface is untouched");
}

#[test]
fn second_down_does_not_begin_duplicate_gesture() {
    let mut c = Case::new(12);
    let mut g = GestureCoordinator::new();
    assert_eq!(c.step(&mut g, 1, down(1, 20.0, Units::Logical)).len(), 1);
    assert!(c.step(&mut g, 2, down(1, 21.0, Units::Logical)).is_empty());
    assert_eq!(g.active_count(), 1);
}

mod axis_lock {
    use blobin_ui_core::gestures::{AxisLock, DragAxis, Translation};

    fn t(x: f32, y: f32) -> Translation {
        Translation { x, y }
    }

    #[test]
    fn jitter_inside_slop_projects_to_zero_and_stays_unlocked() {
        let mut lock = AxisLock::new(6.0);
        assert_eq!(lock.project(t(3.0, -4.0)), t(0.0, 0.0));
        assert_eq!(lock.locked(), None);
    }

    #[test]
    fn locks_on_dominant_axis_and_keeps_it() {
        let mut lock = AxisLock::new(6.0);
        assert_eq!(lock.project(t(9.0, 3.0)), t(9.0, 0.0));
        assert_eq!(lock.locked(), Some(DragAxis::Horizontal));
        // A later mostly-vertical delta stays horizontal.
        assert_eq!(lock.project(t(2.0, 40.0)), t(2.0, 0.0));
        lock.reset();
        assert_eq!(lock.project(t(-2.0, -30.0)), t(0.0, -30.0));
        assert_eq!(lock.locked(), Some(DragAxis::Vertical));
    }

    #[test]
    fn ties_lock_horizontally_and_bad_slop_is_zero() {
        let mut lock = AxisLock::new(f32::NAN);
        assert_eq!(lock.project(t(5.0, 5.0)), t(5.0, 0.0));
        let mut negative = AxisLock::new(-3.0);
        assert_eq!(negative.project(t(0.0, 0.5)), t(0.0, 0.5));
    }
}
