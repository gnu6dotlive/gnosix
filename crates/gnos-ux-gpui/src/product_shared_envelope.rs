//! Product C: one material owner and two semantic inner regions. Geometry is
//! backend independent; motion advances analytically from elapsed wall time.
use gnos_ux_surface::grammar::shared_envelope::{SharedEnvelopeGeometry, SharedEnvelopeInput};
use gnos_ux_surface::grammar::{GeometryError, GeometryRect, SurfaceViewport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InnerId {
    DockInner,
    AppStackInner,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialOwner {
    SharedDockEnvelope,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InnerRole {
    DockControls,
    ApplicationList,
}
#[derive(Clone, Copy, Debug)]
pub struct InnerSurface {
    pub id: InnerId,
    pub parent: MaterialOwner,
    pub role: InnerRole,
    pub bounds: GeometryRect,
    pub clip: GeometryRect,
    pub visible: bool,
    pub interactable: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SharedDockEnvelope {
    pub geometry: SharedEnvelopeGeometry,
    pub dock: InnerSurface,
    pub appstack: InnerSurface,
    pub content_allocation: GeometryRect,
    pub content_clip: GeometryRect,
    pub motion: SharedMotion,
    pub material_owner: MaterialOwner,
}
impl SharedDockEnvelope {
    pub fn resolve(
        viewport: SurfaceViewport,
        structural: GeometryRect,
        painted: GeometryRect,
        motion: SharedMotion,
    ) -> Result<Self, GeometryError> {
        let safe =
            GeometryRect::try_new(16.0, 16.0, viewport.width - 16.0, structural.bottom(), 0.0)?;
        Self::resolve_with_safe(viewport, structural, painted, motion, safe)
    }
    /// Same bottom-anchored owner, with scene-derived available space.
    pub fn resolve_with_safe(
        viewport: SurfaceViewport,
        structural: GeometryRect,
        painted: GeometryRect,
        motion: SharedMotion,
        safe: GeometryRect,
    ) -> Result<Self, GeometryError> {
        viewport.validate()?;
        if safe.left() < 0.
            || safe.top() < 0.
            || safe.right() > viewport.width
            || safe.bottom() > viewport.height
        {
            return Err(GeometryError::InvalidParameter {
                name: "shared.safe",
                value: safe.width(),
                requirement: "inside viewport",
            });
        }
        for channel in [motion.horizontal, motion.vertical, motion.reveal] {
            gnos_ux_surface::grammar::UnitInterval::try_new(channel.value)?;
            if !channel.velocity.is_finite() {
                return Err(GeometryError::NonFinite("shared.motion.velocity"));
            }
        }
        // Preserve the current product content dimensions; no donor constants.
        let desired_width = structural.width() * 0.65;
        let desired_height = 156.0;
        let geometry = SharedEnvelopeGeometry::resolve(SharedEnvelopeInput {
            structural,
            painted_request: painted,
            safe,
            content_width: desired_width,
            content_height: desired_height,
            horizontal_progress: motion.horizontal.value,
            vertical_progress: motion.vertical.value,
            radius: structural.radius(),
        })?;
        let inset = structural.radius() * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
        let allocation_width = (desired_width.min(geometry.outer.width()) - 2.0 * inset).max(0.0);
        let cx = (geometry.outer.left() + geometry.outer.right()) * 0.5;
        let content_allocation = GeometryRect::try_new(
            cx - allocation_width * 0.5,
            geometry.content.top() + inset,
            cx + allocation_width * 0.5,
            geometry.content.top() + inset + (desired_height - 2.0 * inset).max(0.0),
            0.0,
        )?;
        let left = content_allocation
            .left()
            .max(geometry.content.left() + inset);
        let right = content_allocation
            .right()
            .min(geometry.content.right() - inset)
            .max(left);
        let top = content_allocation.top();
        let bottom = content_allocation
            .bottom()
            .min(geometry.content.bottom() - inset)
            .min(top + (desired_height - 2.0 * inset) * motion.reveal.value)
            .max(top);
        let content_clip = GeometryRect::try_new(left, top, right, bottom, 0.0)?;
        let visible = content_clip.width() > 0.0 && content_clip.height() > 0.0;
        // Retain identity throughout retraction. Rows become inert as soon as
        // the product closes, and enter traversal only when fully revealed.
        let ready = visible
            && motion.target_open
            && [motion.horizontal, motion.vertical, motion.reveal]
                .iter()
                .all(|channel| channel.value >= 0.995);
        let dock = InnerSurface {
            id: InnerId::DockInner,
            role: InnerRole::DockControls,
            parent: MaterialOwner::SharedDockEnvelope,
            bounds: geometry.compact,
            clip: geometry.compact,
            visible: true,
            interactable: true,
        };
        let appstack = InnerSurface {
            id: InnerId::AppStackInner,
            role: InnerRole::ApplicationList,
            parent: MaterialOwner::SharedDockEnvelope,
            bounds: geometry.content,
            clip: content_clip,
            visible,
            interactable: ready,
        };
        Ok(Self {
            geometry,
            dock,
            appstack,
            content_allocation,
            content_clip,
            motion,
            material_owner: MaterialOwner::SharedDockEnvelope,
        })
    }
    /// TOPO-01C2b: the same owner hanging from the top of the output (a
    /// bottom Bar sends the Dock to the top). `structural` and `safe` are
    /// top-anchored; the bottom-anchored grammar resolves their mirror image
    /// and the result is mirrored back, so the AppStack opens downward. The
    /// list is revealed from the Dock outward, as it is from the Dock upward
    /// at the bottom.
    pub fn resolve_top_anchored(
        viewport: SurfaceViewport,
        structural: GeometryRect,
        motion: SharedMotion,
        safe: GeometryRect,
    ) -> Result<Self, GeometryError> {
        let h = viewport.height;
        let flipped = Self::resolve_with_safe(
            viewport,
            mirror(structural, h)?,
            mirror(structural, h)?,
            motion,
            mirror(safe, h)?,
        )?;
        let mut out = flipped.mirrored(h)?;
        // Reveal from the Dock side: keep the revealed height, anchored at the
        // allocation's edge nearest the Dock.
        let clip = out.content_clip;
        let top = out.content_allocation.top();
        let bottom = (top + clip.height())
            .min(out.geometry.content.bottom())
            .max(top);
        out.content_clip = GeometryRect::try_new(clip.left(), top, clip.right(), bottom, 0.0)?;
        out.appstack.clip = out.content_clip;
        Ok(out)
    }

    fn mirrored(self, height: f32) -> Result<Self, GeometryError> {
        let m = |r: GeometryRect| mirror(r, height);
        let mut geometry = self.geometry;
        geometry.outer = m(geometry.outer)?;
        geometry.compact = m(geometry.compact)?;
        geometry.content = m(geometry.content)?;
        geometry.structural = m(geometry.structural)?;
        geometry.painted_request = m(geometry.painted_request)?;
        let inner = |mut i: InnerSurface| -> Result<InnerSurface, GeometryError> {
            i.bounds = m(i.bounds)?;
            i.clip = m(i.clip)?;
            Ok(i)
        };
        Ok(Self {
            geometry,
            dock: inner(self.dock)?,
            appstack: inner(self.appstack)?,
            content_allocation: m(self.content_allocation)?,
            content_clip: m(self.content_clip)?,
            ..self
        })
    }

    pub fn endpoint(
        viewport: SurfaceViewport,
        dock: GeometryRect,
        open: bool,
    ) -> Result<Self, GeometryError> {
        Self::resolve(viewport, dock, dock, SharedMotion::endpoint(open))
    }
    pub fn input_regions(self) -> Vec<GeometryRect> {
        self.geometry.input_regions()
    }
    pub fn inner_input(self, id: InnerId) -> Vec<GeometryRect> {
        let inner = match id {
            InnerId::DockInner => self.dock,
            InnerId::AppStackInner => self.appstack,
        };
        if !inner.interactable {
            return Vec::new();
        }
        self.input_regions()
            .into_iter()
            .filter_map(|r| {
                let (l, t, right, b) = (
                    r.left().max(inner.clip.left()),
                    r.top().max(inner.clip.top()),
                    r.right().min(inner.clip.right()),
                    r.bottom().min(inner.clip.bottom()),
                );
                if right > l && b > t {
                    GeometryRect::try_new(l, t, right, b, 0.0).ok()
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn contains_input(self, x: f32, y: f32) -> bool {
        self.input_regions().iter().any(|r| r.contains_point(x, y))
    }
}

/// The rectangle mirrored about the output's horizontal centre line.
fn mirror(r: GeometryRect, height: f32) -> Result<GeometryRect, GeometryError> {
    GeometryRect::try_new(
        r.left(),
        height - r.bottom(),
        r.right(),
        height - r.top(),
        r.radius(),
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExtentChannel {
    pub value: f32,
    pub velocity: f32,
}
impl ExtentChannel {
    fn step(&mut self, target: f32, omega: f32, dt: f32) {
        // Closed-form critically damped response, invariant to render cadence.
        let (target, omega, dt) = (f64::from(target), f64::from(omega), f64::from(dt));
        let x = f64::from(self.value) - target;
        let c = f64::from(self.velocity) + omega * x;
        let decay = (-omega * dt).exp();
        self.value = (target + (x + c * dt) * decay) as f32;
        self.velocity = ((f64::from(self.velocity) - omega * c * dt) * decay) as f32;
        if self.value < 0.0 || self.value > 1.0 {
            self.value = self.value.clamp(0.0, 1.0);
            self.velocity = 0.0;
        }
        if (f64::from(self.value) - target).abs() < 0.0005 && self.velocity.abs() < 0.01 {
            self.value = target as f32;
            self.velocity = 0.0;
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SharedMotion {
    pub horizontal: ExtentChannel,
    pub vertical: ExtentChannel,
    pub reveal: ExtentChannel,
    pub target_open: bool,
}
impl SharedMotion {
    pub fn endpoint(open: bool) -> Self {
        let channel = ExtentChannel {
            value: if open { 1.0 } else { 0.0 },
            velocity: 0.0,
        };
        Self {
            horizontal: channel,
            vertical: channel,
            reveal: channel,
            target_open: open,
        }
    }
    pub fn advance(
        &mut self,
        target_open: bool,
        reduced: bool,
        dt: f32,
    ) -> Result<(), GeometryError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "shared.motion.dt",
                value: dt,
                requirement: "finite nonnegative wall-clock seconds",
            });
        }
        for ch in [self.horizontal, self.vertical, self.reveal] {
            gnos_ux_surface::grammar::UnitInterval::try_new(ch.value)?;
            if !ch.velocity.is_finite() {
                return Err(GeometryError::NonFinite("shared.motion.velocity"));
            }
        }
        self.target_open = target_open;
        if reduced {
            *self = Self::endpoint(target_open);
            return Ok(());
        }
        let target = if target_open { 1.0 } else { 0.0 };
        // Initial consumer profile, not a shared-solver or donor default.
        self.horizontal.step(target, 20.0, dt);
        self.vertical.step(target, 20.0, dt);
        self.reveal.step(target, 22.0, dt);
        Ok(())
    }
    /// Frozen inspection of the production opening trajectory, not a second
    /// motion profile. Endpoints are exact; intermediate values retain velocity.
    pub fn opening_sample(progress: f32) -> Result<Self, GeometryError> {
        gnos_ux_surface::grammar::UnitInterval::try_new(progress)?;
        if progress == 0.0 {
            return Ok(Self::endpoint(false));
        }
        if progress == 1.0 {
            return Ok(Self::endpoint(true));
        }
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..30 {
            let t = (lo + hi) * 0.5;
            let mut m = Self::default();
            m.advance(true, false, t)?;
            if m.vertical.value < progress {
                lo = t;
            } else {
                hi = t;
            }
        }
        let mut m = Self::default();
        m.advance(true, false, (lo + hi) * 0.5)?;
        Ok(m)
    }
    /// First render of a retarget must not charge time spent idle to the new
    /// target. A zero-time retarget preserves position AND velocity.
    pub fn advance_observed(
        &mut self,
        open: bool,
        reduced: bool,
        elapsed: f32,
    ) -> Result<(), GeometryError> {
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "shared.motion.elapsed",
                value: elapsed,
                requirement: "finite nonnegative elapsed time",
            });
        }
        let dt = if self.target_open != open {
            0.0
        } else {
            elapsed
        };
        self.advance(open, reduced, dt)
    }
    pub fn needs_frame(&self, open: bool, reduced: bool) -> bool {
        !reduced && !self.is_settled_for(open)
    }
    pub fn is_settled_for(&self, target_open: bool) -> bool {
        let target = if target_open { 1.0 } else { 0.0 };
        [self.horizontal, self.vertical, self.reveal]
            .iter()
            .all(|c| c.value == target && c.velocity == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(motion: SharedMotion) -> SharedDockEnvelope {
        let v = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let d = crate::product_scene::ProductLayout::resolved_dock(v).unwrap();
        SharedDockEnvelope::resolve(v, d, d, motion).unwrap()
    }
    #[test]
    fn idle_time_is_not_consumed_by_a_new_target_and_driver_follows_both_directions() {
        let mut m = SharedMotion::default();
        assert!(m.needs_frame(true, false));
        m.advance_observed(true, false, 3600.0).unwrap();
        assert_eq!(m.vertical.value, 0.0);
        m.advance_observed(true, false, 0.1).unwrap();
        assert!(m.vertical.value > 0.0 && m.vertical.value < 1.0);
        let before = m;
        m.advance_observed(false, false, 3600.0).unwrap();
        assert_eq!(m.vertical.value, before.vertical.value);
        assert_eq!(m.vertical.velocity, before.vertical.velocity);
        assert!(m.needs_frame(false, false));
        m.advance_observed(false, true, 0.0).unwrap();
        assert!(!m.needs_frame(false, false));
        assert!(!m.needs_frame(true, true));
    }
    #[cfg(feature = "component-lab")]
    #[test]
    fn real_fake_launch_closes_and_refusal_keeps_semantic_inner_visible() {
        use crate::product_scene::ProductPanelState;
        let mut normal = crate::product_lab::fixture("appstack-basic").unwrap();
        let runtime = &mut normal.outputs[0].runtime;
        let selected = runtime.installed_applications().remove(0);
        let record = crate::product_launch::launch_test_application(runtime, &selected);
        assert!(record.terminal.as_ref().unwrap().is_launched());
        assert_eq!(
            ProductPanelState::AppStack.after_launch(&record),
            ProductPanelState::Closed
        );
        let refused = crate::product_lab::fixture("appstack-refused").unwrap();
        let crate::product_runtime::ProductLaunchState::Terminal(record) =
            refused.outputs[0].runtime.launch_state()
        else {
            panic!("real refusal fixture")
        };
        assert_eq!(
            ProductPanelState::AppStack.after_launch(record),
            ProductPanelState::AppStack
        );
        assert!(fixture(SharedMotion::endpoint(true)).appstack.visible);
    }
    #[test]
    fn closed_and_open_keep_distinct_semantics_with_one_material_owner() {
        let closed = fixture(SharedMotion::endpoint(false));
        assert_eq!(closed.geometry.outer, closed.geometry.structural);
        assert!(!closed.appstack.visible);
        assert!(!closed.appstack.interactable);
        let open = fixture(SharedMotion::endpoint(true));
        assert_ne!(open.dock.id, open.appstack.id);
        assert_eq!(open.dock.parent, open.appstack.parent);
        assert_eq!(open.appstack.bounds.bottom(), open.dock.bounds.top());
        assert!(open.appstack.interactable);
        let mut revealing = SharedMotion::endpoint(true);
        revealing.vertical.value = 0.2;
        assert!(!fixture(revealing).appstack.interactable);
    }
    #[test]
    fn wall_clock_response_reversal_reduced_and_input_containment() {
        let mut fine = SharedMotion::default();
        let mut coarse = SharedMotion::default();
        for _ in 0..20 {
            fine.advance(true, false, 0.01).unwrap();
        }
        coarse.advance(true, false, 0.2).unwrap();
        assert!((fine.vertical.value - coarse.vertical.value).abs() < 0.00001);
        let before = coarse;
        coarse.advance(false, false, 0.0).unwrap();
        assert_eq!(coarse.vertical.value, before.vertical.value);
        assert_eq!(coarse.vertical.velocity, before.vertical.velocity);
        for open in [true, false, true, false] {
            for _ in 0..90 {
                coarse.advance(open, false, 1.0 / 60.0).unwrap();
                let f = fixture(coarse);
                for r in f.input_regions() {
                    for x in [r.left(), r.right()] {
                        for y in [r.top(), r.bottom()] {
                            assert!(f.geometry.contains_paint(x, y));
                        }
                    }
                }
                if !open {
                    assert!(!f.appstack.interactable);
                }
            }
            assert!(coarse.is_settled_for(open));
        }
        coarse.advance(true, true, 0.0).unwrap();
        assert!(coarse.is_settled_for(true));
        assert_eq!(
            fixture(coarse).geometry,
            fixture(SharedMotion::endpoint(true)).geometry
        );
    }
}

#[cfg(test)]
mod available_region_tests {
    use super::*;
    #[test]
    fn a_top_anchored_owner_opens_downward_and_reveals_from_the_dock() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let dock = GeometryRect::try_new(380.0, 16.0, 900.0, 80.0, 32.0).unwrap();
        let safe = GeometryRect::try_new(16.0, 16.0, 1264.0, 724.0, 0.0).unwrap();
        let open = SharedDockEnvelope::resolve_top_anchored(
            viewport,
            dock,
            SharedMotion::endpoint(true),
            safe,
        )
        .unwrap();
        let g = open.geometry;
        assert_eq!(g.outer.top(), dock.top(), "hangs from the Dock's place");
        assert!(g.outer.bottom() > dock.bottom() + 100.0, "opens downward");
        assert!(
            g.content.top() >= dock.bottom() - 0.01,
            "the list lies below the Dock"
        );
        assert_eq!(open.dock.bounds, g.compact);
        assert!(open.appstack.interactable);
        let mid = SharedDockEnvelope::resolve_top_anchored(
            viewport,
            dock,
            SharedMotion::opening_sample(0.5).unwrap(),
            safe,
        )
        .unwrap();
        assert_eq!(
            mid.content_clip.top(),
            mid.content_allocation.top(),
            "revealed from the Dock side"
        );
        let closed = SharedDockEnvelope::resolve_top_anchored(
            viewport,
            dock,
            SharedMotion::endpoint(false),
            safe,
        )
        .unwrap();
        assert_eq!(closed.geometry.outer.top(), dock.top());
        assert!(!closed.appstack.interactable);
    }

    #[test]
    fn scene_constraint_cannot_escape_the_viewport() {
        let viewport = SurfaceViewport::try_new(1280., 800.).unwrap();
        let dock = GeometryRect::try_new(380., 720., 900., 784., 24.).unwrap();
        let outside = GeometryRect::try_new(-1., 16., 1264., 784., 0.).unwrap();
        assert!(
            SharedDockEnvelope::resolve_with_safe(
                viewport,
                dock,
                dock,
                SharedMotion::endpoint(true),
                outside
            )
            .is_err()
        );
        let safe = GeometryRect::try_new(16., 16., 1264., 784., 0.).unwrap();
        let a = SharedDockEnvelope::resolve(viewport, dock, dock, SharedMotion::endpoint(true))
            .unwrap();
        let b = SharedDockEnvelope::resolve_with_safe(
            viewport,
            dock,
            dock,
            SharedMotion::endpoint(true),
            safe,
        )
        .unwrap();
        assert_eq!(a.geometry.outer, b.geometry.outer);
        assert_eq!(a.content_clip, b.content_clip);
        assert_eq!(a.dock.id, b.dock.id);
        assert_eq!(a.appstack.id, b.appstack.id);
    }
}
