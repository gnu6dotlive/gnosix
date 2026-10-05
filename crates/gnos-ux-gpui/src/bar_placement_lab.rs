//! Development-only frozen endpoints. Uses production geometry, painter and content.
use crate::{
    lab_topology_view::{ComparisonContent, content_fixture},
    product_design::{ProductControlFocus, installed_application_item},
    product_focus::traversable_focus_handle,
    product_scene::{ProductHit, ProductLayout},
    product_shared_envelope::{SharedDockEnvelope, SharedMotion},
};
use gnos_ux_surface::{
    envelope::SurfaceEdge,
    grammar::{
        BarPlacementTransition, GeometryRect, ResolvedBarPlacement, SurfaceViewport, UnitInterval,
    },
    scene::{MonitorSceneId, SceneEpoch},
};
use gpui::{
    AnyElement, Context, FocusHandle, Render, Window, canvas, div, point, prelude::*, px, rgba,
};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureState {
    Closed,
    Workspace,
    AppStack,
}
pub struct BarPlacementLab {
    pub edge: SurfaceEdge,
    pub state: FixtureState,
    content: ComparisonContent,
    pub bar_focus: FocusHandle,
    pub dock_focus: FocusHandle,
    pub row_focus: Vec<FocusHandle>,
    ack: Option<PathBuf>,
    observed: Rc<RefCell<Option<[f32; 4]>>>,
    pub last_layout: Option<ProductLayout>,
    pub clicks: usize,
    scroll: gpui::ScrollHandle,
    warmup_frames: u8,
    transition: Option<(SurfaceEdge, UnitInterval)>,
}
fn rect(r: GeometryRect) -> [f32; 4] {
    [r.left(), r.top(), r.right(), r.bottom()]
}
impl BarPlacementLab {
    pub fn new(
        edge: SurfaceEdge,
        state: FixtureState,
        ack: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let content = content_fixture("appstack-basic").expect("static local fixture");
        let count = crate::product_appstack::appstack_rows(&content.runtime).len();
        Self {
            edge,
            state,
            content,
            bar_focus: traversable_focus_handle(cx),
            dock_focus: traversable_focus_handle(cx),
            row_focus: (0..count).map(|_| traversable_focus_handle(cx)).collect(),
            ack,
            observed: Default::default(),
            last_layout: None,
            clicks: 0,
            scroll: gpui::ScrollHandle::new(),
            warmup_frames: 0,
            transition: None,
        }
    }

    pub fn new_transition(
        source: SurfaceEdge,
        target: SurfaceEdge,
        progress: UnitInterval,
        state: FixtureState,
        ack: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut lab = Self::new(target, state, ack, cx);
        lab.transition = Some((source, progress));
        lab
    }
}
impl Render for BarPlacementLab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The debug accessibility tree describes the last completed frame.
        // Allow bounded startup redraws before freezing the capture endpoint.
        if self.ack.is_some() && self.warmup_frames < 3 {
            self.warmup_frames += 1;
            window.request_animation_frame();
        }
        let size = window.viewport_size();
        let viewport = SurfaceViewport::try_new(f32::from(size.width), f32::from(size.height))
            .expect("viewport");
        let workspace_open = self.state == FixtureState::Workspace
            && self
                .transition
                .is_none_or(|(_, progress)| progress == UnitInterval::ONE);
        let mut layout = ProductLayout::resolve_reactive_at_edge(
            viewport,
            &mut None,
            workspace_open,
            true,
            None,
            self.edge,
            14.,
        )
        .expect("valid cardinal endpoint");
        let transition_frame = self.transition.map(|(source_edge, progress)| {
            let source_layout = ProductLayout::resolve_reactive_at_edge(
                viewport,
                &mut None,
                false,
                true,
                None,
                source_edge,
                14.,
            )
            .expect("source endpoint");
            let transition = BarPlacementTransition::new(
                viewport,
                source_layout.bar_placement,
                layout.bar_placement,
            )
            .expect("valid transition endpoints");
            transition
                .evaluate(progress)
                .expect("valid transition progress")
        });
        let active_placement: ResolvedBarPlacement = if let Some(ref motion) = transition_frame {
            if motion.active_edge == layout.bar_placement.edge() {
                layout.bar_placement
            } else {
                ProductLayout::resolve_reactive_at_edge(
                    viewport,
                    &mut None,
                    false,
                    true,
                    None,
                    motion.active_edge,
                    14.,
                )
                .expect("active edge placement")
                .bar_placement
            }
        } else {
            layout.bar_placement
        };
        let shared = (self.state == FixtureState::AppStack).then(|| {
            SharedDockEnvelope::resolve_with_safe(
                viewport,
                layout.dock,
                layout.dock,
                SharedMotion::endpoint(true),
                layout.dock_safe_bounds().unwrap(),
            )
            .expect("safe bottom shared outer for visible AppStack")
        });
        if !workspace_open {
            layout.shared_dock = shared;
        }
        if let Some(ref motion) = transition_frame {
            layout.bar = motion.active_allocation;
        }
        let frame = if let Some(ref motion) = transition_frame {
            layout.frame_with_bar_transition(MonitorSceneId::new(6), SceneEpoch::new(1), motion)
        } else {
            layout.frame(MonitorSceneId::new(6), SceneEpoch::new(1))
        };
        let input = frame
            .input_regions
            .iter()
            .map(|r| gpui::Bounds {
                origin: point(px(r.left()), px(r.top())),
                size: gpui::size(px(r.width()), px(r.height())),
            })
            .collect::<Vec<_>>();
        window.set_input_region(Some(&input));
        let semantic = self.content.runtime.semantic();
        let draw_frame = frame.clone();
        let draw_transition = transition_frame.clone();
        let reactive = layout.reactive_frame();
        let draw_shared = layout.shared_dock;
        let paint = canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                crate::scene_runtime::paint_product_scene_with_transition(
                    bounds,
                    &draw_frame,
                    0.,
                    reactive,
                    draw_shared.as_ref(),
                    draw_transition.as_ref(),
                    true,
                    window,
                )
            },
        )
        .absolute()
        .inset_0();
        let observed = self.observed.clone();
        let bar_geometry = layout.clone();
        let bar = crate::product_bar::content(semantic, active_placement, 14.)
            .absolute()
            .left(px(layout.bar.left()))
            .top(px(layout.bar.top()))
            .w(px(layout.bar.width()))
            .h(px(layout.bar.height()))
            .id("product-bar")
            .role(gpui::Role::Button)
            .aria_label(
                semantic
                    .bar_accessibility_action_label(workspace_open)
                    .unwrap(),
            )
            .track_focus(&self.bar_focus)
            .tab_stop(true)
            .tab_index(0)
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                if this.ack.is_some() {
                    return;
                }
                let p = event.position();
                if matches!(event, gpui::ClickEvent::Keyboard(_))
                    || bar_geometry.hit(f32::from(p.x), f32::from(p.y)) == Some(ProductHit::Bar)
                {
                    this.clicks += 1;
                    this.state = if this.state == FixtureState::Workspace {
                        FixtureState::Closed
                    } else {
                        FixtureState::Workspace
                    };
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| {
                        *observed.borrow_mut() = Some([
                            f32::from(bounds.left()),
                            f32::from(bounds.top()),
                            f32::from(bounds.right()),
                            f32::from(bounds.bottom()),
                        ]);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        let dock = div()
            .absolute()
            .left(px(layout.dock.left()))
            .top(px(layout.dock.top()))
            .w(px(layout.dock.width()))
            .h(px(layout.dock.height()))
            .flex()
            .items_center()
            .justify_center()
            .text_color(gpui::white())
            .text_size(px(14.))
            .child(semantic.dock_label())
            .id("product-dock")
            .role(gpui::Role::Button)
            .aria_label("Ouvrir AppStack")
            .track_focus(&self.dock_focus)
            .tab_stop(true)
            .tab_index(1)
            .on_click(cx.listener(|this, _, _, cx| {
                if this.ack.is_some() {
                    return;
                }
                this.state = FixtureState::AppStack;
                cx.notify();
            }));
        let workspace = layout.workspace.map(|r| {
            crate::product_bar::workspace_content(semantic, layout.bar_placement, 14.)
                .absolute()
                .left(px(r.left()))
                .top(px(r.top()))
                .w(px(r.width()))
                .h(px(r.height()))
                .id("product-workspace")
                .role(gpui::Role::Button)
                .aria_label("Fermer le résumé de l’espace de travail")
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.ack.is_some() {
                        return;
                    }
                    this.state = FixtureState::Closed;
                    cx.notify();
                }))
        });
        let mut appstack: Option<AnyElement> = None;
        if self.state == FixtureState::AppStack {
            let r = shared
                .as_ref()
                .expect("AppStack state builds shared envelope")
                .content_clip;
            let rows = crate::product_appstack::appstack_rows(&self.content.runtime);
            let children = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    installed_application_item(
                        ("appstack-row", i),
                        row.name.clone(),
                        row.entry.as_str(),
                        self.content.design,
                        &ProductControlFocus::new(&self.row_focus[i], i as isize + 2),
                    )
                })
                .collect::<Vec<_>>();
            appstack = Some(
                div()
                    .id("product-appstack")
                    .role(gpui::Role::Group)
                    .aria_label("AppStack")
                    .absolute()
                    .left(px(r.left()))
                    .top(px(r.top()))
                    .w(px(r.width()))
                    .h(px(r.height()))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("product-appstack-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(children),
                    )
                    .into_any_element(),
            );
        }
        let snapshot = json!({"schema":"gnosix.donor07.transition-fixture.v1","edge":format!("{:?}",self.edge),"transition":transition_frame.as_ref().map(|t|json!({"progress":t.progress.value(),"phase":format!("{:?}",t.phase),"active_edge":format!("{:?}",t.active_edge),"reservation_edge":format!("{:?}",t.reservation_edge),"arms":t.arms.iter().map(|a|json!({"edge":format!("{:?}",a.edge),"bounds":rect(a.bounds)})).collect::<Vec<_>>()})),"state":format!("{:?}",self.state),"viewport":[viewport.width,viewport.height],"scene_id":6,"epoch":1,"bar":rect(layout.bar),"orientation":format!("{:?}",active_placement.orientation()),"dock":rect(layout.dock),"dock_safe":layout.dock_safe_bounds().map(rect),"workspace":layout.workspace.map(rect),"shared_outer":layout.shared_dock.map(|s|rect(s.geometry.outer)),"appstack":layout.shared_dock.filter(|s|s.appstack.visible).map(|s|rect(s.content_clip)),"input":frame.input_regions.iter().copied().map(rect).collect::<Vec<_>>(),"reservation":[frame.reservation.top,frame.reservation.right,frame.reservation.bottom,frame.reservation.left],"bar_identity":"SceneNodeId::BAR / SceneNodeRole::Bar","logical_groups":semantic.bar_groups(),"bar_accessibility_label":semantic.bar_accessibility_action_label(workspace_open),"focus_sequence":std::iter::once("product-bar".to_string()).chain(std::iter::once("product-dock".to_string())).chain(crate::product_appstack::appstack_rows(&self.content.runtime).into_iter().map(|row|row.entry.as_str().to_string())).collect::<Vec<_>>(),"execution":"DISABLED_FIXTURE"});
        self.last_layout = Some(layout);
        let ack = self.ack.clone();
        let observed = self.observed.clone();
        div()
            .relative()
            .size_full()
            .bg(rgba(0x0d1218ff))
            .child(paint)
            .child(bar)
            .child(dock)
            .when_some(workspace, |d, w| d.child(w))
            .when_some(appstack, |d, a| d.child(a))
            .child(
                canvas(
                    move |_, _, _| (),
                    move |_, _, window, _| {
                        if let Some(path) = &ack {
                            let mut snapshot = snapshot.clone();
                            snapshot["native_bar_bounds"] = json!(*observed.borrow());
                            snapshot["a11y_tree"] = json!(window.debug_a11y_tree_json());
                            std::fs::write(path, snapshot.to_string()).expect("fixture ack");
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext, size};
    #[test]
    fn native_bar_bounds_and_focus_sequence_survive_all_cardinal_placements() {
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let mut app = TestAppContext::single();
            let window = app
                .add_window(|_, cx| BarPlacementLab::new(edge, FixtureState::AppStack, None, cx));
            let context = VisualTestContext::from_window(window.into(), &app).into_mut();
            context.simulate_resize(size(px(1280.), px(800.)));
            context.run_until_parked();
            let (handles, observed, layout) = {
                let cx = &app.app.borrow();
                let view = window.read(cx).unwrap();
                (
                    std::iter::once(view.bar_focus.clone())
                        .chain(std::iter::once(view.dock_focus.clone()))
                        .chain(view.row_focus.iter().cloned())
                        .collect::<Vec<_>>(),
                    *view.observed.borrow(),
                    view.last_layout.clone().unwrap(),
                )
            };
            let observed = observed.expect("actual Bar prepaint allocation");
            for (a, b) in observed.into_iter().zip(rect(layout.bar)) {
                assert!(
                    (a - b).abs() <= 0.5,
                    "{edge:?} native Bar allocation observed={observed:?} expected={:?}",
                    rect(layout.bar)
                );
            }
            context.update(|window, cx| {
                window.focus(&handles[0], cx);
                for handle in handles.iter().skip(1) {
                    window.focus_next(cx);
                    assert!(handle.is_focused(window), "{edge:?} forward logical order");
                }
                for handle in handles.iter().rev().skip(1) {
                    window.focus_prev(cx);
                    assert!(handle.is_focused(window), "{edge:?} backward logical order");
                }
                // TestPlatform does not activate an accessibility adapter.
                // Native focus traversal above is independent of that adapter.
            });
        }
    }
    #[test]
    fn native_bar_clicks_respect_rounded_input_on_every_edge() {
        for edge in [
            SurfaceEdge::Top,
            SurfaceEdge::Right,
            SurfaceEdge::Bottom,
            SurfaceEdge::Left,
        ] {
            let mut app = TestAppContext::single();
            let window =
                app.add_window(|_, cx| BarPlacementLab::new(edge, FixtureState::Closed, None, cx));
            let context = VisualTestContext::from_window(window.into(), &app).into_mut();
            context.simulate_resize(size(px(1280.), px(800.)));
            context.run_until_parked();
            let bar = window
                .read(&app.app.borrow())
                .unwrap()
                .last_layout
                .as_ref()
                .unwrap()
                .bar;
            for p in [
                point(px(bar.left() + 1.), px(bar.top() + 1.)),
                point(px(bar.left() - 2.), px(bar.top() + bar.height() / 2.)),
            ] {
                context.simulate_click(p, gpui::Modifiers::default());
                assert_eq!(
                    window.read(&app.app.borrow()).unwrap().clicks,
                    0,
                    "{edge:?} corner/outside pass through"
                );
            }
            context.simulate_click(
                point(
                    px((bar.left() + bar.right()) / 2.),
                    px((bar.top() + bar.bottom()) / 2.),
                ),
                gpui::Modifiers::default(),
            );
            assert_eq!(
                window.read(&app.app.borrow()).unwrap().clicks,
                1,
                "{edge:?} visible center activates"
            );
        }
    }
}
