//! Experimental lab adapters. No production topology or donor authority adoption.
//! A delegates to the current product resolver; B/C share one rounded outer.
use crate::product_scene::ProductLayout;
use gnos_ux_surface::grammar::connective_hull::{ConnectiveHullGeometry, hull_probe};
use gnos_ux_surface::grammar::reactive_envelope::{ReactiveEnvelopeFrame, ReactiveEnvelopeSolver};
use gnos_ux_surface::grammar::{GeometryError, GeometryRect, SurfaceViewport};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
    A,
    B,
    C,
}
impl Topology {
    pub const ALL: [Self; 3] = [Self::A, Self::B, Self::C];
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ComparisonContract {
    pub viewport: SurfaceViewport,
    pub safe: GeometryRect,
    pub dock: GeometryRect,
    pub desired_width: f32,
    pub desired_height: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub content_inset: f32,
    pub radius: f32,
}
impl ComparisonContract {
    fn validate(self) -> Result<(), GeometryError> {
        self.viewport.validate()?;
        for (name, value) in [
            ("lab.desired_width", self.desired_width),
            ("lab.desired_height", self.desired_height),
            ("lab.content_width", self.content_width),
            ("lab.content_height", self.content_height),
            ("lab.content_inset", self.content_inset),
            ("lab.radius", self.radius),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
            if value < 0.0 {
                return Err(GeometryError::InvalidParameter {
                    name,
                    value,
                    requirement: "nonnegative",
                });
            }
        }
        Ok(())
    }
    pub fn current(viewport: SurfaceViewport) -> Result<Self, GeometryError> {
        viewport.validate()?;
        let dock =
            ProductLayout::resolved_dock(viewport).ok_or(GeometryError::NonFinite("lab.dock"))?;
        let frame = ProductLayout::settled_appstack_envelope(viewport, dock, true)
            .ok_or(GeometryError::NonFinite("lab.baseline"))?;
        let content = ProductLayout::appstack_content_bounds(&frame);
        Ok(Self {
            viewport,
            safe: GeometryRect::try_new(16.0, 16.0, viewport.width - 16.0, dock.bottom(), 0.0)?,
            dock,
            desired_width: frame.geometry.width(),
            desired_height: frame.geometry.height(),
            content_width: content.width(),
            content_height: content.height(),
            content_inset: content.top() - frame.geometry.top(),
            radius: dock.radius(),
        })
    }
    pub fn json(self) -> Value {
        json!({"schema":"gnosix.topology.common.v1", "viewport":[self.viewport.width,self.viewport.height],
            "output_scale":1.0,"safe_bounds":edges(self.safe),"dock_compact_bounds":edges(self.dock),
            "dock_compact_width":self.dock.width(),"dock_compact_height":self.dock.height(),
            "appstack_desired_width":self.desired_width,"appstack_desired_height":self.desired_height,
            "content_allocation_width":self.content_width,"content_allocation_height":self.content_height,
            "outer_radius":self.radius,"dimensions_source":"unchanged current A reduced-motion endpoint",
            "magnification":MagProfile::EXPERIMENT.json(),"shared_motion":SharedProgress::profile_json(),
            "A_motion":"unchanged ProductLayout resolver, fixed 1/60 simulation step per GPUI render",
            "A_magnified_body_policy":"unchanged solver parent-width ratio; actual body width may differ from common desired input",
            "structural_minimum":"resting Dock, independent of painted request", "topology_selected":null})
    }
}

pub fn edges(rect: GeometryRect) -> [f32; 4] {
    [rect.left(), rect.top(), rect.right(), rect.bottom()]
}
fn centered(cx: f32, top: f32, w: f32, h: f32, r: f32) -> Result<GeometryRect, GeometryError> {
    GeometryRect::try_new(cx - w * 0.5, top, cx + w * 0.5, top + h, r)
}

#[derive(Clone, Debug)]
pub enum PaintHull {
    Current(std::rc::Rc<ConnectiveHullGeometry>),
    Rounded(GeometryRect),
}
impl PaintHull {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Self::Current(hull) => hull_probe::contours(**hull).iter().any(|contour| {
                hull_probe::filled_intervals_at(contour, y)
                    .iter()
                    .any(|&(a, b)| x >= a - 0.002 && x <= b + 0.002)
            }),
            Self::Rounded(r) => {
                gnos_ux_surface::grammar::shared_envelope::rounded_contains(*r, x, y)
            }
        }
    }
    pub fn json(&self) -> Value {
        use gnos_ux_surface::grammar::HullCommand;
        match self {
            Self::Rounded(r) => {
                json!({"kind":"rounded_rectangle","bounds":edges(*r),"radius":r.radius()})
            }
            Self::Current(hull) => {
                json!({"kind":"current_connective_hull","commands":(0..hull.command_count()).map(|i|
                match hull.command(i).unwrap() {
                    HullCommand::MoveTo(p)=>json!(["M",p.x(),p.y()]),
                    HullCommand::LineTo(p)=>json!(["L",p.x(),p.y()]),
                    HullCommand::CubicTo{control1,control2,to}=>json!(["C",control1.x(),control1.y(),control2.x(),control2.y(),to.x(),to.y()]),
                    HullCommand::Close=>json!(["Z"]), HullCommand::Empty=>json!([]),
                }).collect::<Vec<_>>()})
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticInner {
    pub id: &'static str,
    pub parent: &'static str,
    pub bounds: GeometryRect,
    pub clip: GeometryRect,
    pub focus_group: &'static str,
}
#[derive(Clone, Debug)]
pub struct ComparisonFrame {
    pub topology: Topology,
    pub outer: GeometryRect,
    pub dock: GeometryRect,
    pub body: GeometryRect,
    pub content_allocation: GeometryRect,
    pub clip: GeometryRect,
    pub paint: PaintHull,
    pub input: Vec<GeometryRect>,
    pub progress: f32,
    pub velocity: f32,
    pub current: Option<ReactiveEnvelopeFrame>,
    pub inners: Vec<SemanticInner>,
}
impl ComparisonFrame {
    pub fn json(
        &self,
        contract: ComparisonContract,
        items: &[ItemProjection],
        timestamp_ms: f64,
        delta_ms: f64,
    ) -> Value {
        let contours = match &self.paint {
            PaintHull::Current(h) => Some(hull_probe::contours(**h)),
            PaintHull::Rounded(_) => None,
        };
        let inside = self.input.iter().all(|r| {
            (0..=8).all(|i| {
                let y = (r.top() + r.height() * i as f32 / 8.0).min(r.bottom() - 0.001);
                if let Some(contours) = &contours {
                    contours.iter().any(|c| {
                        hull_probe::filled_intervals_at(c, y)
                            .iter()
                            .any(|&(a, b)| r.left() >= a - 0.002 && r.right() <= b + 0.002)
                    })
                } else {
                    self.paint.contains(r.left(), y) && self.paint.contains(r.right(), y)
                }
            })
        });
        let intersections = match &self.paint {
            PaintHull::Current(_) => contours
                .as_ref()
                .unwrap()
                .iter()
                .map(|c| hull_probe::self_intersections(c))
                .sum::<usize>(),
            PaintHull::Rounded(_) => 0,
        };
        let body_coverage = match &self.paint {
            PaintHull::Current(_) if self.body.height() > self.body.radius() * 2.0 + 1.0 => {
                let contours = contours.as_ref().unwrap();
                let inset = self.body.radius().max(self.body.height() * 0.25);
                Some(
                    hull_probe::narrowest_filled_width(
                        &contours[0],
                        self.body.top() + inset,
                        self.body.bottom() - inset,
                        24,
                    ) / self.body.width(),
                )
            }
            PaintHull::Current(_) => None,
            PaintHull::Rounded(outer) => (self.body.height() > 0.0).then(|| {
                // Analytic scanline width of the actual rounded outer, sampled
                // over the same central body band as A. No assumed fill ratio.
                let inset = self.body.radius().max(self.body.height() * 0.25);
                (0..24)
                    .map(|i| {
                        let y = self.body.top()
                            + inset
                            + (self.body.height() - 2.0 * inset) * i as f32 / 23.0;
                        let radius = outer.radius();
                        let clearance = (y - outer.top()).min(outer.bottom() - y).max(0.0);
                        let cut = if clearance >= radius {
                            0.0
                        } else {
                            radius
                                - (radius * radius - (radius - clearance).powi(2))
                                    .max(0.0)
                                    .sqrt()
                        };
                        (self.body.right().min(outer.right() - cut)
                            - self.body.left().max(outer.left() + cut))
                        .max(0.0)
                            / self.body.width()
                    })
                    .fold(1.0, f32::min)
            }),
        };
        json!({"schema":"gnosix.geometry.debug.topology.v1","topology":self.topology.label(),
            "timestamp_ms":timestamp_ms,"delta_ms":delta_ms,"progress":self.progress,"velocity":self.velocity,
            "outer_bounds":edges(self.outer),"paint_bounds":edges(self.outer),"paint":self.paint.json(),
            "dock":{"structural_allocation":edges(contract.dock),"allocation":edges(self.dock),"paint":edges(self.dock),"input":self.input.iter().filter_map(|r|intersection(*r,self.dock)).map(edges).collect::<Vec<_>>()},
            "appstack":{"semantic_bounds":edges(self.body),"allocation":edges(self.content_allocation),"paint":edges(self.body),
                "input":self.input.iter().filter_map(|r|intersection(*r,self.body)).map(edges).collect::<Vec<_>>(),"clip":edges(self.clip),
                "desired_dimensions":[contract.desired_width,contract.desired_height]},
            "relation":{"parent":"Dock","child":"AppStack","lineage":"Dock -> AppStack","growth":"Upward",
                "representation":if self.topology==Topology::A {"Fused connective hull"}else{"abutting inner regions"},
                "joint_or_seam_y":self.dock.top(),"separation":self.dock.top()-self.body.bottom(),
                "attachment":self.current.map(|f|format!("{:?}",f.attachment)),
                "connector_width":self.current.map(|f|f.connector_width)},
            "material_owner":if self.topology==Topology::C {"SharedDockEnvelope"}else{"OuterEnvelope"},"material_owner_count":1,
            "material_owner_evidence":"renderer construction: one shell paint branch, not a runtime node census",
            "semantic_inners":self.inners.iter().map(|inner|json!({"id":inner.id,"parent":inner.parent,
                "bounds":edges(inner.bounds),"clip":edges(inner.clip),"focus_group":inner.focus_group,
                "input":self.input.iter().filter_map(|r|intersection(*r,inner.bounds)).map(edges).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "input_bounds":self.input.iter().copied().map(edges).collect::<Vec<_>>(),"input_inside_paint":inside,
            "self_intersections":intersections,"body_coverage":body_coverage,
            "body_coverage_method":if self.topology==Topology::A {"24 software scanlines over flattened cubic hull, central body band"}else{"24 analytic rounded-outer scanlines intersected with central body band"},
            "input_containment_method":if self.topology==Topology::A {"9 scanlines per input rectangle over flattened cubic hull"}else{"analytic convex rounded-outer containment, sampled band edges"},
            "body_coverage_sample_available":body_coverage.is_some(),
            "items":items.iter().map(ItemProjection::json).collect::<Vec<_>>()})
    }
}
pub fn intersection(a: GeometryRect, b: GeometryRect) -> Option<GeometryRect> {
    let (l, t, r, bottom) = (
        a.left().max(b.left()),
        a.top().max(b.top()),
        a.right().min(b.right()),
        a.bottom().min(b.bottom()),
    );
    (r > l && bottom > t).then(|| GeometryRect::try_new(l, t, r, bottom, 0.0).unwrap())
}
fn content_bounds(
    contract: ComparisonContract,
    body: GeometryRect,
) -> Result<(GeometryRect, GeometryRect), GeometryError> {
    let allocation = centered(
        (body.left() + body.right()) * 0.5,
        body.top() + contract.content_inset,
        contract.content_width,
        contract.content_height,
        0.0,
    )?;
    let inset = contract
        .content_inset
        .min(body.width() * 0.5)
        .min(body.height() * 0.5);
    let body_clip = GeometryRect::try_new(
        body.left() + inset,
        body.top() + inset,
        body.right() - inset,
        body.bottom() - inset,
        0.0,
    )?;
    let clip = intersection(allocation, body_clip).unwrap_or(GeometryRect::try_new(
        allocation.left(),
        allocation.top(),
        allocation.left(),
        allocation.top(),
        0.0,
    )?);
    Ok((allocation, clip))
}

pub fn current_frame(
    contract: ComparisonContract,
    dock: GeometryRect,
    solver: &mut Option<ReactiveEnvelopeSolver>,
    open: bool,
    reduced: bool,
) -> Result<ComparisonFrame, GeometryError> {
    contract.validate()?;
    let current = ProductLayout::resolve_appstack_envelope(
        contract.viewport,
        solver,
        dock,
        open,
        reduced,
        None,
    );
    let empty = centered(
        (dock.left() + dock.right()) * 0.5,
        dock.top(),
        contract.desired_width,
        0.0,
        0.0,
    )?;
    let (body, paint, outer, input, progress, velocity) = if let Some(f) = current {
        (
            f.geometry,
            PaintHull::Current(std::rc::Rc::new(f.hull)),
            f.hull.bounds(),
            f.shape_input_regions(),
            f.openness,
            f.velocity_y,
        )
    } else {
        (
            empty,
            PaintHull::Rounded(dock),
            dock,
            rounded_input(dock),
            0.0,
            0.0,
        )
    };
    let (content_allocation, clip) = content_bounds(contract, body)?;
    Ok(ComparisonFrame {
        topology: Topology::A,
        outer,
        dock,
        body,
        content_allocation,
        clip,
        paint,
        input,
        progress,
        velocity,
        current,
        inners: vec![],
    })
}
pub fn production_c_frame(
    contract: ComparisonContract,
    dock: GeometryRect,
    motion: crate::product_shared_envelope::SharedMotion,
) -> Result<ComparisonFrame, GeometryError> {
    let f = crate::product_shared_envelope::SharedDockEnvelope::resolve(
        contract.viewport,
        contract.dock,
        dock,
        motion,
    )?;
    Ok(ComparisonFrame {
        topology: Topology::C,
        outer: f.geometry.outer,
        dock: f.dock.bounds,
        body: f.appstack.bounds,
        content_allocation: f.content_allocation,
        clip: f.content_clip,
        paint: PaintHull::Rounded(f.geometry.outer),
        input: f.input_regions(),
        progress: motion.vertical.value,
        velocity: motion.vertical.velocity,
        current: None,
        inners: vec![
            SemanticInner {
                id: "DockInner",
                parent: "SharedDockEnvelope",
                bounds: f.dock.bounds,
                clip: f.dock.clip,
                focus_group: "Dock controls",
            },
            SemanticInner {
                id: "AppStackInner",
                parent: "SharedDockEnvelope",
                bounds: f.appstack.bounds,
                clip: f.appstack.clip,
                focus_group: "AppStack rows",
            },
        ],
    })
}
pub fn shared_frame(
    contract: ComparisonContract,
    dock: GeometryRect,
    progress: f32,
    velocity: f32,
    topology: Topology,
) -> Result<ComparisonFrame, GeometryError> {
    contract.validate()?;
    if !progress.is_finite() || !velocity.is_finite() {
        return Err(GeometryError::NonFinite("lab.progress"));
    }
    if !(0.0..=1.0).contains(&progress) || topology == Topology::A {
        return Err(GeometryError::InvalidParameter {
            name: "lab.progress/topology",
            value: progress,
            requirement: "B/C and 0..=1",
        });
    }
    if topology == Topology::C {
        let mut motion = crate::product_shared_envelope::SharedMotion::endpoint(progress == 1.0);
        for c in [
            &mut motion.horizontal,
            &mut motion.vertical,
            &mut motion.reveal,
        ] {
            c.value = progress;
            c.velocity = velocity;
        }
        return production_c_frame(contract, dock, motion);
    }
    let geometry = gnos_ux_surface::grammar::shared_envelope::SharedEnvelopeGeometry::resolve(
        gnos_ux_surface::grammar::shared_envelope::SharedEnvelopeInput {
            structural: contract.dock,
            painted_request: dock,
            safe: contract.safe,
            content_width: contract.desired_width,
            content_height: contract.desired_height,
            horizontal_progress: progress,
            vertical_progress: progress,
            radius: contract.radius,
        },
    )?;
    let outer = geometry.outer;
    let compact = geometry.compact;
    let body = geometry.content;
    let (content_allocation, clip) = content_bounds(contract, body)?;
    let inners = if topology == Topology::C {
        vec![
            SemanticInner {
                id: "DockInner",
                parent: "OuterEnvelope",
                bounds: compact,
                clip: compact,
                focus_group: "Dock controls",
            },
            SemanticInner {
                id: "AppStackInner",
                parent: "OuterEnvelope",
                bounds: body,
                clip,
                focus_group: "AppStack rows",
            },
        ]
    } else {
        vec![]
    };
    Ok(ComparisonFrame {
        topology,
        outer,
        dock: compact,
        body,
        content_allocation,
        clip,
        paint: PaintHull::Rounded(outer),
        input: geometry.input_regions(),
        progress,
        velocity,
        current: None,
        inners,
    })
}
fn rounded_input(r: GeometryRect) -> Vec<GeometryRect> {
    let mut regions = vec![];
    let mut y = r.top();
    while y < r.bottom() - 0.001 {
        let bottom = (y + 4.0).min(r.bottom());
        let clearance = (y - r.top()).min(r.bottom() - bottom).max(0.0);
        let radius = r.radius();
        let inset = if clearance >= radius {
            0.5
        } else {
            radius
                - (radius * radius - (radius - clearance).powi(2))
                    .max(0.0)
                    .sqrt()
                + 0.5
        };
        if r.width() > inset * 2.0 {
            regions.push(
                GeometryRect::try_new(r.left() + inset, y, r.right() - inset, bottom, 0.0).unwrap(),
            );
        }
        y = bottom;
    }
    regions
}

#[derive(Clone, Copy, Debug)]
pub struct SharedProgress {
    pub progress: f32,
    pub velocity: f32,
    pub target_open: bool,
}
impl Default for SharedProgress {
    fn default() -> Self {
        Self {
            progress: 0.0,
            velocity: 0.0,
            target_open: false,
        }
    }
}
impl SharedProgress {
    pub fn profile_json() -> Value {
        json!({"status":"LAB_EXPERIMENT_ONLY","stiffness":290.0,"damping":26.0,"max_dt_seconds":0.05,
        "settle":"first target crossing OR abs(position-target)<0.001 and abs(velocity)<0.01",
        "provenance":"adapted donor TC-301/306; not canonical tuning"})
    }
    pub fn settled(self) -> bool {
        self.progress == if self.target_open { 1.0 } else { 0.0 } && self.velocity == 0.0
    }
    pub fn step(&mut self, dt: f32, reduced: bool) -> Result<(), GeometryError> {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(GeometryError::NonFinite("lab.dt"));
        }
        let target = if self.target_open { 1.0 } else { 0.0 };
        if reduced {
            self.progress = target;
            self.velocity = 0.0;
            return Ok(());
        }
        let previous = self.progress;
        let dt = dt.min(0.05);
        self.velocity += ((target - self.progress) * 290.0 - self.velocity * 26.0) * dt;
        self.progress += self.velocity * dt;
        if (previous - target) * (self.progress - target) <= 0.0
            || ((self.progress - target).abs() < 0.001 && self.velocity.abs() < 0.01)
        {
            self.progress = target;
            self.velocity = 0.0;
        } else {
            self.progress = self.progress.clamp(0.0, 1.0);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MagProfile {
    pub strength: f32,
    pub spread: f32,
    pub response_seconds: f32,
}
impl MagProfile {
    pub const EXPERIMENT: Self = Self {
        strength: 0.32,
        spread: 0.85,
        response_seconds: 0.085,
    };
    pub fn json(self) -> Value {
        json!({"status":"LAB_EXPERIMENT_ONLY","strength":self.strength,"spread":self.spread,
        "sigma":(self.spread*1.25).max(1.0),"response_seconds":self.response_seconds,"item_base_size":44.0,"resting_slot_pitch":56.0,
        "influence":"index Gaussian","response":"1-exp(-dt/response_seconds)","hit_model":"exact painted rectangular GPUI control box"})
    }
    pub fn targets(self, hovered: Option<usize>) -> [f32; 5] {
        std::array::from_fn(|i| {
            hovered.map_or(1.0, |h| {
                let distance = i as f32 - h as f32;
                1.0 + self.strength
                    * (-distance.powi(2) / (2.0 * (self.spread * 1.25).max(1.0).powi(2))).exp()
            })
        })
    }
}
#[derive(Clone, Debug)]
pub struct ItemProjection {
    pub id: &'static str,
    pub resting: GeometryRect,
    pub painted: GeometryRect,
    pub scale: f32,
    pub displacement: f32,
    pub dragged: bool,
}
impl ItemProjection {
    pub fn json(&self) -> Value {
        json!({"id":self.id,"resting_bounds":edges(self.resting),"painted_bounds":edges(self.painted),
        "hit_bounds":edges(self.painted),"scale":self.scale,"neighbour_displacement_x":self.displacement,"dragged":self.dragged})
    }
}
pub const ITEM_NAMES: [&str; 5] = ["AppStack", "Files", "Firefox", "Notes", "Workspace"];
pub fn item_projection(
    contract: ComparisonContract,
    scales: [f32; 5],
    reorder: bool,
) -> Result<(GeometryRect, Vec<ItemProjection>), GeometryError> {
    contract.validate()?;
    if scales
        .iter()
        .any(|s| !s.is_finite() || *s < 1.0 || *s > 2.0)
    {
        return Err(GeometryError::NonFinite("lab.item.scale"));
    }
    let extra_width = scales.iter().map(|s| (s - 1.0) * 44.0).sum::<f32>();
    let extra_height = (scales.iter().copied().fold(1.0, f32::max) - 1.0) * 44.0;
    let cx = (contract.dock.left() + contract.dock.right()) * 0.5;
    let dock = centered(
        cx,
        contract.dock.top() - extra_height,
        contract.dock.width() + extra_width,
        contract.dock.height() + extra_height,
        contract.radius,
    )?;
    if dock.left() < contract.safe.left() || dock.right() > contract.safe.right() {
        return Err(GeometryError::InvalidClamp {
            min: contract.safe.left(),
            max: contract.safe.right(),
        });
    }
    let resting_width = 5.0 * 44.0 + 4.0 * 12.0;
    let padding = (contract.dock.width() - resting_width) * 0.5;
    let mut x = dock.left() + padding;
    let mut items = Vec::new();
    for (i, scale) in scales.iter().enumerate() {
        let rest_x = contract.dock.left() + padding + i as f32 * 56.0;
        let rest_y = (contract.dock.top() + contract.dock.bottom() - 44.0) * 0.5;
        let resting = GeometryRect::try_new(rest_x, rest_y, rest_x + 44.0, rest_y + 44.0, 0.0)?;
        let size = 44.0 * scale;
        let reorder_offset = if reorder && (2..=3).contains(&i) {
            -56.0
        } else if reorder && i == 1 {
            112.0
        } else {
            0.0
        };
        let painted = GeometryRect::try_new(
            x + reorder_offset,
            dock.bottom() - (dock.height() + size) * 0.5,
            x + size + reorder_offset,
            dock.bottom() - (dock.height() - size) * 0.5,
            0.0,
        )?;
        items.push(ItemProjection {
            id: ITEM_NAMES[i],
            resting,
            painted,
            scale: *scale,
            displacement: painted.left() - resting.left(),
            dragged: reorder && i == 1,
        });
        x += size + 12.0;
    }
    Ok((dock, items))
}
pub fn hit_item(items: &[ItemProjection], x: f32, y: f32) -> Option<&'static str> {
    items
        .iter()
        .rev()
        .find(|item| item.painted.contains_point(x, y))
        .map(|item| item.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contract() -> ComparisonContract {
        ComparisonContract::current(SurfaceViewport::try_new(1280.0, 800.0).unwrap()).unwrap()
    }
    #[test]
    fn c_lab_and_production_share_exact_geometry_clip_and_input() {
        let c = contract();
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut m = crate::product_shared_envelope::SharedMotion::endpoint(p == 1.0);
            for ch in [&mut m.horizontal, &mut m.vertical, &mut m.reveal] {
                ch.value = p;
            }
            let lab = production_c_frame(c, c.dock, m).unwrap();
            let prod = crate::product_shared_envelope::SharedDockEnvelope::resolve(
                c.viewport, c.dock, c.dock, m,
            )
            .unwrap();
            assert_eq!(lab.outer, prod.geometry.outer);
            assert_eq!(lab.body, prod.appstack.bounds);
            assert_eq!(lab.clip, prod.content_clip);
            assert_eq!(lab.input, prod.input_regions());
        }
    }
    #[test]
    fn common_dimensions_are_the_unchanged_current_endpoint() {
        let c = contract();
        let mut solver = None;
        let a = current_frame(c, c.dock, &mut solver, true, true).unwrap();
        assert_eq!(a.body.width(), c.desired_width);
        assert_eq!(a.body.height(), c.desired_height);
        for t in [Topology::B, Topology::C] {
            let f = shared_frame(c, c.dock, 1.0, 0.0, t).unwrap();
            assert_eq!(f.body.width(), c.desired_width);
            assert!((f.body.height() - c.desired_height).abs() < 0.001);
            assert_eq!(f.content_allocation.width(), a.content_allocation.width());
            assert_eq!(f.content_allocation.height(), a.content_allocation.height());
        }
        let measurement = a.json(c, &[], 0.0, 0.0);
        assert_eq!(measurement["self_intersections"], 0);
        assert_eq!(measurement["input_inside_paint"], true);
        assert_eq!(measurement["body_coverage"], 1.0);
        assert_eq!(measurement["relation"]["attachment"], "Fused");
    }
    #[test]
    fn shared_formula_baseline_abutment_and_single_owner() {
        let c = contract();
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let f = shared_frame(c, c.dock, p, 0.0, Topology::B).unwrap();
            assert_eq!(f.outer.width(), c.dock.width().max(c.desired_width * p));
            assert!((f.outer.height() - (c.dock.height() + c.desired_height * p)).abs() < 0.001);
            assert_eq!(f.outer.bottom(), c.dock.bottom());
            assert_eq!(f.body.bottom(), f.dock.top());
            let j = f.json(c, &[], 0.0, 0.0);
            assert_eq!(j["material_owner_count"], 1);
            assert_eq!(j["input_inside_paint"], true);
        }
    }
    #[test]
    fn c_has_distinct_typed_inners_and_identical_b_outer() {
        let c = contract();
        for p in [0.0, 0.5, 1.0] {
            let b = shared_frame(c, c.dock, p, 0.0, Topology::B).unwrap();
            let cframe = shared_frame(c, c.dock, p, 0.0, Topology::C).unwrap();
            assert_eq!(b.outer, cframe.outer);
            assert_eq!(b.input, cframe.input);
            assert_eq!(b.body, cframe.body);
            assert!(b.inners.is_empty());
            assert_ne!(cframe.inners[0].id, cframe.inners[1].id);
            assert_eq!(cframe.inners[0].parent, "SharedDockEnvelope");
            assert_eq!(cframe.inners[1].parent, "SharedDockEnvelope");
        }
    }
    #[test]
    fn hostile_progress_and_scales_fail_closed() {
        let c = contract();
        for p in [f32::NAN, f32::INFINITY, -1.0, 2.0] {
            assert!(shared_frame(c, c.dock, p, 0.0, Topology::B).is_err());
        }
        assert!(item_projection(c, [f32::NAN; 5], false).is_err());
        let hostile = ComparisonContract {
            desired_width: f32::NAN,
            ..c
        };
        assert!(shared_frame(hostile, c.dock, 1.0, 0.0, Topology::B).is_err());
        assert!(current_frame(hostile, c.dock, &mut None, true, true).is_err());
        assert!(item_projection(hostile, [1.0; 5], false).is_err());
    }
    #[test]
    fn shared_x_expansion_and_safe_capacity_are_explicit() {
        let c = ComparisonContract {
            desired_width: 900.0,
            desired_height: 1200.0,
            ..contract()
        };
        let f = shared_frame(c, c.dock, 1.0, 0.0, Topology::B).unwrap();
        assert_eq!(f.outer.width(), 900.0);
        assert_eq!(f.outer.height(), c.safe.height());
        assert_eq!(f.outer.bottom(), c.dock.bottom());
        assert_eq!(f.body.bottom(), f.dock.top());
    }
    #[test]
    fn gaussian_layout_structural_minimum_and_painted_hits() {
        let c = contract();
        let profile = MagProfile::EXPERIMENT;
        let targets = profile.targets(Some(2));
        assert!((targets[2] - 1.32).abs() < 0.000001);
        assert_eq!(targets[1], targets[3]);
        assert_eq!(targets[0], targets[4]);
        let (dock, items) = item_projection(c, targets, false).unwrap();
        assert!(dock.width() > c.dock.width());
        assert!(dock.height() > c.dock.height());
        assert_eq!(c.dock, contract().dock);
        assert!(items[0].displacement.abs() > 0.01);
        for item in &items {
            assert_eq!(
                hit_item(
                    &items,
                    (item.painted.left() + item.painted.right()) * 0.5,
                    (item.painted.top() + item.painted.bottom()) * 0.5
                ),
                Some(item.id)
            );
        }
    }
    #[test]
    fn reorder_moves_one_slot_and_retains_outer_extent() {
        let c = contract();
        let (dock, plain) = item_projection(c, [1.0; 5], false).unwrap();
        let (other, moved) = item_projection(c, [1.0; 5], true).unwrap();
        assert_eq!(dock, other);
        for i in [2, 3] {
            assert_eq!(moved[i].painted.left(), plain[i].painted.left() - 56.0);
        }
        for t in [Topology::B, Topology::C] {
            assert_eq!(
                shared_frame(c, dock, 1.0, 0.0, t).unwrap().outer,
                shared_frame(c, other, 1.0, 0.0, t).unwrap().outer
            );
        }
    }
    #[test]
    fn shared_motion_reversal_keeps_pose_and_reduced_endpoints() {
        let mut motion = SharedProgress {
            target_open: true,
            ..Default::default()
        };
        for _ in 0..5 {
            motion.step(1.0 / 60.0, false).unwrap();
        }
        let (p, v) = (motion.progress, motion.velocity);
        motion.target_open = false;
        assert_eq!((motion.progress, motion.velocity), (p, v));
        for _ in 0..200 {
            motion.step(1.0 / 60.0, false).unwrap();
        }
        assert!(motion.settled());
        motion.target_open = true;
        motion.step(0.016, true).unwrap();
        assert_eq!(motion.progress, 1.0);
        assert!(motion.settled());
    }
}
