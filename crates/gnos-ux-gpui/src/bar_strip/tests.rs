//! The strip riding the travelling Bar, over every route on three outputs.

use super::*;
use gnos_ux_surface::grammar::{BarPlacementTransition, SurfaceViewport, UnitInterval};

const E: [SurfaceEdge; 4] = [
    SurfaceEdge::Left,
    SurfaceEdge::Top,
    SurfaceEdge::Right,
    SurfaceEdge::Bottom,
];
const OUTPUTS: [(f32, f32); 3] = [(1280.0, 800.0), (800.0, 1280.0), (2560.0, 1080.0)];
const STEPS: usize = 200;

fn form(entry: IdleBarEntry, vertical: f32, horizontal: f32) -> ModuleForm {
    ModuleForm {
        entry,
        vertical,
        horizontal,
    }
}

fn forms() -> Vec<ModuleForm> {
    use IdleBarEntry as B;
    vec![
        form(B::Logo, 19.0, 26.0),
        // Short enough to fit a horizontal Bar 800 px wide.
        form(B::Workspaces, 150.0, 140.0),
        form(B::ActiveWindow, 120.0, 160.0),
        form(B::Tray, 60.0, 70.0),
        form(B::Clock, 60.0, 98.0),
        form(B::StatusIcons, 110.0, 110.0),
        form(B::Power, 24.0, 24.0),
    ]
}

fn layout(forms: &[ModuleForm], length: f32, horizontal: bool) -> StripLayout {
    canonical_layout(
        [&forms[0..2], &forms[2..3], &forms[3..7]],
        length,
        horizontal,
    )
}

fn span(edge: SurfaceEdge, (w, h): (f32, f32)) -> f32 {
    if horizontal(edge) { w } else { h }
}

struct Route {
    plan: BarPlacementTransition,
    route: Vec<SurfaceEdge>,
    source: StripLayout,
    target: StripLayout,
}

fn route(viewport: (f32, f32), a: SurfaceEdge, b: SurfaceEdge, forms: &[ModuleForm]) -> Route {
    let vp = SurfaceViewport::try_new(viewport.0, viewport.1).unwrap();
    let place = |edge| {
        crate::reference_bar::native_bar_placement(vp, edge)
            .unwrap()
            .resolve(vp)
            .unwrap()
    };
    let plan = BarPlacementTransition::new(vp, place(a), place(b)).unwrap();
    Route {
        route: plan.route(),
        source: layout(forms, span(a, viewport), horizontal(a)),
        target: layout(forms, span(b, viewport), horizontal(b)),
        plan,
    }
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.width / 2.0, r.y + r.height / 2.0)
}

/// Places every module at every step of every route; calls `check` with
/// the step, the frame and the placements.
fn sweep(mut check: impl FnMut(&Route, usize, &ResolvedBarTransitionFrame, &[ModulePlacement])) {
    let forms = forms();
    for viewport in OUTPUTS {
        for a in E {
            for b in E {
                if a == b {
                    continue;
                }
                let r = route(viewport, a, b, &forms);
                let conveyor = Conveyor {
                    viewport,
                    route: &r.route,
                    forms: &forms,
                    source: &r.source,
                    target: &r.target,
                    inner: crate::reference_bar::BAR_INNER_WIDTH,
                };
                for step in 0..=STEPS {
                    let p = UnitInterval::clamped(step as f32 / STEPS as f32);
                    let frame = r.plan.evaluate(p).unwrap();
                    let placed = conveyor.place(&frame);
                    check(&r, step, &frame, &placed);
                }
            }
        }
    }
}

#[test]
fn every_module_stays_on_the_material_at_every_step() {
    sweep(|r, step, frame, placed| {
        assert_eq!(placed.len(), 7, "{:?} step {step}", r.route);
        for m in placed {
            assert!(
                m.clip.width > 0.5 && m.clip.height > 0.5,
                "{:?} step {step} {:?} off the material: {:?} arms {:?}",
                r.route,
                m.entry,
                m.rect,
                frame.arms
            );
            let inside = frame.arms.iter().any(|arm| {
                let b = arm.bounds;
                m.clip.x >= b.left() - 1.0
                    && m.clip.y >= b.top() - 1.0
                    && m.clip.x + m.clip.width <= b.right() + 1.0
                    && m.clip.y + m.clip.height <= b.bottom() + 1.0
            });
            assert!(inside, "{:?} step {step} {:?}", r.route, m.entry);
        }
    });
}

#[test]
fn the_ends_are_the_rest_layouts() {
    let forms = forms();
    sweep(|r, step, frame, placed| {
        if step != 0 && step != STEPS {
            return;
        }
        let (strip, edge) = if step == 0 {
            (&r.source, r.route[0])
        } else {
            (&r.target, *r.route.last().unwrap())
        };
        for (entry, c) in &strip.centres {
            let m = placed.iter().find(|m| m.entry == *entry).unwrap();
            let along = if horizontal(edge) {
                centre(m.rect).0
            } else {
                centre(m.rect).1
            };
            assert!(
                (along - c).abs() < 0.01,
                "{:?} {entry:?} {along} vs {c}",
                r.route
            );
            let f = forms.iter().find(|f| f.entry == *entry).unwrap();
            assert_eq!(m.horizontal, horizontal(edge));
            let len = if horizontal(edge) {
                m.rect.width
            } else {
                m.rect.height
            };
            assert!((len - f.along(horizontal(edge))).abs() < 0.01);
        }
        let _ = frame;
    });
}

#[test]
fn modules_move_continuously() {
    let mut worst = 0.0f32;
    let mut previous: Vec<ModulePlacement> = Vec::new();
    sweep(|r, step, _frame, placed| {
        if step > 0 {
            let w = r.source.length.max(r.target.length);
            // A step is 1/200 of the leg; the material crosses about two
            // edges per leg, so a module never moves more than a few
            // percent of the longer edge in one step.
            let bound = 0.06 * w + crate::rail_topology::RAIL_WIDTH / 2.0;
            for m in placed {
                let before = previous.iter().find(|p| p.entry == m.entry).unwrap();
                let (a, b) = (centre(before.rect), centre(m.rect));
                let jump = (a.0 - b.0).hypot(a.1 - b.1);
                let grow = (before.rect.width - m.rect.width)
                    .abs()
                    .max((before.rect.height - m.rect.height).abs());
                worst = worst.max(jump / bound);
                assert!(
                    jump <= bound && grow <= bound,
                    "{:?} step {step} {:?} jumped {jump:.1} (grew {grow:.1}), bound {bound:.1}",
                    r.route,
                    m.entry
                );
            }
        }
        previous = placed.to_vec();
    });
    eprintln!("worst jump / bound = {worst:.3}");
}

#[test]
fn the_order_crosses_only_on_routes_that_reverse_it() {
    use IdleBarEntry as B;
    let expect = |a, b| -> bool {
        matches!(
            (a, b),
            (SurfaceEdge::Left, SurfaceEdge::Top)
                | (SurfaceEdge::Top, SurfaceEdge::Left)
                | (SurfaceEdge::Right, SurfaceEdge::Bottom)
                | (SurfaceEdge::Bottom, SurfaceEdge::Right)
                | (SurfaceEdge::Left, SurfaceEdge::Right)
                | (SurfaceEdge::Right, SurfaceEdge::Left)
                | (SurfaceEdge::Top, SurfaceEdge::Bottom)
                | (SurfaceEdge::Bottom, SurfaceEdge::Top)
        )
    };
    let mut seen_slide = 0;
    sweep(|r, _step, frame, placed| {
        let (a, b) = (r.route[0], *r.route.last().unwrap());
        assert_eq!(route_reverses(&r.route), expect(a, b), "{:?}", r.route);
        // At the final slide's start the conveyor has carried the strip in
        // its own order: canonical unless the route reverses it.
        let first_of_slide = frame.phase == BarPlacementTransitionPhase::TargetSlide
            && frame.segment.is_some_and(|s| s.progress.value() < 0.05);
        if !first_of_slide {
            return;
        }
        seen_slide += 1;
        let along = |e: B| {
            let m = placed.iter().find(|m| m.entry == e).unwrap();
            let c = centre(m.rect);
            if horizontal(b) { c.0 } else { c.1 }
        };
        let logo_first = along(B::Logo) < along(B::Power);
        assert_eq!(logo_first, !route_reverses(&r.route), "{:?}", r.route);
    });
    assert!(seen_slide > 0);
}

#[test]
fn a_pointer_on_a_travelling_module_lands_on_the_same_module_at_rest() {
    use IdleBarEntry as B;
    let placed = |entry, x, y, w, h, row| ModulePlacement {
        entry,
        rect: Rect::new(x, y, w, h),
        clip: Rect::new(x, y, w, h),
        horizontal: row,
    };
    // The clock rides a top arm as a row; at rest it is a column on the left.
    let now = [placed(B::Clock, 400.0, 10.0, 98.0, 40.0, true)];
    let rest = [placed(B::Clock, 10.0, 600.0, 40.0, 105.0, false)];
    let (x, y) = map_through(&now, &rest, 400.0 + 49.0, 30.0).unwrap();
    assert_eq!(x, 30.0);
    assert!((y - (600.0 + 52.5)).abs() < 1e-3, "half way along: {y}");
    assert!(
        map_through(&now, &rest, 10.0, 10.0).is_none(),
        "on no module"
    );
}

#[test]
fn a_carried_module_lands_in_the_nearest_slot_of_its_group() {
    use crate::reference_bar::BarModuleOrder;
    use IdleBarEntry as B;
    let order = BarModuleOrder::default();
    // A vertical rail 800 long; the clock is carried, the others at rest.
    let without = crate::bar_strip::moved(&order, B::Clock, None);
    let forms = forms();
    let rest = {
        let split: [Vec<ModuleForm>; 3] = without.0.clone().map(|g| {
            g.iter()
                .filter_map(|e| forms.iter().find(|f| f.entry == *e).copied())
                .collect()
        });
        canonical_layout([&split[0], &split[1], &split[2]], 800.0, false)
    };
    let others: Vec<ModulePlacement> = rest
        .centres
        .iter()
        .map(|(e, c)| {
            let f = forms.iter().find(|f| f.entry == *e).unwrap();
            ModulePlacement {
                entry: *e,
                rect: Rect::new(10.0, c - f.vertical / 2.0, 40.0, f.vertical),
                clip: Rect::new(10.0, c - f.vertical / 2.0, 40.0, f.vertical),
                horizontal: false,
            }
        })
        .collect();
    // Just after the workspaces: the start group's end.
    let ws = others
        .iter()
        .find(|m| m.entry == B::Workspaces)
        .unwrap()
        .rect;
    assert_eq!(
        insertion(
            &others,
            &without,
            false,
            ws.y + ws.height + 4.0,
            (0.0, 800.0)
        ),
        (0, 2)
    );
    // At the very bottom: after power, the end group's end.
    assert_eq!(
        insertion(&others, &without, false, 795.0, (0.0, 800.0)),
        (2, 3)
    );
    // A module the group holds but does not show (a hidden logo) still
    // counts: the slot after the workspaces is the group's index 2.
    let shown: Vec<ModulePlacement> = others
        .iter()
        .copied()
        .filter(|m| m.entry != B::Logo)
        .collect();
    assert_eq!(
        insertion(
            &shown,
            &without,
            false,
            ws.y + ws.height + 4.0,
            (0.0, 800.0)
        ),
        (0, 2)
    );
    let to = moved(&order, B::Clock, Some((0, 2)));
    assert_eq!(to.0[0], [B::Logo, B::Workspaces, B::Clock]);
    assert!(!to.0[2].contains(&B::Clock));
    let gone = moved(&order, B::Tray, None);
    assert!(gone.0.iter().all(|g| !g.contains(&B::Tray)));
}

#[test]
fn a_retargeted_travel_turns_without_a_module_jump() {
    // BAR-01J (B09): a travel interrupted at 20, 50 or 80 % by a third edge
    // turns from its live frame; the modules blend from where they were
    // painted to the rebased route over the handoff, then ride it. No step
    // moves a module further than the route itself moves them.
    let forms = forms();
    let vp_of = |(w, h): (f32, f32)| SurfaceViewport::try_new(w, h).unwrap();
    for viewport in OUTPUTS {
        let vp = vp_of(viewport);
        let place = |edge| {
            crate::reference_bar::native_bar_placement(vp, edge)
                .unwrap()
                .resolve(vp)
                .unwrap()
        };
        for a in E {
            for b in E {
                for c in E {
                    if a == b || b == c {
                        continue;
                    }
                    for at in [0.2_f32, 0.5, 0.8] {
                        let first = route(viewport, a, b, &forms);
                        let conveyor = Conveyor {
                            viewport,
                            route: &first.route,
                            forms: &forms,
                            source: &first.source,
                            target: &first.target,
                            inner: crate::reference_bar::BAR_INNER_WIDTH,
                        };
                        let origin = first.plan.evaluate(UnitInterval::clamped(at)).unwrap();
                        let painted = conveyor.place(&origin);
                        let next = BarPlacementTransition::retarget_from_frame(
                            vp,
                            &origin,
                            place(origin.active_edge),
                            place(c),
                        )
                        .unwrap();
                        let route2 = next.route();
                        let source =
                            layout(&forms, span(route2[0], viewport), horizontal(route2[0]));
                        let target = layout(&forms, span(c, viewport), horizontal(c));
                        let rebased = Conveyor {
                            viewport,
                            route: &route2,
                            forms: &forms,
                            source: &source,
                            target: &target,
                            inner: crate::reference_bar::BAR_INNER_WIDTH,
                        };
                        let mut last = painted.clone();
                        let mut worst = 0.0_f32;
                        for step in 0..=STEPS {
                            let p = step as f32 / STEPS as f32;
                            let frame = next.evaluate(UnitInterval::clamped(p)).unwrap();
                            let mut placed = rebased.place(&frame);
                            let rect = |g: gnos_ux_surface::grammar::GeometryRect| {
                                Rect::new(g.left(), g.top(), g.width(), g.height())
                            };
                            if frame.phase == BarPlacementTransitionPhase::RetargetHandoff {
                                let t = (p / gnos_ux_surface::grammar::RETARGET_HANDOFF)
                                    .clamp(0.0, 1.0);
                                let arms: Vec<Rect> =
                                    frame.arms.iter().map(|a| rect(a.bounds)).collect();
                                let other = origin
                                    .arms
                                    .iter()
                                    .find(|a| a.edge != origin.active_edge)
                                    .map(|a| rect(a.bounds));
                                placed = handoff_blend(
                                    &painted,
                                    &placed,
                                    t * t * (3.0 - 2.0 * t),
                                    rect(origin.active_allocation),
                                    other,
                                    &arms,
                                );
                            }
                            assert_eq!(
                                placed.len(),
                                7,
                                "{a:?}->{b:?} at {at} then {c:?}: step {step}"
                            );
                            for m in &placed {
                                assert!(
                                    m.clip.width > 0.5 && m.clip.height > 0.5,
                                    "{a:?}->{b:?} at {at} then {c:?} on {viewport:?}: step {step} {:?} off the material {:?}",
                                    m.entry,
                                    m.rect
                                );
                            }
                            for m in &placed {
                                let before = last.iter().find(|l| l.entry == m.entry).unwrap();
                                let ((x0, y0), (x1, y1)) = (centre(before.rect), centre(m.rect));
                                worst = worst.max(((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt());
                            }
                            last = placed;
                        }
                        // The same bound the plain routes are held to: 6 % of
                        // the longer edge per 1/200 of a leg, plus the form's
                        // turn.
                        let bound = 0.06 * viewport.0.max(viewport.1) + 30.0;
                        assert!(
                            worst <= bound,
                            "{a:?}->{b:?} at {at} then {c:?} on {viewport:?}: a module moved {worst:.1} px in one step"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn modules_give_way_in_the_operators_order_until_the_bar_fits() {
    use crate::reference_bar::BarModuleOrder;
    let order = BarModuleOrder::default();
    let forms: Vec<ModuleForm> = order
        .entries()
        .into_iter()
        .filter(|e| *e != IdleBarEntry::Spacer)
        .map(|entry| ModuleForm {
            entry,
            vertical: 40.0,
            horizontal: if entry == IdleBarEntry::ActiveWindow {
                300.0
            } else {
                80.0
            },
        })
        .collect();
    let all = fixed_length([&forms[..2], &forms[2..3], &forms[3..]], true);
    let give_way = [
        IdleBarEntry::ActiveWindow,
        IdleBarEntry::Tray,
        IdleBarEntry::Clock,
    ];
    // Room for everything: nothing leaves.
    let (kept, hidden) = fitted_order(&order, &forms, all + 1.0, true, &give_way);
    assert_eq!((kept, hidden.len()), (order.clone(), 0));
    // A little short: only the first to give way leaves.
    let (kept, hidden) = fitted_order(&order, &forms, all - 10.0, true, &give_way);
    assert_eq!(hidden, vec![IdleBarEntry::ActiveWindow]);
    assert_eq!(kept.group(IdleBarEntry::ActiveWindow), None);
    assert_eq!(kept.group(IdleBarEntry::Clock), Some(2));
    // Much shorter: the next ones follow, in order, and no more than needed.
    let (_, hidden) = fitted_order(&order, &forms, all - 320.0, true, &give_way);
    assert_eq!(hidden, vec![IdleBarEntry::ActiveWindow, IdleBarEntry::Tray]);
    // Vertical: the column forms count, and they fit.
    let (_, hidden) = fitted_order(&order, &forms, 800.0, false, &give_way);
    assert!(hidden.is_empty());
}

#[test]
fn a_reversing_route_keeps_the_order_it_arrives_in() {
    use crate::reference_bar::{BarModuleOrder, IdleBarEntry as E};
    let order = BarModuleOrder([
        vec![E::Logo, E::Workspaces],
        vec![E::ActiveWindow],
        vec![E::Clock, E::Power],
    ]);
    let m = super::mirrored(&order);
    assert_eq!(
        m.0,
        [
            vec![E::Power, E::Clock],
            vec![E::ActiveWindow],
            vec![E::Workspaces, E::Logo],
        ]
    );
    assert_eq!(
        super::mirrored(&m),
        order,
        "mirroring twice is the identity"
    );
}
