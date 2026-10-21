//! ActiveWindow title for the reference Bar: font, shaping, elision, rotation
//! and the two-slot focus transition.
//!
//! The dynamic title is **one** GPUI-shaped text run. It is never generated as
//! an SVG and never split into per-character elements. Glyph outlines come from
//! the same bytes GPUI shaped (the process-private reference font), and the
//! whole run is rotated by an exact 90°/270° coordinate permutation.
//!
//! Observed reference facts (see `evidence/research/.../active-window-oracle`):
//! body-small 12 pt = 16 px, `opsz=12 wght=400 ROND=25`, absolute letter
//! spacing 1.4 px that Qt adds after every glyph (ligatures are disabled when
//! spacing is non-zero), `Qt.ElideRight`, title top margin 8 px.

use std::sync::Arc;

use gpui::{Font, FontFeatures, FontStyle, FontWeight, Hsla, TextRun, WindowTextSystem, px};

use crate::{
    product_icon::{IconRole, icon_metrics},
    reference_bar::{
        ACTIVE_WINDOW_TITLE_TOP_MARGIN, active_window_elide_width, active_window_max_height,
    },
};

/// Unique family name of the derived static instance. It must never collide
/// with a product font: the GPUI text system is process-global.
pub const REFERENCE_FONT_FAMILY: &str = "Gnosix Reference Sans 01F";
/// Body small, 12 pt at the observed 96 dpi.
pub const TITLE_FONT_SIZE: f32 = 16.0;
/// `letterSpacing(1.4)` is absolute and added after every glyph.
pub const TITLE_LETTER_SPACING: f32 = 1.4;
/// Icon size token: 15 pt at the observed 96 dpi.
pub const ICON_SIZE: f32 = 20.0;
/// `Tokens.anim.durations.expressiveDefaultEffects`.
pub const TITLE_FADE_MS: f32 = 200.0;
/// `Tokens.anim.expressiveDefaultEffects` control points `x1 y1 x2 y2`.
pub const TITLE_FADE_EASING: [f32; 4] = [0.34, 0.8, 0.34, 1.0];

#[cfg(any(test, feature = "component-lab"))]
pub const REFERENCE_FONT_BYTES: &[u8] =
    include_bytes!("../assets/fonts/gnosix-reference-sans-01f/GnosixReferenceSans01F-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleError {
    /// The derived reference family is not registered with the text system.
    /// Glyph ids from any fallback family would not match the reference
    /// outlines, so nothing is shaped or painted.
    FontUnavailable,
}

/// Register the derived reference font with this process's text system only.
/// No fontconfig, user font directory or host state is touched.
#[cfg(any(test, feature = "component-lab"))]
pub fn register_reference_font(text_system: &gpui::TextSystem) -> Result<(), TitleError> {
    text_system
        .add_fonts(vec![std::borrow::Cow::Borrowed(REFERENCE_FONT_BYTES)])
        .map_err(|_| TitleError::FontUnavailable)?;
    reference_font_registered(text_system)
        .then_some(())
        .ok_or(TitleError::FontUnavailable)
}

/// `Title.implicitHeight`: the reference font's line height, rounded up to
/// whole pixels. It is also the rotation origin (`h/2`, `h/2`).
pub fn title_line_height(wts: &WindowTextSystem) -> Result<f32, TitleError> {
    if !reference_font_registered(wts) {
        return Err(TitleError::FontUnavailable);
    }
    let id = wts.resolve_font(&reference_font());
    Ok((f32::from(wts.ascent(id, px(TITLE_FONT_SIZE)))
        + f32::from(wts.descent(id, px(TITLE_FONT_SIZE))).abs())
    .ceil())
}

pub fn reference_font_registered(text_system: &gpui::TextSystem) -> bool {
    text_system
        .all_font_names()
        .iter()
        .any(|name| name == REFERENCE_FONT_FAMILY)
}

pub fn reference_font() -> Font {
    Font {
        family: REFERENCE_FONT_FAMILY.into(),
        // Qt disables ligatures when letter spacing is non-zero.
        features: FontFeatures(Arc::new(vec![("liga".into(), 0), ("clig".into(), 0)])),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TitleGlyph {
    pub glyph_id: u32,
    /// Pen x in the unrotated line, letter spacing included.
    pub x: f32,
    pub byte_index: usize,
    /// Shaped by a family other than the reference font; cannot be outlined.
    pub fallback: bool,
    /// Pen advanced with the reference 26.6 arithmetic; false marks a glyph
    /// that spans several characters or came from a fallback (residual).
    pub quantized: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapedTitle {
    pub text: String,
    pub glyphs: Vec<TitleGlyph>,
    /// Unrotated `Title.implicitWidth`: advances plus trailing letter spacing.
    pub advance: f32,
    /// Baseline inside the unrotated text item: the font ascent snapped to a
    /// whole pixel (measured: lowercase ink boxes coincide with the reference).
    pub baseline: f32,
    /// `Title.implicitHeight` for this font (see [`title_line_height`]).
    pub line_height: f32,
}

impl ShapedTitle {
    /// Advance of the prefix that ends before `byte_index`.
    fn prefix_advance(&self, byte_index: usize) -> f32 {
        self.glyphs
            .iter()
            .find(|glyph| glyph.byte_index >= byte_index)
            .map_or(self.advance, |glyph| glyph.x)
    }
}

pub fn shape_title(wts: &WindowTextSystem, text: &str) -> Result<ShapedTitle, TitleError> {
    if !reference_font_registered(wts) {
        return Err(TitleError::FontUnavailable);
    }
    let line_height = title_line_height(wts)?;
    if text.is_empty() {
        return Ok(ShapedTitle {
            text: String::new(),
            glyphs: Vec::new(),
            advance: 0.0,
            baseline: 0.0,
            line_height,
        });
    }
    let font = reference_font();
    let reference_id = wts.resolve_font(&font);
    let run = TextRun {
        len: text.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let layout = wts.layout_line(text, px(TITLE_FONT_SIZE), &[run], None);
    let shaped: Vec<(u32, f32, usize, bool)> = layout
        .runs
        .iter()
        .flat_map(|shaped_run| {
            let fallback = shaped_run.font_id != reference_id;
            shaped_run.glyphs.iter().map(move |glyph| {
                (
                    glyph.id.0,
                    f32::from(glyph.position.x),
                    glyph.index,
                    fallback,
                )
            })
        })
        .collect();
    let line_width = f32::from(layout.width);

    // Reference pen arithmetic (26.6 fixed point), not GPUI's f32 pen:
    // pen += trunc64(advance) + round64(kerning) + trunc64(letter spacing).
    let mut glyphs = Vec::with_capacity(shaped.len());
    let mut pen = 0.0_f32;
    for (index, &(glyph_id, gpui_x, byte_index, fallback)) in shaped.iter().enumerate() {
        let next_gpui_x = shaped.get(index + 1).map_or(line_width, |next| next.1);
        let next_byte = shaped.get(index + 1).map_or(text.len(), |next| next.2);
        // Quantize only when exactly one character produced this glyph, so the
        // raw advance is known; otherwise keep GPUI's span (residual, flagged
        // by `quantized == false`).
        let single_char = text[byte_index..next_byte].chars().count() == 1;
        let raw_advance = single_char
            .then(|| text[byte_index..].chars().next())
            .flatten()
            .filter(|_| !fallback)
            .and_then(|ch| wts.advance(reference_id, px(TITLE_FONT_SIZE), ch).ok())
            .map(|size| f32::from(size.width));
        glyphs.push(TitleGlyph {
            glyph_id,
            x: pen,
            byte_index,
            fallback,
            quantized: raw_advance.is_some(),
        });
        pen += match raw_advance {
            Some(advance) => {
                let kerning = (next_gpui_x - gpui_x) - advance;
                fixed_trunc(advance) + fixed_round(kerning) + letter_spacing_step()
            }
            None => (next_gpui_x - gpui_x) + letter_spacing_step(),
        };
    }
    let baseline = f32::from(wts.ascent(reference_id, px(TITLE_FONT_SIZE))).round();
    Ok(ShapedTitle {
        text: text.to_owned(),
        glyphs,
        advance: pen,
        baseline,
        line_height,
    })
}

/// Pen step added after every glyph: the configured spacing in 26.6.
fn letter_spacing_step() -> f32 {
    fixed_trunc(TITLE_LETTER_SPACING)
}

/// 26.6 fixed point: the largest multiple of 1/64 strictly below the value.
///
/// Observed: an exact 20.0 advance is reported as 19.984375 (1279/64), at both
/// point and pixel sizes and with hinting off; for non-integer values this is
/// ordinary truncation. The tolerance absorbs the f32 representation error of
/// exact `units * size / upem` rationals and is far below 1/64.
fn fixed_trunc(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    (((f64::from(value) * 64.0 - 1e-3).ceil() - 1.0) / 64.0) as f32
}

/// 26.6 fixed point, round-to-nearest (kerning adjustments).
fn fixed_round(value: f32) -> f32 {
    ((f64::from(value) * 64.0).round() / 64.0) as f32
}

/// `TextMetrics { elide: Qt.ElideRight; elideWidth }.elidedText`.
///
/// The text is kept whole when its advance is `<=` the width (measured: an
/// advance of exactly 119 fits width 119); otherwise the longest prefix whose
/// advance plus the ellipsis (without trailing spacing) is strictly `<` the
/// width (measured: the sum 151.0 is rejected at width 151), followed by `…`; when not even the ellipsis fits the result is empty. Prefixes break at
/// `char` boundaries; combining sequences are not clustered (non-claim).
pub fn elide_right(
    wts: &WindowTextSystem,
    text: &str,
    elide_width: f32,
) -> Result<String, TitleError> {
    let full = shape_title(wts, text)?;
    if full.advance <= elide_width {
        return Ok(text.to_owned());
    }
    let ellipsis = shape_title(wts, "\u{2026}")?.advance - letter_spacing_step();
    let mut kept = None;
    for (byte_index, _) in text.char_indices() {
        if full.prefix_advance(byte_index) + ellipsis < elide_width {
            kept = Some(byte_index);
        } else {
            break;
        }
    }
    Ok(match kept {
        Some(end) => format!("{}\u{2026}", &text[..end]),
        None => String::new(),
    })
}

/// Map a point of the unrotated 21 px-high text item into the rotated title
/// box (`width = 21`, `height = advance`) used by `ActiveWindow.qml`.
///
/// Default: `Rotation { angle: 90 }` about `(h/2, h/2)`.
/// Inverted: `Translate { x: -advance + h }` then `angle: 270`.
pub fn rotate_title_point(
    x: f32,
    y: f32,
    advance: f32,
    line_height: f32,
    inverted: bool,
) -> (f32, f32) {
    if inverted {
        (y, advance - x)
    } else {
        (line_height - y, x)
    }
}

/// One `Title` slot: its text, and its opacity animation.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SlotFade {
    from: f32,
    to: f32,
    start_ms: f32,
    duration_ms: f32,
}

impl SlotFade {
    fn settled(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start_ms: 0.0,
            duration_ms: 0.0,
        }
    }

    fn value(&self, now_ms: f32) -> f32 {
        if self.duration_ms <= 0.0 || now_ms >= self.start_ms + self.duration_ms {
            return self.to;
        }
        let progress = ((now_ms - self.start_ms) / self.duration_ms).max(0.0);
        self.from + (self.to - self.from) * cubic_bezier_ease(TITLE_FADE_EASING, progress)
    }
}

/// Cubic bezier easing through (0,0), (x1,y1), (x2,y2), (1,1): `QEasingCurve`
/// `BezierSpline` as configured by `Tokens.anim.expressiveDefaultEffects`.
///
/// The parameter `t` with `x(t) == progress` is found by Newton iteration
/// starting at `t = progress` (bisection when the slope vanishes), so the
/// curve endpoints are reproduced by construction, not special-cased.
pub fn cubic_bezier_ease([x1, y1, x2, y2]: [f32; 4], progress: f32) -> f32 {
    let progress = f64::from(progress.clamp(0.0, 1.0));
    let (x1, y1, x2, y2) = (f64::from(x1), f64::from(y1), f64::from(x2), f64::from(y2));
    let axis = |a: f64, b: f64, t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let slope = |a: f64, b: f64, t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };
    let mut t = progress;
    let mut solved = false;
    for _ in 0..16 {
        let error = axis(x1, x2, t) - progress;
        if error.abs() < 1e-12 {
            solved = true;
            break;
        }
        let derivative = slope(x1, x2, t);
        if derivative.abs() < 1e-9 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    if !solved {
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        for _ in 0..60 {
            t = (low + high) / 2.0;
            if axis(x1, x2, t) < progress {
                low = t;
            } else {
                high = t;
            }
        }
    }
    axis(y1, y2, t) as f32
}

/// `text1` / `text2` with `root.current` and `Behavior on opacity`.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleSlots {
    texts: [String; 2],
    current: usize,
    fades: [SlotFade; 2],
}

impl Default for TitleSlots {
    /// Initial QML state: `current: text1`, both texts empty.
    fn default() -> Self {
        Self {
            texts: [String::new(), String::new()],
            current: 0,
            fades: [SlotFade::settled(1.0), SlotFade::settled(0.0)],
        }
    }
}

impl TitleSlots {
    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn text(&self, slot: usize) -> &str {
        &self.texts[slot]
    }

    pub fn current_text(&self) -> &str {
        &self.texts[self.current]
    }

    pub fn opacity(&self, slot: usize, now_ms: f32) -> f32 {
        self.fades[slot].value(now_ms)
    }

    /// `onTextChanged`: write the next slot's elided text, make it current.
    pub fn change_title(&mut self, elided: &str, now_ms: f32, reduced_motion: bool) {
        let next = 1 - self.current;
        self.texts[next] = elided.to_owned();
        self.current = next;
        let duration_ms = if reduced_motion { 0.0 } else { TITLE_FADE_MS };
        for slot in 0..2 {
            let target = if slot == self.current { 1.0 } else { 0.0 };
            self.fades[slot] = SlotFade {
                from: self.fades[slot].value(now_ms),
                to: target,
                start_ms: now_ms,
                duration_ms,
            };
        }
    }

    /// `onElideWidthChanged`: rewrite the current slot without swapping.
    pub fn refit_current(&mut self, elided: &str) {
        self.texts[self.current] = elided.to_owned();
    }
}

/// Everything the renderer needs for one frame of the module.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveWindowFrame {
    pub inverted: bool,
    /// Shaped text per slot with its opacity at the sampled time.
    pub slots: [(ShapedTitle, f32); 2],
    pub current: usize,
}

impl ActiveWindowFrame {
    pub fn build(
        wts: &WindowTextSystem,
        slots: &TitleSlots,
        inverted: bool,
        now_ms: f32,
    ) -> Result<Self, TitleError> {
        Ok(Self {
            inverted,
            slots: [
                (shape_title(wts, slots.text(0))?, slots.opacity(0, now_ms)),
                (shape_title(wts, slots.text(1))?, slots.opacity(1, now_ms)),
            ],
            current: slots.current_index(),
        })
    }

    pub fn current_title(&self) -> &ShapedTitle {
        &self.slots[self.current].0
    }
}

/// Layout facts the module needs from the surrounding reference Bar.
#[derive(Clone, Copy, Debug)]
pub struct ActiveWindowInputs<'a> {
    /// Effective donor `windowTitle` (Desktop/compact already applied).
    pub title: &'a str,
    pub role: Option<IconRole>,
    pub inverted: bool,
    pub bar_height: f32,
    /// Intrinsic heights of every participating non-spacer module except
    /// ActiveWindow itself.
    pub other_module_heights: &'a [f32],
    /// `bar.children.length`: entries plus the Repeater.
    pub bar_child_count: usize,
}

/// One planned frame of the module.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveWindowPlan {
    /// Effective donor `windowTitle` before elision.
    pub source_title: String,
    pub role: Option<IconRole>,
    pub max_height: f32,
    pub elide_width: f32,
    /// Native icon box (see `product_icon::icon_metrics`).
    pub icon_width: f32,
    pub icon_height: f32,
    pub icon_baseline: f32,
    /// Derived `Title.implicitHeight`.
    pub title_line_height: f32,
    pub frame: ActiveWindowFrame,
    /// `Math.max(icon.implicitWidth, current.implicitHeight)`.
    pub implicit_width: f32,
    /// `icon.implicitHeight + current.implicitWidth + topMargin`.
    pub implicit_height: f32,
}

/// The donor module's retained state: two title slots and the last
/// `windowTitle`, so a title change swaps slots while a width change refits.
#[derive(Clone, Debug, Default)]
pub struct ActiveWindowState {
    slots: TitleSlots,
    observed_title: Option<String>,
}

impl ActiveWindowState {
    pub fn slots(&self) -> &TitleSlots {
        &self.slots
    }

    pub fn plan(
        &mut self,
        wts: &WindowTextSystem,
        inputs: &ActiveWindowInputs<'_>,
        now_ms: f32,
        reduced_motion: bool,
    ) -> Result<ActiveWindowPlan, TitleError> {
        let max_height = active_window_max_height(
            inputs.bar_height,
            inputs.other_module_heights,
            inputs.bar_child_count,
        );
        // The icon box exists even before a role resolves: the module always
        // reserves its icon line.
        let icon = icon_metrics(ICON_SIZE);
        let elide_width = active_window_elide_width(max_height, icon.height);
        let elided = elide_right(wts, inputs.title, elide_width)?;
        if self.observed_title.as_deref() == Some(inputs.title) {
            if self.slots.current_text() != elided {
                self.slots.refit_current(&elided);
            }
        } else {
            self.slots.change_title(&elided, now_ms, reduced_motion);
            self.observed_title = Some(inputs.title.to_owned());
        }
        let frame = ActiveWindowFrame::build(wts, &self.slots, inputs.inverted, now_ms)?;
        let title_line_height = frame.current_title().line_height;
        let implicit_width = icon.width.max(title_line_height);
        let implicit_height =
            icon.height + frame.current_title().advance + ACTIVE_WINDOW_TITLE_TOP_MARGIN;
        Ok(ActiveWindowPlan {
            source_title: inputs.title.to_owned(),
            role: inputs.role,
            max_height,
            elide_width,
            icon_width: icon.width,
            icon_height: icon.height,
            icon_baseline: icon.baseline,
            title_line_height,
            frame,
            implicit_width,
            implicit_height,
        })
    }
}

#[cfg(feature = "component-lab")]
impl ActiveWindowPlan {
    /// Machine-readable description bound into capture manifests.
    pub fn to_json(&self, case: &str, font_registered: bool) -> serde_json::Value {
        let slots: Vec<_> = self
            .frame
            .slots
            .iter()
            .map(|(title, opacity)| {
                serde_json::json!({
                    "text": title.text,
                    "opacity": opacity,
                    "advance": title.advance,
                    "glyphs": title.glyphs.len(),
                    "quantized_glyphs": title.glyphs.iter().filter(|g| g.quantized).count(),
                    "fallback_glyphs": title.glyphs.iter().filter(|g| g.fallback).count(),
                    "painted_runs": usize::from(!title.glyphs.is_empty() && *opacity > 0.0),
                })
            })
            .collect();
        serde_json::json!({
            "case": case,
            "source_title": self.source_title,
            "font_family": REFERENCE_FONT_FAMILY,
            "font_registered_process_privately": font_registered,
            "font_sha256": "see evidence/research/.../font/derivation.json",
            "icon_role": self.role.map(IconRole::name),
            "icon_box": [self.icon_width, self.icon_height],
            "icon_baseline": self.icon_baseline,
            "title_line_height": self.title_line_height,
            "max_height": self.max_height,
            "elide_width": self.elide_width,
            "implicit_width": self.implicit_width,
            "implicit_height": self.implicit_height,
            "inverted": self.frame.inverted,
            "current_slot": self.frame.current,
            "slots": slots,
        })
    }
}

#[cfg(feature = "component-lab")]
pub fn title_paths(
    title: &ShapedTitle,
    inverted: bool,
    origin: (f32, f32),
) -> Vec<gpui::Path<gpui::Pixels>> {
    use gpui::{PathBuilder, point};

    struct Outline<'a> {
        builder: PathBuilder,
        map: &'a dyn Fn(f32, f32) -> gpui::Point<gpui::Pixels>,
        open: bool,
    }
    impl ttf_parser::OutlineBuilder for Outline<'_> {
        fn move_to(&mut self, x: f32, y: f32) {
            self.builder.move_to((self.map)(x, y));
            self.open = true;
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.builder.line_to((self.map)(x, y));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.builder.curve_to((self.map)(x, y), (self.map)(x1, y1));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.builder
                .cubic_bezier_to((self.map)(x, y), (self.map)(x1, y1), (self.map)(x2, y2));
        }
        fn close(&mut self) {
            self.builder.close();
            self.open = false;
        }
    }

    let Ok(face) = ttf_parser::Face::parse(REFERENCE_FONT_BYTES, 0) else {
        return Vec::new();
    };
    let scale = TITLE_FONT_SIZE / f32::from(face.units_per_em());
    let advance = title.advance;
    let line_height = title.line_height;
    let mut paths = Vec::new();
    for glyph in title.glyphs.iter().filter(|glyph| !glyph.fallback) {
        // Measured (ink centroids, 4 snapping variants): the reference keeps
        // the horizontal pen at its exact sub-pixel value (mean offset
        // +0.03 px) while its baseline is whole-pixel (light vertical hinting).
        let pen_x = glyph.x;
        let baseline = title.baseline;
        let map = move |fx: f32, fy: f32| {
            let (rx, ry) = rotate_title_point(
                pen_x + fx * scale,
                baseline - fy * scale,
                advance,
                line_height,
                inverted,
            );
            point(px(origin.0 + rx), px(origin.1 + ry))
        };
        let mut outline = Outline {
            builder: PathBuilder::fill(),
            map: &map,
            open: false,
        };
        let Some(_) = face.outline_glyph(ttf_parser::GlyphId(glyph.glyph_id as u16), &mut outline)
        else {
            continue;
        };
        if let Ok(path) = outline.builder.build() {
            paths.push(path);
        }
    }
    paths
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use gpui::TextSystem;

    /// A real (platform) text system without any window or event loop.
    pub(crate) fn text_system(register: bool) -> (Arc<TextSystem>, WindowTextSystem) {
        let platform = gpui_platform::current_platform(true);
        let text_system = Arc::new(TextSystem::new(platform.text_system()));
        if register {
            register_reference_font(&text_system).expect("reference font registers");
        }
        let window_text_system = WindowTextSystem::new(text_system.clone());
        (text_system, window_text_system)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::text_system;
    use super::*;

    const LIVE_TITLE: &str = "Ratification Phase 5 OSD - Helium";
    /// Qt 6.11 advance of `LIVE_TITLE` in the derived static font.
    const QT_STATIC_ADVANCE: f32 = 307.109_38;
    /// Qt 6.11 advance of the same title in the original variable font at the
    /// donor axes (the live truth).
    const QT_VARIABLE_ADVANCE: f32 = 306.718_75;
    /// Largest accepted |GPUI - Qt variable| advance difference, in px.
    const ADVANCE_TOLERANCE: f32 = 1.0;

    #[test]
    fn unregistered_reference_font_is_refused_instead_of_falling_back() {
        let (text_system, wts) = text_system(false);
        assert!(!reference_font_registered(&text_system));
        assert_eq!(
            shape_title(&wts, LIVE_TITLE),
            Err(TitleError::FontUnavailable)
        );
        assert_eq!(
            elide_right(&wts, LIVE_TITLE, 100.0),
            Err(TitleError::FontUnavailable)
        );
    }

    #[test]
    fn registered_reference_font_shapes_one_run_with_oracle_metrics() {
        let (text_system, wts) = text_system(true);
        assert!(reference_font_registered(&text_system));
        let title = shape_title(&wts, LIVE_TITLE).unwrap();
        // Ligatures are off: one glyph per character, one run.
        assert_eq!(title.glyphs.len(), LIVE_TITLE.chars().count());
        assert!(title.glyphs.iter().all(|glyph| !glyph.fallback));
        assert!(title.glyphs.windows(2).all(|pair| pair[0].x < pair[1].x));
        // Letter spacing is included: first pen gap exceeds the raw advance.
        assert!((title.glyphs[1].x - 10.464 - TITLE_LETTER_SPACING).abs() < 0.05);
        let versus_variable = (title.advance - QT_VARIABLE_ADVANCE).abs();
        let versus_static = (title.advance - QT_STATIC_ADVANCE).abs();
        assert!(
            versus_variable <= ADVANCE_TOLERANCE,
            "advance {} vs live variable-font oracle {QT_VARIABLE_ADVANCE}",
            title.advance
        );
        assert!(versus_static <= ADVANCE_TOLERANCE);
    }

    /// Qt 6.11 oracle: implicit height of one body-small title line.
    const QT_TITLE_LINE_HEIGHT: f32 = 21.0;

    #[test]
    fn fixed_point_truncation_is_strictly_below_the_exact_value() {
        // Observed: an exact 20.0 advance is reported as 19.984375 (1279/64).
        assert_eq!(fixed_trunc(20.0), 19.984_375);
        assert_eq!(fixed_trunc(10.448), 10.4375);
        assert_eq!(fixed_trunc(0.0), 0.0);
        assert_eq!(fixed_trunc(1.4), 1.390_625);
    }

    #[test]
    fn title_line_height_is_derived_from_the_font_not_a_constant() {
        let (_, wts) = text_system(true);
        assert_eq!(title_line_height(&wts).unwrap(), QT_TITLE_LINE_HEIGHT);
        assert_eq!(
            shape_title(&wts, "Ab").unwrap().line_height,
            QT_TITLE_LINE_HEIGHT
        );
    }

    #[test]
    fn baseline_is_derived_from_the_font_ascent_not_a_constant() {
        let (_, wts) = text_system(true);
        let title = shape_title(&wts, "Ab").unwrap();
        let ascent = wts.ascent(wts.resolve_font(&reference_font()), px(TITLE_FONT_SIZE));
        assert_eq!(title.baseline, f32::from(ascent).round());
        assert_eq!(shape_title(&wts, "").unwrap().baseline, 0.0);
    }

    #[test]
    fn ligatures_are_disabled_like_qt_with_letter_spacing() {
        let (_, wts) = text_system(true);
        let title = shape_title(&wts, "ffi fi fl office").unwrap();
        assert_eq!(title.glyphs.len(), "ffi fi fl office".chars().count());
    }

    #[test]
    fn elision_matches_qt_right_elision_sweep() {
        let (_, wts) = text_system(true);
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/active-window-oracle/elision-sweep.json"
        ))
        .unwrap();
        let mut total = 0_usize;
        let mut mismatched = Vec::new();
        for case in oracle["cases"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            for (width, expected) in case["sweep"].as_array().unwrap().iter().enumerate() {
                total += 1;
                let actual = elide_right(&wts, text, width as f32).unwrap();
                if actual != expected.as_str().unwrap() {
                    mismatched.push((text.chars().take(12).collect::<String>(), width));
                }
            }
        }
        eprintln!(
            "elision sweep: {} / {} mismatched: {:?}",
            mismatched.len(),
            total,
            mismatched
        );
        assert!(
            mismatched.len() * 100 <= total * 3,
            "more than 3% of integer widths elide differently from Qt"
        );
    }

    /// GPUI shaper residual: ASCII pairs whose kerning GPUI's shaper does not
    /// apply the way Qt's HarfBuzz does (font verified independently: Qt with
    /// the static instance equals Qt with the variable font on 300/300 strings).
    fn pair_residual() -> std::collections::BTreeMap<String, f64> {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/active-window-oracle/gpui-shaper-pair-residual.json"
        ))
        .unwrap();
        oracle["pairs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(pair, row)| {
                (
                    pair.clone(),
                    row["qt"].as_f64().unwrap() - row["gpui"].as_f64().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn pen_arithmetic_reproduces_qt_advances_and_the_residual_is_fully_explained() {
        let (_, wts) = text_system(true);
        let oracle: Vec<(String, f64, f64)> = serde_json::from_str(include_str!(
            "../tests/fixtures/active-window-oracle/random-strings-qt-advances.json"
        ))
        .unwrap();
        let residual = pair_residual();
        let (mut exact, mut explained, mut unexplained) = (0, 0, Vec::new());
        for (text, qt_variable, _) in &oracle {
            let ours = f64::from(shape_title(&wts, text).unwrap().advance);
            if (ours - qt_variable).abs() < 1e-6 {
                exact += 1;
                continue;
            }
            let chars: Vec<char> = text.chars().collect();
            let correction: f64 = chars
                .windows(2)
                .filter_map(|pair| residual.get(&pair.iter().collect::<String>()))
                .sum();
            if (ours + correction - qt_variable).abs() < 1e-6 {
                explained += 1;
            } else {
                unexplained.push((text.clone(), ours, correction, *qt_variable));
            }
        }
        eprintln!(
            "random advances: exact {exact}, explained by the pair residual {explained}, unexplained {}",
            unexplained.len()
        );
        assert!(
            unexplained.is_empty(),
            "an unidentified mechanism remains: {unexplained:?}"
        );
        assert!(exact >= oracle.len() * 4 / 5);
    }

    #[test]
    #[ignore = "diagnostic: Qt vs GPUI pair advances, run with --ignored --nocapture"]
    fn diagnostic_pair_advances() {
        let (_, wts) = text_system(true);
        let qt: std::collections::BTreeMap<String, f64> = serde_json::from_str(
            &std::fs::read_to_string(std::env::var("PAIRS_JSON").unwrap()).unwrap(),
        )
        .unwrap();
        let mut bad = Vec::new();
        for (text, qt_advance) in &qt {
            let ours = f64::from(shape_title(&wts, text).unwrap().advance);
            if (ours - qt_advance).abs() > 1e-6 {
                bad.push(format!(
                    "{text:?} qt={qt_advance} ours={ours} d={:+.4}",
                    ours - qt_advance
                ));
            }
        }
        eprintln!("PAIRDIAG total={} mismatched={}", qt.len(), bad.len());
        for line in bad.iter().take(60) {
            eprintln!("PAIRDIAG {line}");
        }
    }

    #[test]
    fn elision_edge_cases_follow_the_donor_contract() {
        let (_, wts) = text_system(true);
        assert_eq!(elide_right(&wts, "", 10.0).unwrap(), "");
        // The text is kept exactly when its advance fits the integer width.
        let advance = shape_title(&wts, "Desktop").unwrap().advance;
        let fits = advance.ceil();
        assert_eq!(elide_right(&wts, "Desktop", fits).unwrap(), "Desktop");
        let cut = elide_right(&wts, "Desktop", fits - 1.0).unwrap();
        assert!(
            cut.ends_with('\u{2026}') && cut.len() > '\u{2026}'.len_utf8(),
            "{cut:?}"
        );
        assert!("Desktop".starts_with(cut.trim_end_matches('\u{2026}')));
        // Not even the ellipsis fits.
        assert_eq!(elide_right(&wts, "Desktop", 12.0).unwrap(), "");
        assert_eq!(elide_right(&wts, "Desktop", -5.0).unwrap(), "");
    }

    #[test]
    fn rotation_is_an_exact_permutation_for_both_orientations() {
        let advance = 100.0;
        // Default 90°: text start at the top, text top toward the right edge.
        assert_eq!(
            rotate_title_point(0.0, 0.0, advance, 21.0, false),
            (21.0, 0.0)
        );
        assert_eq!(
            rotate_title_point(0.0, 21.0, advance, 21.0, false),
            (0.0, 0.0)
        );
        assert_eq!(
            rotate_title_point(100.0, 0.0, advance, 21.0, false),
            (21.0, 100.0)
        );
        // Inverted 270°: text start at the bottom, text top toward the left.
        assert_eq!(
            rotate_title_point(0.0, 0.0, advance, 21.0, true),
            (0.0, 100.0)
        );
        assert_eq!(
            rotate_title_point(100.0, 21.0, advance, 21.0, true),
            (21.0, 0.0)
        );
    }

    #[test]
    fn title_slots_follow_the_two_text_instance_transition() {
        let mut slots = TitleSlots::default();
        assert_eq!(slots.current_index(), 0);
        assert_eq!(slots.opacity(0, 0.0), 1.0);
        assert_eq!(slots.opacity(1, 0.0), 0.0);

        slots.change_title("Desktop", 1000.0, false);
        assert_eq!(slots.current_index(), 1);
        assert_eq!(slots.text(1), "Desktop");
        assert_eq!(slots.opacity(0, 1000.0), 1.0);
        assert_eq!(slots.opacity(1, 1000.0), 0.0);
        let mid_old = slots.opacity(0, 1100.0);
        let mid_new = slots.opacity(1, 1100.0);
        assert!(mid_old < 1.0 && mid_old > 0.0, "{mid_old}");
        assert!(mid_new < 1.0 && mid_new > 0.0, "{mid_new}");
        assert!((mid_old + mid_new - 1.0).abs() < 1e-4);
        assert_eq!(slots.opacity(0, 1200.0), 0.0);
        assert_eq!(slots.opacity(1, 1200.0), 1.0);

        // The previous title stays in the old slot while it fades out.
        slots.change_title("Helium", 2000.0, false);
        assert_eq!(slots.current_index(), 0);
        assert_eq!(slots.text(1), "Desktop");
        assert_eq!(slots.current_text(), "Helium");

        // Retargeting mid-fade starts from the sampled opacity (no jump).
        let before = slots.opacity(1, 2100.0);
        slots.change_title("Third", 2100.0, false);
        assert!((slots.opacity(1, 2100.0) - before).abs() < 1e-4);
    }

    #[test]
    fn reduced_motion_collapses_the_fade_but_keeps_the_state_transition() {
        let mut slots = TitleSlots::default();
        slots.change_title("Desktop", 1000.0, true);
        assert_eq!(slots.current_index(), 1);
        assert_eq!(slots.opacity(0, 1000.0), 0.0);
        assert_eq!(slots.opacity(1, 1000.0), 1.0);
    }

    #[test]
    fn refitting_the_current_slot_does_not_swap_or_fade() {
        let mut slots = TitleSlots::default();
        slots.change_title("Desktop", 0.0, false);
        slots.refit_current("Desk\u{2026}");
        assert_eq!(slots.current_index(), 1);
        assert_eq!(slots.current_text(), "Desk\u{2026}");
        assert_eq!(slots.opacity(1, 500.0), 1.0);
    }

    #[test]
    fn fade_easing_is_the_donor_bezier() {
        assert_eq!(cubic_bezier_ease(TITLE_FADE_EASING, 0.0), 0.0);
        assert!((cubic_bezier_ease(TITLE_FADE_EASING, 1.0) - 1.0).abs() < 1e-5);
        // y rises faster than linear early on for control points (.34,.8).
        assert!(cubic_bezier_ease(TITLE_FADE_EASING, 0.25) > 0.25);
    }

    #[cfg(feature = "component-lab")]
    #[test]
    fn title_is_painted_as_one_outline_per_glyph_never_stacked_characters() {
        let (_, wts) = text_system(true);
        let title = shape_title(&wts, "Ab C").unwrap();
        let paths = title_paths(&title, false, (0.0, 0.0));
        // Space has no outline; three visible glyphs; all inside the rotated box.
        assert_eq!(paths.len(), 3);
        for path in &paths {
            let bounds = path.bounds;
            assert!(f32::from(bounds.origin.x) >= -1.0);
            assert!(f32::from(bounds.origin.x + bounds.size.width) <= title.line_height + 1.0);
            assert!(f32::from(bounds.origin.y + bounds.size.height) <= title.advance + 1.0);
        }
        // Reading direction runs down the rail: later glyphs sit lower.
        let tops: Vec<f32> = paths
            .iter()
            .map(|path| f32::from(path.bounds.origin.y))
            .collect();
        assert!(tops.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
