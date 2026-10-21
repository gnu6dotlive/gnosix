//! BAR-01B: the Bar's modules ride the travelling material (B01, B02).
//!
//! During an edge travel (TOPO-01B/C3) the rest content is not mounted; this
//! layer places every module on the arms with `bar_strip::Conveyor`, each in
//! its column or row form, clipped to the material it stands on. At either
//! end of the route the placements are the rest layouts, so the hand-over to
//! the rest content is seamless.

use super::*;
use crate::bar_strip::{Conveyor, ModuleForm, ModulePlacement, StripLayout, canonical_layout};
use crate::reference_bar::IdleBarEntry;

/// The modules' lengths in both orientations, from the same recipes the
/// rest content and the pointer router use.
pub(super) fn module_forms(
    presentation: &crate::reference_bar::BarPresentation,
    active_window: Option<&crate::reference_active_window::ActiveWindowPlan>,
) -> Vec<ModuleForm> {
    use crate::reference_bar::{
        reference_module_layout, row_active_window_length, row_clock_length,
    };
    reference_module_layout(presentation)
        .entries
        .iter()
        .filter(|(entry, _)| *entry != IdleBarEntry::Spacer)
        .filter_map(|(entry, height)| {
            let (vertical, horizontal) = match entry {
                IdleBarEntry::ActiveWindow => {
                    let plan = active_window?;
                    (
                        plan.implicit_height,
                        row_active_window_length(plan.icon_width),
                    )
                }
                IdleBarEntry::Clock => (
                    *height,
                    presentation
                        .clock
                        .as_ref()
                        .map_or(*height, row_clock_length),
                ),
                _ => (*height, *height),
            };
            Some(ModuleForm {
                entry: *entry,
                vertical,
                horizontal,
            })
        })
        .collect()
}

/// The rest layout of these modules along an edge `length` long, in the
/// Bar's groups and order (BAR-01D).
pub(super) fn rest_layout(
    forms: &[ModuleForm],
    order: &crate::reference_bar::BarModuleOrder,
    length: f32,
    horizontal: bool,
) -> StripLayout {
    let split: [Vec<ModuleForm>; 3] = order.0.clone().map(|group| {
        group
            .iter()
            .filter_map(|e| forms.iter().find(|f| f.entry == *e).copied())
            .collect()
    });
    canonical_layout([&split[0], &split[1], &split[2]], length, horizontal)
}

fn edge_length(edge: gnos_ux_surface::envelope::SurfaceEdge, viewport: (f32, f32)) -> f32 {
    use gnos_ux_surface::envelope::SurfaceEdge;
    match edge {
        SurfaceEdge::Top | SurfaceEdge::Bottom => viewport.0,
        SurfaceEdge::Left | SurfaceEdge::Right => viewport.1,
    }
}

fn is_horizontal(edge: gnos_ux_surface::envelope::SurfaceEdge) -> bool {
    use gnos_ux_surface::envelope::SurfaceEdge;
    matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom)
}

/// The rest placements on `side`, the rail standing at `rail`.
pub(super) fn rest_placements(
    forms: &[ModuleForm],
    order: &crate::reference_bar::BarModuleOrder,
    side: crate::rail_topology::RailSide,
    (x, y, w, h): (f32, f32, f32, f32),
    viewport: (f32, f32),
) -> Vec<ModulePlacement> {
    let edge = side.edge();
    let rest = rest_layout(forms, order, side.length(viewport), side.is_horizontal());
    let route = [edge];
    gnos_ux_surface::grammar::GeometryRect::try_new(x, y, x + w, y + h, 0.0)
        .map(|bounds| {
            Conveyor {
                viewport,
                route: &route,
                forms,
                source: &rest,
                target: &rest,
                inner: crate::reference_bar::BAR_INNER_WIDTH,
            }
            .rest_on(edge, bounds)
        })
        .unwrap_or_default()
}

/// Every module's placement on this frame of the route.
pub(super) fn travel_placements(
    frame: &gnos_ux_surface::grammar::ResolvedBarTransitionFrame,
    route: &[gnos_ux_surface::envelope::SurfaceEdge],
    forms: &[ModuleForm],
    order: &crate::reference_bar::BarModuleOrder,
    // BAR-03: the travel keeps the order it arrives in (a reversing route
    // after the Bar has rested on an edge).
    mirror: bool,
    viewport: (f32, f32),
) -> Vec<ModulePlacement> {
    let (Some(first), Some(last)) = (route.first(), route.last()) else {
        return Vec::new();
    };
    let source = rest_layout(
        forms,
        order,
        edge_length(*first, viewport),
        is_horizontal(*first),
    );
    let mirrored;
    let target_order = if mirror {
        mirrored = crate::bar_strip::mirrored(order);
        &mirrored
    } else {
        order
    };
    let target = rest_layout(
        forms,
        target_order,
        edge_length(*last, viewport),
        is_horizontal(*last),
    );
    Conveyor {
        viewport,
        route,
        forms,
        source: &source,
        target: &target,
        inner: crate::reference_bar::BAR_INNER_WIDTH,
    }
    .place(frame)
}

/// The travel layer, in scene coordinates: each module's element centred in
/// its footprint, clipped to the part of it on the material.
pub(super) fn travel_layer(
    placements: &[ModulePlacement],
    mut column: Vec<(IdleBarEntry, gpui::AnyElement)>,
    mut row: Vec<(IdleBarEntry, gpui::AnyElement)>,
) -> gpui::Div {
    let mut layer = div().absolute().left(px(0.0)).top(px(0.0)).size_full();
    for placed in placements {
        let forms = if placed.horizontal {
            &mut row
        } else {
            &mut column
        };
        let Some(index) = forms.iter().position(|(entry, _)| *entry == placed.entry) else {
            continue;
        };
        let (_, element) = forms.remove(index);
        let (clip, rect) = (placed.clip, placed.rect);
        if clip.width <= 0.0 || clip.height <= 0.0 {
            continue;
        }
        layer = layer.child(
            div()
                .absolute()
                .left(px(clip.x))
                .top(px(clip.y))
                .w(px(clip.width))
                .h(px(clip.height))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left(px(rect.x - clip.x))
                        .top(px(rect.y - clip.y))
                        .w(px(rect.width))
                        .h(px(rect.height))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(element),
                ),
        );
    }
    layer
}

/// How far the Deck travels past the output's edge when it relocates, so
/// its hump and its margin leave the output too.
pub(super) const DECK_CLEARANCE: f32 = 80.0;
/// The drawers' and Settings' retraction distances.
const DRAWER_TRAVEL: f32 = 480.0;

/// BAR-01C: the drawers' and Settings' offsets while the rail travels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct RelocationOffsets {
    pub toasts_dx: f32,
    pub centre_dx: f32,
    pub settings_dy: f32,
}

impl RelocationOffsets {
    pub(super) fn of(r: &crate::rail_topology::Relocation, viewport: (f32, f32)) -> Self {
        use crate::rail_topology::RailSide as S;
        // The drawers sit on the left beside a right rail, on the right
        // otherwise: they slide out sideways, through their own edge.
        let sideways = if r.side() == S::Right { -1.0 } else { 1.0 };
        // Settings hangs from the top, or stands on a bottom Bar (T04, T06).
        let vertical = if r.side() == S::Bottom { 1.0 } else { -1.0 };
        Self {
            toasts_dx: sideways * DRAWER_TRAVEL * r.of(|s| (s == S::Right, s == S::Top)),
            centre_dx: sideways
                * DRAWER_TRAVEL
                * r.of(|s| (s == S::Right, s == S::Top, s == S::Bottom)),
            settings_dy: vertical * viewport.1 * r.of(|s| (s == S::Bottom, s == S::Top)),
        }
    }
}

/// A layer moved by `(dx, dy)`, position only.
pub(super) fn shifted(layer: impl IntoElement, dx: f32, dy: f32) -> gpui::AnyElement {
    div()
        .absolute()
        .left(px(dx))
        .top(px(dy))
        .size_full()
        .child(layer)
        .into_any_element()
}

/// The Deck's bounds and the relocation offsets, for the trace.
pub(super) fn surfaces_trace(
    deck: Option<&crate::deck::wave::DeckShape>,
    relocation: Option<&crate::rail_topology::Relocation>,
    viewport: (f32, f32),
) -> String {
    let deck = deck.map_or("None".to_owned(), |shape| {
        let b = shape.bounds;
        format!("[{:.1},{:.1},{:.1},{:.1}]", b.x, b.y, b.width, b.height)
    });
    let o = relocation.map_or_else(RelocationOffsets::default, |r| {
        RelocationOffsets::of(r, viewport)
    });
    format!(
        "deck={deck} toasts_dx={:.1} centre_dx={:.1} settings_dy={:.1}",
        o.toasts_dx, o.centre_dx, o.settings_dy
    )
}

/// `GNOS_UX_BAR_CONTINUITY` (qualification only): every module's visible
/// rect and form on this frame, the arms it stands on, and the Deck and the
/// drawers' offsets (BAR-01C).
pub(super) fn report_continuity(
    seq: u64,
    frame: Option<&gnos_ux_surface::grammar::ResolvedBarTransitionFrame>,
    placements: &[ModulePlacement],
    surfaces: &str,
) {
    let r = |r: blobin_ui_core::layout::Rect| {
        format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
    };
    let modules = placements
        .iter()
        .map(|m| {
            format!(
                "{{{:?} clip={} rect={} row={}}}",
                m.entry,
                r(m.clip),
                r(m.rect),
                m.horizontal
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    match frame {
        Some(frame) => {
            let arms = frame
                .arms
                .iter()
                .map(|a| {
                    format!(
                        "{:?}:[{:.1},{:.1},{:.1},{:.1}]",
                        a.edge,
                        a.bounds.left(),
                        a.bounds.top(),
                        a.bounds.width(),
                        a.bounds.height()
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "GNOS_UX_BAR_CONTINUITY seq={seq} state=travel phase={:?} progress={:.4} arms=[{arms}] {surfaces} modules=[{modules}]",
                frame.phase,
                frame.progress.value()
            );
        }
        None => println!(
            "GNOS_UX_BAR_CONTINUITY seq={seq} state=settled {surfaces} modules=[{modules}]"
        ),
    }
}

/// `GNOS_UX_BAR_MODULES`: the modules at rest, each
/// time the rest layout changes (BAR-01I: where a press finds them).
pub(super) fn report_modules(side: crate::rail_topology::RailSide, placements: &[ModulePlacement]) {
    let modules = placements
        .iter()
        .map(|m| {
            format!(
                "{{{:?} rect=[{:.1},{:.1},{:.1},{:.1}]}}",
                m.entry, m.rect.x, m.rect.y, m.rect.width, m.rect.height
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    println!("GNOS_UX_BAR_MODULES side={side:?} modules=[{modules}]");
}

/// Whether the rest layout jumped between two frames: a module came or
/// went, or one moved or changed size by more than 2 px (an eased metric
/// moves them far less per frame).
pub(super) fn relayout_jumped(before: &[ModulePlacement], now: &[ModulePlacement]) -> bool {
    if before.len() != now.len() {
        return true;
    }
    now.iter().any(|m| {
        before.iter().find(|b| b.entry == m.entry).is_none_or(|b| {
            let (r, s) = (m.rect, b.rect);
            (r.x - s.x).abs() > 2.0
                || (r.y - s.y).abs() > 2.0
                || (r.width - s.width).abs() > 2.0
                || (r.height - s.height).abs() > 2.0
        })
    })
}

/// The Bar between its previous and its new rest layout at eased `w`: a
/// module that comes grows from nothing at its new place, one that goes
/// shrinks to nothing at its old place.
pub(super) fn blend_relayout(
    from: &[ModulePlacement],
    to: &[ModulePlacement],
    w: f32,
    material: blobin_ui_core::layout::Rect,
) -> Vec<ModulePlacement> {
    use blobin_ui_core::layout::Rect;
    let point = |r: Rect| Rect::new(r.x + r.width / 2.0, r.y + r.height / 2.0, 0.0, 0.0);
    let lerp = |a: f32, b: f32| a + (b - a) * w;
    let mut out = Vec::new();
    let mut entries: Vec<IdleBarEntry> = to.iter().map(|m| m.entry).collect();
    entries.extend(
        from.iter()
            .map(|m| m.entry)
            .filter(|e| !to.iter().any(|m| m.entry == *e)),
    );
    for entry in entries {
        let a = from.iter().find(|m| m.entry == entry);
        let b = to.iter().find(|m| m.entry == entry);
        let (ra, rb, row) = match (a, b) {
            (Some(a), Some(b)) => (
                a.rect,
                b.rect,
                if w < 0.5 { a.horizontal } else { b.horizontal },
            ),
            (None, Some(b)) => (point(b.rect), b.rect, b.horizontal),
            (Some(a), None) => (a.rect, point(a.rect), a.horizontal),
            (None, None) => continue,
        };
        let rect = Rect::new(
            lerp(ra.x, rb.x),
            lerp(ra.y, rb.y),
            lerp(ra.width, rb.width),
            lerp(ra.height, rb.height),
        );
        let x = rect.x.max(material.x);
        let y = rect.y.max(material.y);
        let r = (rect.x + rect.width)
            .min(material.x + material.width)
            .max(x);
        let btm = (rect.y + rect.height)
            .min(material.y + material.height)
            .max(y);
        out.push(ModulePlacement {
            entry,
            rect,
            clip: Rect::new(x, y, r - x, btm - y),
            horizontal: row,
        });
    }
    out
}

/// `GNOS_UX_BAR_CONTINUITY state=relayout` (qualification only): a settings
/// reflow's frame, at eased progress `w`.
pub(super) fn report_relayout(seq: u64, w: f32, placements: &[ModulePlacement]) {
    let r = |r: blobin_ui_core::layout::Rect| {
        format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
    };
    let modules = placements
        .iter()
        .map(|m| {
            format!(
                "{{{:?} clip={} rect={} row={}}}",
                m.entry,
                r(m.clip),
                r(m.rect),
                m.horizontal
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    println!("GNOS_UX_BAR_CONTINUITY seq={seq} state=relayout progress={w:.4} modules=[{modules}]");
}

impl SceneHost {
    /// BAR-01G: a scene point over a travelling module, carried to where
    /// the rail's pointer router sees that module; any other point as is.
    pub(super) fn bar_travel_point(&self, x: f32, y: f32) -> (f32, f32) {
        self.bar_travel_map
            .as_ref()
            .and_then(|(now, rest)| crate::bar_strip::map_through(now, rest, x, y))
            .unwrap_or((x, y))
    }

    /// BAR-01G: the pointer moved while the rail travels: report the module
    /// it is carried to when that changes (popouts stay closed until the
    /// rail settles).
    pub(super) fn bar_travel_pointer_moved(&mut self, x: f32, y: f32) {
        let hit = self.bar_travel_map.as_ref().and_then(|(now, rest)| {
            let module = now.iter().rev().find(|m| m.clip.contains(x, y))?.entry;
            Some((module, crate::bar_strip::map_through(now, rest, x, y)?))
        });
        let module = hit.map(|(m, _)| m);
        if module != self.bar_travel_pointer {
            self.bar_travel_pointer = module;
            match hit {
                Some((m, (rx, ry))) => println!(
                    "GNOS_UX_BAR_TRAVEL_POINTER module={m:?} pointer={x:.1},{y:.1} rest={rx:.1},{ry:.1}"
                ),
                None => println!("GNOS_UX_BAR_TRAVEL_POINTER module=None pointer={x:.1},{y:.1}"),
            }
        }
    }
}
