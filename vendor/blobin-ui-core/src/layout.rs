//! UI-G03: deterministic, headless logical layout and clip propagation.
//!
//! All coordinates are logical pixels. Physical coverage is computed from
//! logical edges, not individually rounded widths (avoids cumulative seams).
use crate::{NodeId, SurfaceId, UiSnapshot};
use std::collections::BTreeMap;
use std::fmt;

/// Finite logical extents. Width/height never contain negative values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}
impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Half-open rectangle [x, x+width) × [y, y+height), in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width).max(x);
        let bottom = (self.y + self.height).min(other.y + other.height).max(y);
        Self::new(x, y, right - x, bottom - y)
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x.is_finite()
            && y.is_finite()
            && x >= self.x
            && y >= self.y
            && x < self.x + self.width
            && y < self.y + self.height
    }
}

/// Pixel coverage (exclusive right/bottom); not a raster/paint implementation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl PixelRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && (x as i64) < (self.x as i64 + self.width as i64)
            && (y as i64) < (self.y as i64 + self.height as i64)
    }
}
fn pixel_coverage(rect: Rect, scale: f32) -> PixelRect {
    if rect.width == 0.0 || rect.height == 0.0 {
        return PixelRect::default();
    }
    let x = (rect.x * scale).floor() as i32;
    let y = (rect.y * scale).floor() as i32;
    let right = ((rect.x + rect.width) * scale).ceil() as i32;
    let bottom = ((rect.y + rect.height) * scale).ceil() as i32;
    PixelRect {
        x,
        y,
        width: (right - x).max(0) as u32,
        height: (bottom - y).max(0) as u32,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// Auto intrinsic size, fixed logical pixels, or a fraction of parent's content
/// extent. Fraction is constrained to [0, 1], not CSS flex-grow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Fraction(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Insets {
    pub const fn uniform(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
}

/// Per-component layout inputs. No accessibility, input dispatch, or rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub width: Length,
    pub height: Length,
    pub min: Size,
    pub max: Size,
    pub padding: Insets,
    pub gap: f32,
    pub axis: Axis,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub stretch_cross: bool,
    pub clip_children: bool,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            min: Size::new(0.0, 0.0),
            max: Size::new(100_000.0, 100_000.0),
            padding: Insets::default(),
            gap: 0.0,
            axis: Axis::Column,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            stretch_cross: true,
            clip_children: false,
        }
    }
}
impl Style {
    pub fn fill() -> Self {
        Self {
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Self::default()
        }
    }
    fn check(&self) -> Result<(), LayoutError> {
        fn finite_nonnegative(value: f32) -> bool {
            value.is_finite() && (0.0..=100_000.0).contains(&value)
        }
        for value in [
            self.min.width,
            self.min.height,
            self.max.width,
            self.max.height,
            self.padding.left,
            self.padding.top,
            self.padding.right,
            self.padding.bottom,
            self.gap,
            self.flex_grow,
            self.flex_shrink,
        ] {
            if !finite_nonnegative(value) {
                return Err(LayoutError::InvalidStyle);
            }
        }
        if self.min.width > self.max.width || self.min.height > self.max.height {
            return Err(LayoutError::InvalidStyle);
        }
        for value in [self.width, self.height] {
            match value {
                Length::Auto => {}
                Length::Px(x) if finite_nonnegative(x) => {}
                Length::Fraction(x) if x.is_finite() && (0.0..=1.0).contains(&x) => {}
                _ => return Err(LayoutError::InvalidStyle),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub logical: Size,
    pub scale_factor: f32,
    /// Host-owned monotonic resize / scale event generation.
    pub geometry_epoch: u64,
}
impl Viewport {
    pub fn new(logical: Size, scale_factor: f32, geometry_epoch: u64) -> Self {
        Self {
            logical,
            scale_factor,
            geometry_epoch,
        }
    }
    fn check(self) -> Result<(), LayoutError> {
        if !self.logical.width.is_finite()
            || !self.logical.height.is_finite()
            || self.logical.width < 0.0
            || self.logical.height < 0.0
            || self.logical.width > 100_000.0
            || self.logical.height > 100_000.0
            || !self.scale_factor.is_finite()
            || !(0.125..=16.0).contains(&self.scale_factor)
        {
            return Err(LayoutError::InvalidViewport);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    UnknownSurface(SurfaceId),
    UnknownNode(NodeId),
    InvalidStyle,
    InvalidViewport,
    DepthLimit,
    NonFiniteGeometry,
    RevisionExhausted,
}
impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LayoutError {}

/// Per-surface style revisions avoid invalidating an unrelated window.
#[derive(Default)]
pub struct LayoutStyles {
    overrides: BTreeMap<NodeId, Style>,
    revisions: BTreeMap<SurfaceId, u64>,
}
impl LayoutStyles {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn revision(&self, surface: SurfaceId) -> u64 {
        *self.revisions.get(&surface).unwrap_or(&0)
    }
    pub fn set(
        &mut self,
        snapshot: &UiSnapshot,
        id: NodeId,
        style: Style,
    ) -> Result<bool, LayoutError> {
        style.check()?;
        let node = snapshot.get(id).ok_or(LayoutError::UnknownNode(id))?;
        if self.overrides.get(&id) == Some(&style) {
            return Ok(false);
        }
        let next = self
            .revision(node.surface())
            .checked_add(1)
            .ok_or(LayoutError::RevisionExhausted)?;
        self.overrides.insert(id, style);
        self.revisions.insert(node.surface(), next);
        Ok(true)
    }
    pub fn remove(&mut self, snapshot: &UiSnapshot, id: NodeId) -> Result<bool, LayoutError> {
        let node = snapshot.get(id).ok_or(LayoutError::UnknownNode(id))?;
        if !self.overrides.contains_key(&id) {
            return Ok(false);
        }
        let next = self
            .revision(node.surface())
            .checked_add(1)
            .ok_or(LayoutError::RevisionExhausted)?;
        self.overrides.remove(&id);
        self.revisions.insert(node.surface(), next);
        Ok(true)
    }
    fn style(&self, id: NodeId) -> Style {
        self.overrides.get(&id).cloned().unwrap_or_default()
    }
}

/// Geometry only. visible is the node's own intersection with ancestor clips;
/// child clips are inherited only when its parent sets clip_children.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutBox {
    pub rect: Rect,
    pub visible: Rect,
    pub physical: PixelRect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutStamp {
    pub surface: SurfaceId,
    pub surface_epoch: u64,
    pub surface_revision: u64,
    pub ui_revision_at_compute: u64,
    pub style_revision: u64,
    pub geometry_epoch: u64,
    pub viewport_bits: [u32; 3],
}

#[derive(Clone, Debug)]
pub struct LayoutSnapshot {
    pub stamp: LayoutStamp,
    pub viewport: Viewport,
    pub boxes: BTreeMap<NodeId, LayoutBox>,
    /// Topological preorder, sibling insertion order; G04 will add hit routing.
    pub paint_order: Vec<NodeId>,
}
impl LayoutSnapshot {
    /// A changed unrelated surface must not invalidate this surface geometry.
    pub fn is_current(&self, tree: &UiSnapshot, styles: &LayoutStyles, viewport: Viewport) -> bool {
        self.stamp.surface_epoch == tree.surface_epoch(self.stamp.surface).unwrap_or(0)
            && self.stamp.surface_revision
                == tree
                    .surface_revision(self.stamp.surface)
                    .unwrap_or(u64::MAX)
            && self.stamp.style_revision == styles.revision(self.stamp.surface)
            && self.stamp.geometry_epoch == viewport.geometry_epoch
            && self.stamp.viewport_bits
                == [
                    viewport.logical.width.to_bits(),
                    viewport.logical.height.to_bits(),
                    viewport.scale_factor.to_bits(),
                ]
    }
    pub fn get(&self, id: NodeId) -> Option<&LayoutBox> {
        self.boxes.get(&id)
    }
}

const MAX_DEPTH: usize = 256;
fn resolve(length: Length, available: f32, intrinsic: f32, min: f32, max: f32) -> f32 {
    let value = match length {
        Length::Auto => intrinsic,
        Length::Px(v) => v,
        Length::Fraction(f) => available * f,
    };
    value.max(min).min(max)
}
fn main_extent(size: Size, axis: Axis) -> f32 {
    match axis {
        Axis::Row => size.width,
        Axis::Column => size.height,
    }
}
fn cross_extent(size: Size, axis: Axis) -> f32 {
    match axis {
        Axis::Row => size.height,
        Axis::Column => size.width,
    }
}

struct LayoutRun<'a> {
    tree: &'a UiSnapshot,
    styles: &'a LayoutStyles,
    viewport: Viewport,
    boxes: BTreeMap<NodeId, LayoutBox>,
    paint_order: Vec<NodeId>,
}
impl LayoutRun<'_> {
    fn measure(&self, id: NodeId, available: Size, depth: usize) -> Result<Size, LayoutError> {
        if depth > MAX_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        let node = self.tree.get(id).ok_or(LayoutError::UnknownNode(id))?;
        let style = self.styles.style(id);
        let inner = Size::new(
            (available.width - style.padding.left - style.padding.right).max(0.0),
            (available.height - style.padding.top - style.padding.bottom).max(0.0),
        );
        let mut natural_main: f32 = 0.0;
        let mut natural_cross: f32 = 0.0;
        for (i, child) in node.children().iter().enumerate() {
            let child_size = self.measure(*child, inner, depth + 1)?;
            if i > 0 {
                natural_main += style.gap;
            }
            natural_main += main_extent(child_size, style.axis);
            natural_cross = natural_cross.max(cross_extent(child_size, style.axis));
        }
        let (natural_w, natural_h) = match style.axis {
            Axis::Row => (natural_main, natural_cross),
            Axis::Column => (natural_cross, natural_main),
        };
        let natural = Size::new(
            natural_w + style.padding.left + style.padding.right,
            natural_h + style.padding.top + style.padding.bottom,
        );
        Ok(Size::new(
            resolve(
                style.width,
                available.width,
                natural.width,
                style.min.width,
                style.max.width,
            ),
            resolve(
                style.height,
                available.height,
                natural.height,
                style.min.height,
                style.max.height,
            ),
        ))
    }

    fn place(
        &mut self,
        id: NodeId,
        rect: Rect,
        clip: Rect,
        depth: usize,
    ) -> Result<(), LayoutError> {
        if depth > MAX_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        let node = self.tree.get(id).ok_or(LayoutError::UnknownNode(id))?;
        let style = self.styles.style(id);
        if [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
        {
            return Err(LayoutError::NonFiniteGeometry);
        }
        let visible = rect.intersect(clip);
        let physical = pixel_coverage(visible, self.viewport.scale_factor);
        self.boxes.insert(
            id,
            LayoutBox {
                rect,
                visible,
                physical,
            },
        );
        self.paint_order.push(id);
        let child_clip = if style.clip_children { visible } else { clip };
        let content = Rect::new(
            rect.x + style.padding.left,
            rect.y + style.padding.top,
            (rect.width - style.padding.left - style.padding.right).max(0.0),
            (rect.height - style.padding.top - style.padding.bottom).max(0.0),
        );
        self.place_flow(
            node.children(),
            content,
            child_clip,
            style.axis,
            style.gap,
            depth + 1,
        )
    }

    fn place_flow(
        &mut self,
        ids: &[NodeId],
        content: Rect,
        clip: Rect,
        axis: Axis,
        gap: f32,
        depth: usize,
    ) -> Result<(), LayoutError> {
        if ids.is_empty() {
            return Ok(());
        }
        if depth > MAX_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        let available = Size::new(content.width, content.height);
        let mut sizes = Vec::with_capacity(ids.len());
        let mut styles = Vec::with_capacity(ids.len());
        for id in ids {
            sizes.push(self.measure(*id, available, depth)?);
            styles.push(self.styles.style(*id));
        }
        let total_gap = gap * (ids.len().saturating_sub(1) as f32);
        let target = (main_extent(available, axis) - total_gap).max(0.0);
        let mut bases: Vec<f32> = sizes.iter().map(|s| main_extent(*s, axis)).collect();
        let total: f32 = bases.iter().sum();
        if total < target {
            distribute(&mut bases, &styles, target - total, axis, true);
        } else if total > target {
            distribute(&mut bases, &styles, total - target, axis, false);
        }
        let mut cursor = match axis {
            Axis::Row => content.x,
            Axis::Column => content.y,
        };
        for (index, id) in ids.iter().enumerate() {
            let style = &styles[index];
            let cross_length = match axis {
                Axis::Row => style.height,
                Axis::Column => style.width,
            };
            let cross_bound = match axis {
                Axis::Row => (style.min.height, style.max.height),
                Axis::Column => (style.min.width, style.max.width),
            };
            let cross = if style.stretch_cross && cross_length == Length::Auto {
                cross_extent(available, axis)
                    .max(cross_bound.0)
                    .min(cross_bound.1)
            } else {
                cross_extent(sizes[index], axis)
            };
            let rect = match axis {
                Axis::Row => Rect::new(cursor, content.y, bases[index], cross),
                Axis::Column => Rect::new(content.x, cursor, cross, bases[index]),
            };
            self.place(*id, rect, clip, depth)?;
            cursor += bases[index] + gap;
        }
        Ok(())
    }
}

/// Bounded deterministic water-fill: reallocate after hitting min/max.
/// Remaining over-constrained pixels are deliberately allowed to overflow and
/// are then constrained by clip propagation, not silently shrunk below minima.
fn distribute(bases: &mut [f32], styles: &[Style], mut pending: f32, axis: Axis, grow: bool) {
    for _ in 0..styles.len() {
        if pending <= 0.00001 {
            break;
        }
        let mut weight_sum = 0.0;
        for (i, style) in styles.iter().enumerate() {
            let limit = if grow {
                main_extent(style.max, axis)
            } else {
                main_extent(style.min, axis)
            };
            let capacity = if grow {
                (limit - bases[i]).max(0.0)
            } else {
                (bases[i] - limit).max(0.0)
            };
            if capacity > 0.0 {
                weight_sum += if grow {
                    style.flex_grow
                } else {
                    style.flex_shrink * bases[i]
                };
            }
        }
        if weight_sum <= 0.0 {
            break;
        }
        let mut applied = 0.0;
        for (i, style) in styles.iter().enumerate() {
            let limit = if grow {
                main_extent(style.max, axis)
            } else {
                main_extent(style.min, axis)
            };
            let capacity = if grow {
                (limit - bases[i]).max(0.0)
            } else {
                (bases[i] - limit).max(0.0)
            };
            let weight = if grow {
                style.flex_grow
            } else {
                style.flex_shrink * bases[i]
            };
            if capacity <= 0.0 || weight <= 0.0 {
                continue;
            }
            let change = (pending * weight / weight_sum).min(capacity);
            if grow {
                bases[i] += change;
            } else {
                bases[i] -= change;
            }
            applied += change;
        }
        if applied <= 0.00001 {
            break;
        }
        pending = (pending - applied).max(0.0);
    }
}

/// Computes every layout box in a temporary map and only returns a complete,
/// version-stamped result. Failure publishes no partial geometry.
pub fn compute(
    tree: &UiSnapshot,
    styles: &LayoutStyles,
    surface: SurfaceId,
    viewport: Viewport,
) -> Result<LayoutSnapshot, LayoutError> {
    viewport.check()?;
    let surface_epoch = tree
        .surface_epoch(surface)
        .ok_or(LayoutError::UnknownSurface(surface))?;
    let surface_revision = tree
        .surface_revision(surface)
        .ok_or(LayoutError::UnknownSurface(surface))?;
    let roots = tree
        .roots(surface)
        .ok_or(LayoutError::UnknownSurface(surface))?;
    let screen = Rect::new(0.0, 0.0, viewport.logical.width, viewport.logical.height);
    let mut run = LayoutRun {
        tree,
        styles,
        viewport,
        boxes: BTreeMap::new(),
        paint_order: Vec::new(),
    };
    run.place_flow(roots, screen, screen, Axis::Column, 0.0, 0)?;
    Ok(LayoutSnapshot {
        stamp: LayoutStamp {
            surface,
            surface_epoch,
            surface_revision,
            ui_revision_at_compute: tree.revision(),
            style_revision: styles.revision(surface),
            geometry_epoch: viewport.geometry_epoch,
            viewport_bits: [
                viewport.logical.width.to_bits(),
                viewport.logical.height.to_bits(),
                viewport.scale_factor.to_bits(),
            ],
        },
        viewport,
        boxes: run.boxes,
        paint_order: run.paint_order,
    })
}
