use blobin_ui_core::input::{
    EventKind, HoverTransition, InputError, InputEvent, InputRouter, Key, Point, ScrollDelta,
    ScrollSource, Units,
};
use blobin_ui_core::layout::{
    compute, Axis, LayoutSnapshot, LayoutStyles, Length, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};

struct Case {
    tree: UiTree,
    style: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    surface: SurfaceId,
    root: NodeId,
    a: NodeId,
    b: NodeId,
    router: InputRouter,
}
fn fixture() -> Case {
    let mut tree = UiTree::new();
    let surface = SurfaceId(11);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        let root = tx.append_root(surface, NodeProperties::new(Role::Container, "root"))?;
        let a = tx.append_child(root, NodeProperties::new(Role::Button, "A"))?;
        let b = tx.append_child(root, NodeProperties::new(Role::Button, "B"))?;
        ids = vec![root, a, b];
        Ok(())
    })
    .unwrap();
    let mut style = LayoutStyles::new();
    let mut root = Style::fill();
    root.axis = Axis::Row;
    root.clip_children = true;
    style.set(&tree.snapshot(), ids[0], root).unwrap();
    for id in [ids[1], ids[2]] {
        let node = Style {
            width: Length::Px(100.0),
            height: Length::Px(60.0),
            ..Style::default()
        };
        style.set(&tree.snapshot(), id, node).unwrap();
    }
    let viewport = Viewport::new(Size::new(240.0, 90.0), 1.5, 7);
    let layout = compute(&tree.snapshot(), &style, surface, viewport).unwrap();
    Case {
        tree,
        style,
        layout,
        viewport,
        surface,
        root: ids[0],
        a: ids[1],
        b: ids[2],
        router: InputRouter::new(),
    }
}
fn pt(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn down(x: f32, y: f32) -> EventKind {
    EventKind::PointerDown {
        pointer: 1,
        button: 1,
        position: pt(x, y),
        units: Units::Logical,
    }
}
fn move_to(x: f32, y: f32) -> EventKind {
    EventKind::PointerMove {
        pointer: 1,
        position: pt(x, y),
        units: Units::Logical,
    }
}
fn up(x: f32, y: f32) -> EventKind {
    EventKind::PointerUp {
        pointer: 1,
        button: 1,
        position: pt(x, y),
        units: Units::Logical,
    }
}
impl Case {
    fn send(
        &mut self,
        serial: u64,
        kind: EventKind,
    ) -> Result<blobin_ui_core::input::InputReceipt, InputError> {
        let ev = InputEvent::for_layout(&self.layout, serial, serial * 1000, kind);
        self.router.dispatch(
            &self.tree.snapshot(),
            &self.style,
            &self.layout,
            self.viewport,
            ev,
        )
    }
}

#[test]
fn clipped_reverse_paint_hit_and_half_open_edges() {
    let c = fixture();
    let snap = c.tree.snapshot();
    let hit = |x, y| {
        InputRouter::hit_test(&snap, &c.style, &c.layout, c.viewport, pt(x, y))
            .unwrap()
            .map(|route| route.target)
    };
    assert_eq!(hit(0.0, 0.0), Some(c.a));
    assert_eq!(hit(99.999, 30.0), Some(c.a));
    assert_eq!(hit(100.0, 30.0), Some(c.b));
    assert_eq!(hit(200.0, 30.0), None);
    assert_eq!(hit(-1.0, 30.0), None);
    assert_eq!(hit(f32::NAN, 0.0), None);
}
#[test]
fn exactly_one_activation_per_completed_physical_click() {
    let mut c = fixture();
    let press = c.send(1, down(25.0, 30.0)).unwrap();
    assert_eq!(press.route.unwrap().target, c.a);
    assert_eq!(press.pointer_capture, Some(c.a));
    assert_eq!(press.activated, None);
    assert_eq!(c.router.focus(c.surface), Some(c.a));
    let release = c.send(2, up(25.0, 30.0)).unwrap();
    assert_eq!(release.activated, Some(c.a));
    assert_eq!(c.router.capture(c.surface, 1), None);
    assert_eq!(
        c.send(2, up(25.0, 30.0)).unwrap_err(),
        InputError::NonMonotonicSerial
    );
    assert_eq!(c.send(3, up(25.0, 30.0)).unwrap().activated, None);
}
#[test]
fn physical_pixels_normalize_using_dpr() {
    let mut c = fixture();
    let event = EventKind::PointerDown {
        pointer: 1,
        button: 1,
        position: pt(225.0, 45.0),
        units: Units::Physical,
    };
    assert_eq!(c.send(1, event).unwrap().route.unwrap().target, c.b);
}
#[test]
fn drag_capture_outside_bounds_and_release_outside_does_not_click() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let drag = c.send(2, move_to(900.0, 900.0)).unwrap();
    assert_eq!(drag.route.unwrap().target, c.a);
    assert_eq!(drag.pointer_capture, Some(c.a));
    let up_outside = c.send(3, up(900.0, 900.0)).unwrap();
    assert_eq!(up_outside.route.unwrap().target, c.a);
    assert_eq!(up_outside.activated, None);
    assert_eq!(c.router.capture(c.surface, 1), None);
}

#[test]
fn wrong_button_does_not_release_capture_or_activate() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let wrong = EventKind::PointerUp {
        pointer: 1,
        button: 2,
        position: pt(10.0, 10.0),
        units: Units::Logical,
    };
    let receipt = c.send(2, wrong).unwrap();
    assert_eq!(receipt.pointer_capture, Some(c.a));
    assert_eq!(receipt.activated, None);
    assert_eq!(c.send(3, up(10.0, 10.0)).unwrap().activated, Some(c.a));
}
#[test]
fn pointer_cancel_and_blur_never_activate() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    c.send(2, EventKind::PointerCancel { pointer: 1 }).unwrap();
    assert_eq!(c.router.capture(c.surface, 1), None);
    assert_eq!(c.send(3, up(10.0, 10.0)).unwrap().activated, None);
    c.send(4, down(110.0, 10.0)).unwrap();
    c.send(5, EventKind::WindowBlur).unwrap();
    assert_eq!(c.router.capture(c.surface, 1), None);
    assert_eq!(c.router.focus(c.surface), None);
}
#[test]
fn tab_navigation_reverse_and_enter_repeat_dedup() {
    let mut c = fixture();
    let tab = |shift| EventKind::KeyDown {
        key: Key::Tab,
        shift,
        repeat: false,
    };
    assert_eq!(c.send(1, tab(false)).unwrap().focus, Some(c.a));
    // Holding Tab cannot traverse again until KeyUp.
    assert_eq!(c.send(2, tab(false)).unwrap().focus, Some(c.a));
    c.send(3, EventKind::KeyUp { key: Key::Tab }).unwrap();
    assert_eq!(c.send(4, tab(false)).unwrap().focus, Some(c.b));
    c.send(5, EventKind::KeyUp { key: Key::Tab }).unwrap();
    assert_eq!(c.send(6, tab(true)).unwrap().focus, Some(c.a));
    c.send(7, EventKind::KeyUp { key: Key::Tab }).unwrap();
    let enter = EventKind::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    };
    assert_eq!(c.send(8, enter.clone()).unwrap().activated, Some(c.a));
    assert_eq!(c.send(9, enter.clone()).unwrap().activated, None);
    c.send(10, EventKind::KeyUp { key: Key::Enter }).unwrap();
    assert_eq!(c.send(11, enter).unwrap().activated, Some(c.a));
}
#[test]
fn escape_cancels_capture_but_preserves_focus() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let esc = EventKind::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    };
    assert_eq!(c.send(2, esc).unwrap().focus, Some(c.a));
    assert_eq!(c.router.capture(c.surface, 1), None);
    assert_eq!(c.send(3, up(10.0, 10.0)).unwrap().activated, None);
}
#[test]
fn duplicate_serial_and_timestamp_regression_rejected_without_state_change() {
    let mut c = fixture();
    c.send(5, down(10.0, 10.0)).unwrap();
    let before = c.router.capture(c.surface, 1);
    assert_eq!(
        c.send(5, up(10.0, 10.0)).unwrap_err(),
        InputError::NonMonotonicSerial
    );
    let old = InputEvent::for_layout(&c.layout, 6, 1, up(10.0, 10.0));
    assert_eq!(
        c.router
            .dispatch(&c.tree.snapshot(), &c.style, &c.layout, c.viewport, old)
            .unwrap_err(),
        InputError::NonMonotonicTimestamp
    );
    assert_eq!(c.router.capture(c.surface, 1), before);
    assert_eq!(c.send(6, up(10.0, 10.0)).unwrap().activated, Some(c.a));
}
#[test]
fn stale_resize_and_scale_event_rejected_without_capture_mutation() {
    let mut c = fixture();
    let old = InputEvent::for_layout(&c.layout, 1, 100, down(10.0, 10.0));
    let mut newer = c.viewport;
    newer.geometry_epoch += 1;
    assert_eq!(
        c.router
            .dispatch(&c.tree.snapshot(), &c.style, &c.layout, newer, old.clone())
            .unwrap_err(),
        InputError::StaleLayout
    );
    assert_eq!(c.router.capture(c.surface, 1), None);
    let mut wrong = old;
    wrong.scale_factor_bits = 1.0_f32.to_bits();
    assert_eq!(
        c.router
            .dispatch(&c.tree.snapshot(), &c.style, &c.layout, c.viewport, wrong)
            .unwrap_err(),
        InputError::StaleEvent
    );
    assert_eq!(
        c.send(1, down(10.0, 10.0)).unwrap().pointer_capture,
        Some(c.a)
    );
}
#[test]
fn invalid_pointer_nan_rejected_without_advancing_event_serial() {
    let mut c = fixture();
    assert_eq!(
        c.send(1, down(f32::NAN, 0.0)).unwrap_err(),
        InputError::InvalidPosition
    );
    assert_eq!(
        c.send(1, down(10.0, 10.0)).unwrap().route.unwrap().target,
        c.a
    );
}

#[test]
fn deleted_node_and_reused_slot_cannot_steal_focus() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    c.tree
        .transact(|tx| {
            tx.remove(c.a)?;
            Ok(())
        })
        .unwrap();
    let old = c.a;
    let mut replacement = None;
    c.tree
        .transact(|tx| {
            replacement = Some(tx.append_child(c.root, NodeProperties::new(Role::Button, "new"))?);
            Ok(())
        })
        .unwrap();
    let replacement = replacement.unwrap();
    assert_ne!(old, replacement);
    let st = Style {
        width: Length::Px(100.0),
        height: Length::Px(60.0),
        ..Style::default()
    };
    c.style.set(&c.tree.snapshot(), replacement, st).unwrap();
    c.layout = compute(&c.tree.snapshot(), &c.style, c.surface, c.viewport).unwrap();
    assert!(c.router.capture(c.surface, 1) == Some(old));
    let receipt = c.send(2, move_to(150.0, 30.0)).unwrap();
    assert_eq!(receipt.pointer_capture, None);
    assert_eq!(receipt.focus, None);
    assert_ne!(receipt.route.unwrap().target, old);
}
#[test]
fn disabled_buttons_are_skipped_for_pointer_and_keyboard() {
    let mut c = fixture();
    let mut disabled = NodeProperties::new(Role::Button, "A");
    disabled.enabled = false;
    c.tree
        .transact(|tx| tx.set_properties(c.a, disabled))
        .unwrap();
    c.layout = compute(&c.tree.snapshot(), &c.style, c.surface, c.viewport).unwrap();
    assert!(c.send(1, down(10.0, 10.0)).unwrap().route.is_none());
    let tab = EventKind::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    };
    assert_eq!(c.send(2, tab).unwrap().focus, Some(c.b));
}
#[test]
fn surface_epochs_reject_old_events_and_old_captures() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let prior_event = InputEvent::for_layout(&c.layout, 2, 2_000, up(10.0, 10.0));
    c.tree.transact(|tx| tx.close_surface(c.surface)).unwrap();
    c.tree
        .transact(|tx| {
            tx.open_surface(c.surface)?;
            let root =
                tx.append_root(c.surface, NodeProperties::new(Role::Container, "new root"))?;
            tx.append_child(root, NodeProperties::new(Role::Button, "new button"))?;
            Ok(())
        })
        .unwrap();
    c.layout = compute(&c.tree.snapshot(), &c.style, c.surface, c.viewport).unwrap();
    assert_eq!(
        c.router
            .dispatch(
                &c.tree.snapshot(),
                &c.style,
                &c.layout,
                c.viewport,
                prior_event
            )
            .unwrap_err(),
        InputError::StaleEvent
    );
    let receipt = c.send(1, EventKind::WindowBlur).unwrap();
    assert_eq!(receipt.focus, None);
    assert_eq!(receipt.pointer_capture, None);
}
#[test]
fn capture_and_focus_are_per_surface() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let other = SurfaceId(23);
    c.tree
        .transact(|tx| {
            tx.open_surface(other)?;
            tx.append_root(other, NodeProperties::new(Role::Button, "window two"))?;
            Ok(())
        })
        .unwrap();
    let other_layout = compute(&c.tree.snapshot(), &c.style, other, c.viewport).unwrap();
    let event = InputEvent::for_layout(
        &other_layout,
        1,
        1_000,
        EventKind::KeyDown {
            key: Key::Tab,
            shift: false,
            repeat: false,
        },
    );
    let second = c
        .router
        .dispatch(
            &c.tree.snapshot(),
            &c.style,
            &other_layout,
            c.viewport,
            event,
        )
        .unwrap();
    assert!(second.focus.is_some());
    assert_eq!(c.router.focus(c.surface), Some(c.a));
    assert_eq!(c.router.capture(c.surface, 1), Some(c.a));
    assert_eq!(c.router.capture(other, 1), None);
    c.router.close_surface(c.surface);
    assert_eq!(c.router.focus(c.surface), None);
    assert!(c.router.focus(other).is_some());
}
#[test]
fn checked_hit_test_rejects_stale_resize_without_routing() {
    let c = fixture();
    let new_view = Viewport::new(Size::new(241.0, 90.0), 1.5, 8);
    assert_eq!(
        InputRouter::hit_test(
            &c.tree.snapshot(),
            &c.style,
            &c.layout,
            new_view,
            pt(10.0, 10.0)
        )
        .unwrap_err(),
        InputError::StaleLayout
    );
}
#[test]
fn capture_and_bubble_route_include_correct_ancestors() {
    let mut c = fixture();
    let mut nested = None;
    c.tree
        .transact(|tx| {
            nested = Some(tx.append_child(c.a, NodeProperties::new(Role::Button, "nested"))?);
            Ok(())
        })
        .unwrap();
    let nested = nested.unwrap();
    c.style
        .set(
            &c.tree.snapshot(),
            nested,
            Style {
                width: Length::Px(40.0),
                height: Length::Px(30.0),
                ..Style::default()
            },
        )
        .unwrap();
    c.layout = compute(&c.tree.snapshot(), &c.style, c.surface, c.viewport).unwrap();
    let click = c.send(1, down(10.0, 10.0)).unwrap();
    let route = click.route.unwrap();
    assert_eq!(route.target, nested);
    assert_eq!(route.capture_path, vec![c.root, c.a]);
    assert_eq!(route.bubble_path, vec![c.a, c.root]);
}
#[test]
fn nested_clip_prevents_hit_outside_clipped_ancestor() {
    let mut c = fixture();
    let mut nested = None;
    c.tree
        .transact(|tx| {
            nested = Some(tx.append_child(c.a, NodeProperties::new(Role::Button, "overflow"))?);
            Ok(())
        })
        .unwrap();
    let nested = nested.unwrap();
    c.style
        .set(
            &c.tree.snapshot(),
            nested,
            Style {
                width: Length::Px(150.0),
                height: Length::Px(60.0),
                ..Style::default()
            },
        )
        .unwrap();
    let parent = Style {
        width: Length::Px(100.0),
        height: Length::Px(60.0),
        clip_children: true,
        ..Style::default()
    };
    c.style.set(&c.tree.snapshot(), c.a, parent).unwrap();
    c.layout = compute(&c.tree.snapshot(), &c.style, c.surface, c.viewport).unwrap();
    assert_eq!(c.layout.get(nested).unwrap().rect.width, 150.0);
    assert_eq!(c.layout.get(nested).unwrap().visible.width, 100.0);
    assert_eq!(
        c.send(1, down(110.0, 20.0)).unwrap().route.unwrap().target,
        c.b
    );
}
#[test]
fn overlapping_nodes_route_to_last_painted_target() {
    let mut tree = UiTree::new();
    let s = SurfaceId(3);
    let mut nodes = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(s)?;
        let root = tx.append_root(s, NodeProperties::new(Role::Container, "root"))?;
        let zero = tx.append_child(root, NodeProperties::new(Role::Container, "zero"))?;
        let upper = tx.append_child(zero, NodeProperties::new(Role::Button, "under"))?;
        let front = tx.append_child(root, NodeProperties::new(Role::Button, "front"))?;
        nodes = vec![root, zero, upper, front];
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    let mut root = Style::fill();
    root.axis = Axis::Row;
    styles.set(&tree.snapshot(), nodes[0], root).unwrap();
    styles
        .set(
            &tree.snapshot(),
            nodes[1],
            Style {
                width: Length::Px(0.0),
                height: Length::Px(60.0),
                ..Style::default()
            },
        )
        .unwrap();
    for id in [nodes[2], nodes[3]] {
        styles
            .set(
                &tree.snapshot(),
                id,
                Style {
                    width: Length::Px(100.0),
                    height: Length::Px(60.0),
                    ..Style::default()
                },
            )
            .unwrap();
    }
    let vp = Viewport::new(Size::new(200.0, 100.0), 1.0, 1);
    let layout = compute(&tree.snapshot(), &styles, s, vp).unwrap();
    assert_eq!(layout.get(nodes[2]).unwrap().rect.x, 0.0);
    assert_eq!(layout.get(nodes[3]).unwrap().rect.x, 0.0);
    let hit = InputRouter::hit_test(&tree.snapshot(), &styles, &layout, vp, pt(10.0, 10.0))
        .unwrap()
        .unwrap();
    assert_eq!(hit.target, nodes[3]);
}

fn scroll(x: f32, y: f32, dy: f32) -> EventKind {
    EventKind::Scroll {
        pointer: 1,
        position: pt(x, y),
        units: Units::Logical,
        delta: ScrollDelta { x: 0.0, y: dy },
        source: ScrollSource::Wheel,
    }
}
#[test]
fn hover_reports_only_transitions_and_leave_clears_it() {
    let mut c = fixture();
    let enter = c.send(1, move_to(10.0, 10.0)).unwrap();
    assert_eq!(
        enter.hover,
        Some(HoverTransition {
            pointer: 1,
            from: None,
            to: Some(c.a)
        })
    );
    assert_eq!(c.send(2, move_to(20.0, 10.0)).unwrap().hover, None);
    let cross = c.send(3, move_to(150.0, 10.0)).unwrap();
    assert_eq!(
        cross.hover,
        Some(HoverTransition {
            pointer: 1,
            from: Some(c.a),
            to: Some(c.b)
        })
    );
    assert_eq!(c.router.hover(c.surface, 1), Some(c.b));
    let leave = c.send(4, EventKind::PointerLeave { pointer: 1 }).unwrap();
    assert_eq!(
        leave.hover,
        Some(HoverTransition {
            pointer: 1,
            from: Some(c.b),
            to: None
        })
    );
    assert_eq!(leave.route, None);
    assert_eq!(c.router.hover(c.surface, 1), None);
    assert_eq!(
        c.send(5, EventKind::PointerLeave { pointer: 1 })
            .unwrap()
            .hover,
        None
    );
}
#[test]
fn hover_follows_geometry_during_capture_and_leave_keeps_capture() {
    let mut c = fixture();
    c.send(1, down(10.0, 10.0)).unwrap();
    let dragged = c.send(2, move_to(150.0, 10.0)).unwrap();
    assert_eq!(dragged.route.unwrap().target, c.a);
    assert_eq!(dragged.hover.unwrap().to, Some(c.b));
    c.send(3, EventKind::PointerLeave { pointer: 1 }).unwrap();
    assert_eq!(c.router.capture(c.surface, 1), Some(c.a));
    assert_eq!(c.router.hover(c.surface, 1), None);
}
#[test]
fn scroll_routes_without_focus_or_activation_and_honours_capture() {
    let mut c = fixture();
    let wheel = c.send(1, scroll(150.0, 10.0, 15.0)).unwrap();
    assert_eq!(wheel.route.unwrap().target, c.b);
    assert_eq!(wheel.activated, None);
    assert_eq!(wheel.focus, None);
    assert_eq!(wheel.hover.unwrap().to, Some(c.b));
    assert_eq!(c.send(2, scroll(220.0, 10.0, 15.0)).unwrap().route, None);
    c.send(3, down(10.0, 10.0)).unwrap();
    let held = c.send(4, scroll(150.0, 10.0, -15.0)).unwrap();
    assert_eq!(held.route.unwrap().target, c.a);
    assert_eq!(held.pointer_capture, Some(c.a));
}
#[test]
fn non_finite_or_absurd_scroll_is_rejected_without_consuming_serial() {
    let mut c = fixture();
    for dy in [f32::NAN, f32::INFINITY, 2_000_000.0] {
        assert_eq!(
            c.send(1, scroll(10.0, 10.0, dy)).unwrap_err(),
            InputError::InvalidScroll
        );
    }
    assert!(c.send(1, scroll(10.0, 10.0, 1.0)).is_ok());
}
