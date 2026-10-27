//! BAR-01A: the Bar's modules as a strip that rides the material (B01, B02).
//!
//! At rest the Bar lays its modules out along its edge in reading order
//! (top to bottom on a vertical rail, left to right on a horizontal one), in
//! three groups split by the two flexible spacers: start, centre and end.
//! While the Bar travels between edges (TOPO-01B/C3) its material is one or
//! two arms (`ResolvedBarTransitionFrame`). This module says where every
//! module stands on that material, frame by frame, so nothing disappears:
//!
//! - **A conveyor.** Through a corner transfer the arms keep a constant total
//!   length, so each module keeps one material coordinate (its distance from
//!   the trailing end) and the corner sweeps past it: the module slides with
//!   its arm, turns the corner and runs on along the next edge.
//! - **The turn.** The two arms' centre lines meet at the corner square's
//!   centre (`K`). A module whose coordinate is within half the Bar's
//!   thickness of the corner waits at `K`, and its footprint morphs from its
//!   vertical form to its row form (or back) across the turn, so it pivots in
//!   place instead of jumping.
//! - **The order (B02).** A corner where two reading starts meet (top-left)
//!   or two reading ends (bottom-right) reverses the strip. On such a route
//!   the final slide carries every module from where the conveyor left it to
//!   its canonical place; the start and end groups cross inside the Bar.
//!   On every route the final slide also respaces the strip to the target
//!   edge's canonical layout, so the settled Bar is exactly the rest layout.
//!
//! Everything is pure geometry: the scene supplies the modules' lengths in
//! both orientations and the canonical layouts at both ends.

use crate::reference_bar::IdleBarEntry;
use blobin_ui_core::layout::Rect;
use gnos_ux_surface::envelope::SurfaceEdge;
use gnos_ux_surface::grammar::{
    BarPlacementTransitionPhase, BarTransitionCorner, GeometryRect, ResolvedBarTransitionFrame,
};

/// A module's length along the Bar in each orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModuleForm {
    pub entry: IdleBarEntry,
    /// Along a vertical rail (its height).
    pub vertical: f32,
    /// Along a horizontal Bar (its width).
    pub horizontal: f32,
}

impl ModuleForm {
    fn along(&self, horizontal: bool) -> f32 {
        if horizontal {
            self.horizontal
        } else {
            self.vertical
        }
    }
}

/// The canonical layout on one edge: each module's centre, measured along
/// the reading direction from the Bar's reading start.
#[derive(Clone, Debug, PartialEq)]
pub struct StripLayout {
    pub length: f32,
    pub centres: Vec<(IdleBarEntry, f32)>,
}

impl StripLayout {
    fn centre(&self, entry: IdleBarEntry) -> Option<f32> {
        self.centres
            .iter()
            .find(|(e, _)| *e == entry)
            .map(|(_, c)| *c)
    }
}

/// End padding of the rest layout (`LARGE_VERTICAL_PADDING`): the router's
/// column. Its gap is `reference_bar::module_gap()`, scaled by the density.
pub const STRIP_PADDING: f32 = crate::reference_bar::LARGE_VERTICAL_PADDING;

/// Lay three groups out as the rest Bar does: padding at both ends, a gap
/// between every child, and the free length shared by the two spacers that
/// split start, centre and end.
pub fn canonical_layout(groups: [&[ModuleForm]; 3], length: f32, horizontal: bool) -> StripLayout {
    let fixed = fixed_length(groups, horizontal);
    let spacer = ((length - fixed) / 2.0).max(0.0);
    let mut at = STRIP_PADDING;
    let mut centres = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            at += spacer + crate::reference_bar::module_gap();
        }
        for module in group.iter() {
            let len = module.along(horizontal);
            centres.push((module.entry, at + len / 2.0));
            at += len + crate::reference_bar::module_gap();
        }
    }
    StripLayout { length, centres }
}

/// The length the groups need with no free space: both paddings, every
/// module and every gap (the two spacers count as children).
pub fn fixed_length(groups: [&[ModuleForm]; 3], horizontal: bool) -> f32 {
    let children = groups.iter().map(|g| g.len()).sum::<usize>() + 2;
    2.0 * STRIP_PADDING
        + groups
            .iter()
            .flat_map(|g| g.iter())
            .map(|m| m.along(horizontal))
            .sum::<f32>()
        + crate::reference_bar::module_gap() * (children.saturating_sub(1)) as f32
}

/// SET-02C: the order that fits an edge `length` long. While the modules
/// need more than the edge, the next module in `give_way` (the operator's
/// order) leaves the Bar. Returns the order kept and the modules hidden.
pub fn fitted_order(
    order: &crate::reference_bar::BarModuleOrder,
    forms: &[ModuleForm],
    length: f32,
    horizontal: bool,
    give_way: &[IdleBarEntry],
) -> (crate::reference_bar::BarModuleOrder, Vec<IdleBarEntry>) {
    let mut kept = order.clone();
    let mut hidden = Vec::new();
    let need = |o: &crate::reference_bar::BarModuleOrder| {
        let split: [Vec<ModuleForm>; 3] = o.0.clone().map(|group| {
            group
                .iter()
                .filter_map(|e| forms.iter().find(|f| f.entry == *e).copied())
                .collect()
        });
        fixed_length([&split[0], &split[1], &split[2]], horizontal)
    };
    for entry in give_way {
        if need(&kept) <= length {
            break;
        }
        if kept.group(*entry).is_some() {
            for group in &mut kept.0 {
                group.retain(|e| e != entry);
            }
            hidden.push(*entry);
        }
    }
    (kept, hidden)
}

fn horizontal(edge: SurfaceEdge) -> bool {
    matches!(edge, SurfaceEdge::Top | SurfaceEdge::Bottom)
}

/// The corner's coordinate along `edge`: 0 at the reading start.
fn corner_along(edge: SurfaceEdge, corner: BarTransitionCorner, viewport: (f32, f32)) -> f32 {
    use BarTransitionCorner as C;
    if horizontal(edge) {
        if matches!(corner, C::TopLeft | C::BottomLeft) {
            0.0
        } else {
            viewport.0
        }
    } else if matches!(corner, C::TopLeft | C::TopRight) {
        0.0
    } else {
        viewport.1
    }
}

fn at_reading_start(edge: SurfaceEdge, corner: BarTransitionCorner) -> bool {
    corner_along(edge, corner, (1.0, 1.0)) == 0.0
}

/// Whether the conveyor reverses the strip on this route: the first corner
/// is at the source's reading start exactly when the last corner is at the
/// target's reading end, or the reverse.
pub fn route_reverses(route: &[SurfaceEdge]) -> bool {
    let n = route.len();
    if n < 2 {
        return false;
    }
    let (Some(first), Some(last)) = (
        BarTransitionCorner::between(route[0], route[1]),
        BarTransitionCorner::between(route[n - 2], route[n - 1]),
    ) else {
        return false;
    };
    at_reading_start(route[0], first) == at_reading_start(route[n - 1], last)
}

/// BAR-03: the order a reversing route leaves the strip in, read from the
/// target's reading start: groups and modules mirrored. The travel ends
/// there instead of re-sorting to the canonical order; the operator
/// reorders the modules by hand when they want to.
pub fn mirrored(
    order: &crate::reference_bar::BarModuleOrder,
) -> crate::reference_bar::BarModuleOrder {
    let [start, centre, end] = order.0.clone();
    let rev = |mut g: Vec<IdleBarEntry>| {
        g.reverse();
        g
    };
    crate::reference_bar::BarModuleOrder([rev(end), rev(centre), rev(start)])
}

/// An arm's along and across ranges.
struct Arm {
    edge: SurfaceEdge,
    along: (f32, f32),
    across: (f32, f32),
}

impl Arm {
    fn of(edge: SurfaceEdge, r: GeometryRect) -> Self {
        if horizontal(edge) {
            Self {
                edge,
                along: (r.left(), r.right()),
                across: (r.top(), r.bottom()),
            }
        } else {
            Self {
                edge,
                along: (r.top(), r.bottom()),
                across: (r.left(), r.right()),
            }
        }
    }

    fn length(&self) -> f32 {
        self.along.1 - self.along.0
    }

    fn thickness(&self) -> f32 {
        self.across.1 - self.across.0
    }

    fn rect(&self) -> Rect {
        self.scene(self.along, self.across)
    }

    /// A box given in this arm's along/across coordinates, in scene ones.
    fn scene(&self, along: (f32, f32), across: (f32, f32)) -> Rect {
        if horizontal(self.edge) {
            Rect::new(along.0, across.0, along.1 - along.0, across.1 - across.0)
        } else {
            Rect::new(across.0, along.0, across.1 - across.0, along.1 - along.0)
        }
    }

    /// The scene point at `along` on this arm's centre line.
    fn point(&self, along: f32) -> (f32, f32) {
        let across = (self.across.0 + self.across.1) / 2.0;
        if horizontal(self.edge) {
            (along, across)
        } else {
            (across, along)
        }
    }
}

/// Where one module stands this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModulePlacement {
    pub entry: IdleBarEntry,
    /// The footprint, centred on the module's place.
    pub rect: Rect,
    /// The part of the footprint on the material: the content is clipped to
    /// it.
    pub clip: Rect,
    /// The module lays its content out as a row (else as a column).
    pub horizontal: bool,
}

/// What the conveyor needs besides the frame.
pub struct Conveyor<'a> {
    pub viewport: (f32, f32),
    /// The route's edges, source first (`BarPlacementTransition::route`).
    pub route: &'a [SurfaceEdge],
    pub forms: &'a [ModuleForm],
    /// The canonical layouts on the source and the target edges.
    pub source: &'a StripLayout,
    pub target: &'a StripLayout,
    /// The modules' own thickness across the Bar.
    pub inner: f32,
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = (a.x + a.width).min(b.x + b.width).max(x);
    let btm = (a.y + a.height).min(b.y + b.height).max(y);
    Rect::new(x, y, r - x, btm - y)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Conveyor<'_> {
    fn form(&self, entry: IdleBarEntry) -> Option<&ModuleForm> {
        self.forms.iter().find(|f| f.entry == entry)
    }

    /// The footprint `(w, h)` of `form` laid on `edge`.
    fn footprint(&self, form: &ModuleForm, edge: SurfaceEdge) -> (f32, f32) {
        let along = form.along(horizontal(edge));
        if horizontal(edge) {
            (along, self.inner)
        } else {
            (self.inner, along)
        }
    }

    fn placed(
        &self,
        form: &ModuleForm,
        centre: (f32, f32),
        size: (f32, f32),
        row: bool,
        material: Rect,
    ) -> ModulePlacement {
        let rect = Rect::new(
            centre.0 - size.0 / 2.0,
            centre.1 - size.1 / 2.0,
            size.0,
            size.1,
        );
        ModulePlacement {
            entry: form.entry,
            rect,
            clip: intersect(rect, material),
            horizontal: row,
        }
    }

    /// The canonical layout of `strip` along `arm`, scaled to its length.
    fn canonical(&self, arm: &Arm, strip: &StripLayout) -> Vec<ModulePlacement> {
        let scale = arm.length() / strip.length.max(1.0);
        strip
            .centres
            .iter()
            .filter_map(|(entry, c)| {
                let form = self.form(*entry)?;
                let size = self.footprint(form, arm.edge);
                Some(self.placed(
                    form,
                    arm.point(arm.along.0 + c * scale),
                    size,
                    horizontal(arm.edge),
                    arm.rect(),
                ))
            })
            .collect()
    }

    /// Each module's material coordinate as a fraction of the span: its
    /// distance from the trailing end, fixed for the whole leg once the
    /// first corner is known.
    fn coordinate(&self, entry: IdleBarEntry) -> Option<f32> {
        let first = BarTransitionCorner::between(self.route[0], *self.route.get(1)?)?;
        let f = self.source.centre(entry)? / self.source.length.max(1.0);
        Some(if at_reading_start(self.route[0], first) {
            1.0 - f
        } else {
            f
        })
    }

    /// The module's along position on `arm`, `d` from `corner`'s end.
    fn along_from_corner(&self, arm: &Arm, corner: BarTransitionCorner, d: f32) -> f32 {
        let c = corner_along(arm.edge, corner, self.viewport);
        if c <= arm.along.0 + 0.5 * arm.length() {
            arm.along.0 + d
        } else {
            arm.along.1 - d
        }
    }

    /// Clamp a span coordinate (pixels from the trailing end) off both ends
    /// by half the thickness: the corner square's centre is as far as a
    /// module goes.
    fn clamp(&self, x: f32, span: f32, t: f32) -> f32 {
        let lo = (t / 2.0).min(span / 2.0);
        x.clamp(lo, (span - lo).max(lo))
    }

    /// The rest layout on one end of the route, the Bar standing at
    /// `bounds` on `edge` (the settled Bar).
    pub fn rest_on(&self, edge: SurfaceEdge, bounds: GeometryRect) -> Vec<ModulePlacement> {
        let strip = if edge == self.route[0] && self.route.last() != Some(&edge) {
            self.source
        } else {
            self.target
        };
        self.canonical(&Arm::of(edge, bounds), strip)
    }

    /// Every module's placement for this frame.
    pub fn place(&self, frame: &ResolvedBarTransitionFrame) -> Vec<ModulePlacement> {
        let arms: Vec<Arm> = frame
            .arms
            .iter()
            .map(|a| Arm::of(a.edge, a.bounds))
            .collect();
        let Some(segment) = frame.segment else {
            // An endpoint: the rest layout of whichever edge it is.
            let Some(arm) = arms.first() else {
                return Vec::new();
            };
            let strip = if frame.progress.value() < 0.5 && arm.edge == self.route[0] {
                self.source
            } else {
                self.target
            };
            return self.canonical(arm, strip);
        };
        let s = segment.progress.value();
        match frame.phase {
            BarPlacementTransitionPhase::SourceSlide => {
                let Some(arm) = arms.first() else {
                    return Vec::new();
                };
                match segment.to_corner {
                    Some(corner) => self.slide_in(arm, corner, s),
                    // A slide along one edge (a retarget back to the edge it
                    // left, BAR-01J): the rest layout rides the arm.
                    None => self.canonical(arm, self.target),
                }
            }
            BarPlacementTransitionPhase::CornerTransfer => {
                let Some(corner) = segment.to_corner else {
                    return Vec::new();
                };
                self.turn(&arms, corner)
            }
            BarPlacementTransitionPhase::PerimeterSlide => {
                let (Some(arm), Some(from)) = (arms.first(), segment.from_corner) else {
                    return Vec::new();
                };
                self.carry(arm, from, 1.0)
            }
            BarPlacementTransitionPhase::TargetSlide => {
                let (Some(arm), Some(from)) = (arms.first(), segment.from_corner) else {
                    return Vec::new();
                };
                self.settle(arm, from, s)
            }
            BarPlacementTransitionPhase::RetargetHandoff | BarPlacementTransitionPhase::Settled => {
                // A retarget rebases mid-route (BAR-01J): during the handoff
                // the strip gathers on the active arm, in the layout the new
                // route's source slide starts from; the scene blends the
                // modules there from where they were painted.
                let Some(arm) = arms
                    .iter()
                    .find(|a| a.edge == frame.active_edge)
                    .or(arms.first())
                else {
                    return Vec::new();
                };
                if frame.phase == BarPlacementTransitionPhase::RetargetHandoff {
                    self.canonical(arm, self.source)
                } else {
                    self.canonical(arm, self.target)
                }
            }
        }
    }

    /// The span shared by the arms of a route (`common_span`): the corner
    /// arm the source slides into.
    fn span_of(&self, arms: &[Arm]) -> f32 {
        arms.iter().map(Arm::length).sum()
    }

    /// The source slide: from the rest layout to the conveyor's spacing on
    /// the corner arm, eased by the segment.
    fn slide_in(&self, arm: &Arm, corner: BarTransitionCorner, s: f32) -> Vec<ModulePlacement> {
        let rest = self.canonical(arm, self.source);
        let carried = self.carry(arm, corner, 0.0);
        rest.iter()
            .filter_map(|r| {
                let c = carried.iter().find(|c| c.entry == r.entry)?;
                Some(self.blend(r, c, s, arm.rect()))
            })
            .collect()
    }

    /// A single arm carrying the strip, its trailing end at `corner`
    /// (`trailing` 1) or its leading end there (`trailing` 0).
    fn carry(&self, arm: &Arm, corner: BarTransitionCorner, trailing: f32) -> Vec<ModulePlacement> {
        let span = arm.length();
        let t = arm.thickness();
        self.forms
            .iter()
            .filter_map(|form| {
                let x = self.clamp(self.coordinate(form.entry)? * span, span, t);
                // Distance from the corner end of the arm.
                let d = if trailing > 0.5 { x } else { span - x };
                let along = self.along_from_corner(arm, corner, d);
                Some(self.placed(
                    form,
                    arm.point(along),
                    self.footprint(form, arm.edge),
                    horizontal(arm.edge),
                    arm.rect(),
                ))
            })
            .collect()
    }

    /// A corner transfer: the from-arm shrinks into the corner while the
    /// to-arm grows out of it; each module keeps its coordinate and turns at
    /// the corner square's centre.
    fn turn(&self, arms: &[Arm], corner: BarTransitionCorner) -> Vec<ModulePlacement> {
        let Some(i) = self
            .route
            .windows(2)
            .position(|p| BarTransitionCorner::between(p[0], p[1]) == Some(corner))
        else {
            return Vec::new();
        };
        let (from_edge, to_edge) = (self.route[i], self.route[i + 1]);
        let from = arms.iter().find(|a| a.edge == from_edge);
        let to = arms.iter().find(|a| a.edge == to_edge);
        let lf = from.map_or(0.0, Arm::length);
        let span = self.span_of(arms);
        let t = from.or(to).map_or(0.0, Arm::thickness);
        let rho = t.max(1.0);
        self.forms
            .iter()
            .filter_map(|form| {
                let x = self.clamp(self.coordinate(form.entry)? * span, span, t);
                // Signed distance to the corner: positive on the from-arm.
                let z = lf - x;
                let (arm, d) = match (from, to) {
                    (Some(a), _) if z >= 0.0 => (a, z),
                    (_, Some(b)) => (b, -z),
                    (Some(a), None) => (a, 0.0),
                    (None, None) => return None,
                };
                let along = self.along_from_corner(arm, corner, d.max(t / 2.0));
                let centre = arm.point(along);
                // The footprint morphs across the turn.
                let k = ((rho - z) / (2.0 * rho)).clamp(0.0, 1.0);
                let a = self.footprint(form, from_edge);
                let b = self.footprint(form, to_edge);
                let size = (lerp(a.0, b.0, k), lerp(a.1, b.1, k));
                let row = horizontal(if z >= 0.0 { from_edge } else { to_edge });
                Some(self.placed(form, centre, size, row, arm.rect()))
            })
            .collect()
    }

    /// The final slide: from where the conveyor left the strip to the
    /// target's rest layout, eased by the segment; a reversed strip's groups
    /// cross here.
    fn settle(&self, arm: &Arm, from: BarTransitionCorner, s: f32) -> Vec<ModulePlacement> {
        let rest = self.canonical(arm, self.target);
        let carried = self.carry(arm, from, 1.0);
        rest.iter()
            .filter_map(|r| {
                let c = carried.iter().find(|c| c.entry == r.entry)?;
                Some(self.blend(c, r, s, arm.rect()))
            })
            .collect()
    }

    fn blend(
        &self,
        a: &ModulePlacement,
        b: &ModulePlacement,
        s: f32,
        material: Rect,
    ) -> ModulePlacement {
        let rect = Rect::new(
            lerp(a.rect.x, b.rect.x, s),
            lerp(a.rect.y, b.rect.y, s),
            lerp(a.rect.width, b.rect.width, s),
            lerp(a.rect.height, b.rect.height, s),
        );
        ModulePlacement {
            entry: b.entry,
            rect,
            clip: intersect(rect, material),
            horizontal: b.horizontal,
        }
    }
}

/// BAR-01J (B09): a retarget's handoff. Each module moves from where it was
/// painted when the travel was interrupted (`from`) to where the rebased
/// route places it (`to`), by `w` (0..=1, eased by the caller), along the
/// material: a module on the arm that retracts (`other`, as it was) goes
/// into the corner with it and on along the active arm (`active`), never
/// across the output. It takes its new form where it turns the corner (or
/// past halfway on one arm), and is clipped to the arm it overlaps most of
/// `arms` (the frame's).
pub fn handoff_blend(
    from: &[ModulePlacement],
    to: &[ModulePlacement],
    w: f32,
    active: Rect,
    other: Option<Rect>,
    arms: &[Rect],
) -> Vec<ModulePlacement> {
    let w = w.clamp(0.0, 1.0);
    let centre = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let active_row = active.width >= active.height;
    let inside = |r: Rect, (x, y): (f32, f32)| {
        x >= r.x - 1.0 && x <= r.x + r.width + 1.0 && y >= r.y - 1.0 && y <= r.y + r.height + 1.0
    };
    to.iter()
        .map(|b| {
            let Some(a) = from.iter().find(|a| a.entry == b.entry) else {
                return *b;
            };
            let size = (
                lerp(a.rect.width, b.rect.width, w),
                lerp(a.rect.height, b.rect.height, w),
            );
            let (p0, p1) = (centre(a.rect), centre(b.rect));
            let (point, horizontal) = match other.filter(|o| inside(*o, p0) && !inside(active, p0))
            {
                Some(o) => {
                    // The corner: where the two arms' axes cross.
                    let (oc, ac) = (centre(o), centre(active));
                    let c = if active_row {
                        (oc.0, ac.1)
                    } else {
                        (ac.0, oc.1)
                    };
                    let axis = |p: (f32, f32), row: bool| if row { p.0 - c.0 } else { p.1 - c.1 };
                    let d0 = axis(p0, !active_row).abs();
                    let d1 = axis(p1, active_row).abs();
                    let d = lerp(d0, -d1, w);
                    if d >= 0.0 {
                        let s = axis(p0, !active_row).signum();
                        let pt = if active_row {
                            (c.0, c.1 + s * d)
                        } else {
                            (c.0 + s * d, c.1)
                        };
                        (pt, a.horizontal)
                    } else {
                        let s = axis(p1, active_row).signum();
                        let pt = if active_row {
                            (c.0 - s * d, c.1)
                        } else {
                            (c.0, c.1 - s * d)
                        };
                        (pt, b.horizontal)
                    }
                }
                None => (
                    (lerp(p0.0, p1.0, w), lerp(p0.1, p1.1, w)),
                    if w < 0.5 { a.horizontal } else { b.horizontal },
                ),
            };
            let rect = Rect::new(
                point.0 - size.0 / 2.0,
                point.1 - size.1 / 2.0,
                size.0,
                size.1,
            );
            let clip = arms
                .iter()
                .map(|arm| intersect(rect, *arm))
                .max_by(|p, q| (p.width * p.height).total_cmp(&(q.width * q.height)))
                .unwrap_or(rect);
            ModulePlacement {
                entry: b.entry,
                rect,
                clip,
                horizontal,
            }
        })
        .collect()
}

/// BAR-01G: a pointer over a travelling module, carried to the same point
/// of that module in `rest` (where the rail's pointer router still sees
/// it); the topmost module where they overlap. The point keeps its fraction along the module, whichever form the
/// module has in each; across, it lands on the module's centre line. None
/// when the pointer is on no module's visible part.
pub fn map_through(
    now: &[ModulePlacement],
    rest: &[ModulePlacement],
    x: f32,
    y: f32,
) -> Option<(f32, f32)> {
    // Where modules overlap (crossing groups), the one painted on top: the
    // last placed.
    let here = now.iter().rev().find(|m| m.clip.contains(x, y))?;
    let there = rest.iter().find(|m| m.entry == here.entry)?;
    let r = here.rect;
    let along = if here.horizontal {
        (x - r.x) / r.width.max(1.0)
    } else {
        (y - r.y) / r.height.max(1.0)
    }
    .clamp(0.0, 1.0);
    let t = there.rect;
    Some(if there.horizontal {
        (t.x + along * t.width, t.y + t.height / 2.0)
    } else {
        (t.x + t.width / 2.0, t.y + along * t.height)
    })
}

/// BAR-01I (B08): where a module carried along the Bar would land: the
/// group (0 start, 1 centre, 2 end) and the index in it, or None off the
/// Bar (it would be hidden). `others` are the other modules at rest, laid
/// out without the carried one, so the answer does not move with the
/// preview it causes; `order` gives their groups; `along` is the pointer's
/// coordinate along the Bar and `bar` the Bar's extent along it.
pub fn insertion(
    others: &[ModulePlacement],
    order: &crate::reference_bar::BarModuleOrder,
    horizontal: bool,
    along: f32,
    bar: (f32, f32),
) -> (usize, usize) {
    let span = |m: &ModulePlacement| {
        if horizontal {
            (m.rect.x, m.rect.x + m.rect.width)
        } else {
            (m.rect.y, m.rect.y + m.rect.height)
        }
    };
    // Every slot: before each module of a group, after its last, or the
    // middle of the free stretch for an empty group. The index is the
    // group's own, counting the modules it holds that do not show (an
    // empty tray, a hidden logo).
    let mut slots: Vec<(f32, usize, usize)> = Vec::new();
    for (g, group) in order.0.iter().enumerate() {
        let shown: Vec<(usize, (f32, f32))> = group
            .iter()
            .enumerate()
            .filter_map(|(i, e)| others.iter().find(|m| m.entry == *e).map(|m| (i, span(m))))
            .collect();
        for (i, (start, _)) in &shown {
            slots.push((*start, g, *i));
        }
        if let Some((i, (_, end))) = shown.last() {
            slots.push((*end, g, i + 1));
        }
    }
    for g in 0..3 {
        if !slots.iter().any(|(_, sg, _)| *sg == g) {
            // An empty group sits at its end of the Bar, or in its middle.
            let at = match g {
                0 => bar.0,
                1 => (bar.0 + bar.1) / 2.0,
                _ => bar.1,
            };
            slots.push((at, g, 0));
        }
    }
    slots
        .into_iter()
        .min_by(|a, b| (a.0 - along).abs().total_cmp(&(b.0 - along).abs()))
        .map_or((2, 0), |(_, g, i)| (g, i))
}

/// The order with `entry` moved to `(group, index)`, or removed (`None`).
pub fn moved(
    order: &crate::reference_bar::BarModuleOrder,
    entry: IdleBarEntry,
    to: Option<(usize, usize)>,
) -> crate::reference_bar::BarModuleOrder {
    let mut groups = order.0.clone();
    for group in &mut groups {
        group.retain(|e| *e != entry);
    }
    if let Some((g, i)) = to {
        let group = &mut groups[g.min(2)];
        group.insert(i.min(group.len()), entry);
    }
    crate::reference_bar::BarModuleOrder(groups)
}

#[cfg(test)]
mod tests;
