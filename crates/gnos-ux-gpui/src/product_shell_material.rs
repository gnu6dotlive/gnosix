//! Shell material: the output-level border the Bar is part of.
//!
//! The Bar is not an opaque 60 px panel. It is the left side of one inverted
//! rounded rectangle covering the whole output (a 60 px left border, 10 px
//! border elsewhere, 25 px inner corner radius), painted with a translucent
//! surface colour, casting a soft shadow into the output area and composited
//! over a blurred backdrop. This module is the typed, pure description of that
//! material and nothing else:
//!
//! - it owns border *geometry* and *paint semantics* (colour, alpha, shadow);
//! - it states the *compositor requirement* (blur region and alpha threshold)
//!   as data, because blurring the backdrop is the compositor's act;
//! - it owns neither input regions nor space reservation. Those stay with the
//!   placement authority, and `border.left` only has to agree with it.
//!
//! Source facts are observations of the reference shell's shader and colour
//! functions, re-derived here as Rust equations. Parameters that are an
//! inference from the retained raster are labelled as such.

/// Distances in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellBorder {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    /// Radius of the rounded cut-out's four corners.
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn width(self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(self) -> f32 {
        self.y1 - self.y0
    }
}

impl ShellBorder {
    /// The idle shell: `bar` px on the left, `thickness` on the other sides.
    pub fn idle(bar: f32, thickness: f32, radius: f32) -> Self {
        Self {
            left: bar,
            top: thickness,
            right: thickness,
            bottom: thickness,
            radius,
        }
    }

    /// The shell of one output. A Bar that is present widens the left border to
    /// its width; an output without a Bar (excluded, or hidden) keeps the
    /// ordinary `thickness` on all four sides.
    pub fn for_output(bar_width: Option<f32>, thickness: f32, radius: f32) -> Self {
        Self::idle(bar_width.unwrap_or(thickness), thickness, radius)
    }

    /// The output area left uncovered by the shell.
    pub fn hole(&self, output: (f32, f32)) -> Rect {
        Rect {
            x0: self.left,
            y0: self.top,
            x1: output.0 - self.right,
            y1: output.1 - self.bottom,
        }
    }

    /// The corner radius actually realised: a radius larger than half the
    /// hole's shorter side degenerates, as in the reference SDF.
    pub fn effective_radius(&self, output: (f32, f32)) -> f32 {
        let hole = self.hole(output);
        self.radius
            .min(hole.width() / 2.0)
            .min(hole.height() / 2.0)
            .max(0.0)
    }

    /// Signed distance from `(x, y)` to the hole's rounded boundary: negative
    /// inside the hole, positive in the shell.
    pub fn hole_distance(&self, output: (f32, f32), x: f32, y: f32) -> f32 {
        let hole = self.hole(output);
        let r = self.effective_radius(output);
        let cx = (hole.x0 + hole.x1) / 2.0;
        let cy = (hole.y0 + hole.y1) / 2.0;
        let dx = (x - cx).abs() - hole.width() / 2.0 + r;
        let dy = (y - cy).abs() - hole.height() / 2.0 + r;
        let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
        outside + dx.max(dy).min(0.0) - r
    }

    /// Paint coverage of the shell at the centre of pixel `(px, py)`:
    /// `1 - smoothstep(-fw, fw, d)` with `d` the shell distance
    /// (`-hole_distance`) and `fw = |dd/dx| + |dd/dy|` over the pixel.
    pub fn coverage(&self, output: (f32, f32), px: u32, py: u32) -> f32 {
        let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
        let d = -self.hole_distance(output, x, y);
        let gx =
            (-self.hole_distance(output, x + 0.5, y)) - (-self.hole_distance(output, x - 0.5, y));
        let gy =
            (-self.hole_distance(output, x, y + 0.5)) - (-self.hole_distance(output, x, y - 0.5));
        let fw = (gx.abs() + gy.abs()).max(f32::EPSILON);
        1.0 - smoothstep(-fw, fw, d)
    }

    /// Straight strips that tile the shell exactly outside the four corner
    /// fillets: left (full height), then top, bottom, right between the left
    /// strip and the output's right edge.
    pub fn strips(&self, output: (f32, f32)) -> [Rect; 4] {
        let (w, h) = output;
        [
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: self.left,
                y1: h,
            },
            Rect {
                x0: self.left,
                y0: 0.0,
                x1: w,
                y1: self.top,
            },
            Rect {
                x0: self.left,
                y0: h - self.bottom,
                x1: w,
                y1: h,
            },
            Rect {
                x0: w - self.right,
                y0: self.top,
                x1: w,
                y1: h - self.bottom,
            },
        ]
    }

    /// The four corner fillets: the area between each hole corner's square
    /// and its rounded arc, as polygons `[corner, edge, arc..., edge]`.
    pub fn fillets(&self, output: (f32, f32), arc_segments: usize) -> Vec<Vec<(f32, f32)>> {
        let hole = self.hole(output);
        let r = self.effective_radius(output);
        if r <= 0.0 {
            return Vec::new();
        }
        let arc = |centre: (f32, f32), from_deg: f32| -> Vec<(f32, f32)> {
            (0..=arc_segments)
                .map(|i| {
                    let a = (from_deg + 90.0 * i as f32 / arc_segments as f32).to_radians();
                    (centre.0 + r * a.cos(), centre.1 + r * a.sin())
                })
                .collect()
        };
        let mut out = Vec::with_capacity(4);
        // Top-left: from the left tangent point (180 degrees) to the top one.
        let mut tl = vec![(hole.x0, hole.y0)];
        tl.extend(arc((hole.x0 + r, hole.y0 + r), 180.0));
        out.push(tl);
        let mut tr = vec![(hole.x1, hole.y0)];
        tr.extend(arc((hole.x1 - r, hole.y0 + r), 270.0));
        out.push(tr);
        let mut br = vec![(hole.x1, hole.y1)];
        br.extend(arc((hole.x1 - r, hole.y1 - r), 0.0));
        out.push(br);
        let mut bl = vec![(hole.x0, hole.y1)];
        bl.extend(arc((hole.x0 + r, hole.y1 - r), 90.0));
        out.push(bl);
        out
    }

    /// Concentric outline of the hole inset by `inset` px (corner radius
    /// shrinks with it), as a closed polygon. The shadow is drawn from these.
    pub fn inset_outline(
        &self,
        output: (f32, f32),
        inset: f32,
        arc_segments: usize,
    ) -> Vec<(f32, f32)> {
        let hole = self.hole(output);
        let r = (self.effective_radius(output) - inset).max(0.0);
        let (x0, y0, x1, y1) = (
            hole.x0 + inset,
            hole.y0 + inset,
            hole.x1 - inset,
            hole.y1 - inset,
        );
        let mut points = Vec::new();
        for (cx, cy, start) in [
            (x0 + r, y0 + r, 180.0_f32),
            (x1 - r, y0 + r, 270.0),
            (x1 - r, y1 - r, 0.0),
            (x0 + r, y1 - r, 90.0),
        ] {
            if r == 0.0 {
                points.push((cx, cy));
                continue;
            }
            for i in 0..=arc_segments {
                let a = (start + 90.0 * i as f32 / arc_segments as f32).to_radians();
                points.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        }
        points
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Straight-alpha colour with channels in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub rgb: [f32; 3],
    pub a: f32,
}

impl Rgba {
    pub fn opaque_u8(rgb: [u8; 3]) -> Self {
        Self {
            rgb: rgb.map(|c| f32::from(c) / 255.0),
            a: 1.0,
        }
    }

    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        [q(self.rgb[0]), q(self.rgb[1]), q(self.rgb[2]), q(self.a)]
    }

    /// `self` over an opaque backdrop, channels in `0.0..=1.0`.
    pub fn over(self, backdrop: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| self.a * self.rgb[i] + (1.0 - self.a) * backdrop[i])
    }
}

/// Appearance transparency settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transparency {
    pub enabled: bool,
    /// Alpha of layer-0 surfaces (the shell itself).
    pub base: f32,
    /// Alpha of layer-1 surfaces (module containers).
    pub layers: f32,
}

impl Transparency {
    /// Observed configuration: enabled, base 0.85, layers 0.4.
    pub const OBSERVED: Self = Self {
        enabled: true,
        base: 0.85,
        layers: 0.4,
    };

    pub fn base_effective(&self, light: bool) -> f32 {
        (self.base - if light { 0.1 } else { 0.0 }).clamp(0.0, 1.0)
    }
}

/// Perceptual luminance of one colour, `sqrt(.299 r^2 + .587 g^2 + .114 b^2)`.
pub fn luminance(rgb: [f32; 3]) -> f32 {
    if rgb == [0.0, 0.0, 0.0] {
        return 0.0;
    }
    (0.299 * rgb[0] * rgb[0] + 0.587 * rgb[1] * rgb[1] + 0.114 * rgb[2] * rgb[2]).sqrt()
}

/// Wallpaper-derived observation: the mean of [`luminance`] over the pixels
/// whose alpha is not zero, taken from an already-downscaled image.
pub fn wallpaper_luminance(pixels: &[[u8; 4]]) -> f32 {
    let mut total = 0.0_f64;
    let mut count = 0_u32;
    for p in pixels.iter().filter(|p| p[3] != 0) {
        total += f64::from(luminance([p[0], p[1], p[2]].map(|c| f32::from(c) / 255.0)));
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        (total / f64::from(count)) as f32
    }
}

/// Colour of a surface at `layer` (0 = the shell, 1 = a module container),
/// given the wallpaper's luminance. Layer 1 is brightened toward the
/// wallpaper's brightness so containers stay visible over a translucent shell.
pub fn layer_colour(
    rgb: [f32; 3],
    layer: u8,
    transparency: Transparency,
    wallpaper_luminance: f32,
    light: bool,
) -> Rgba {
    if !transparency.enabled {
        return Rgba { rgb, a: 1.0 };
    }
    let base = transparency.base_effective(light);
    if layer == 0 {
        return Rgba { rgb, a: base };
    }
    let l = luminance(rgb);
    let sign = if !light || layer == 1 {
        1.0
    } else {
        -f32::from(layer) / 2.0
    };
    let strength = if light { 0.2 } else { 0.3 };
    let wall_gain = if light {
        if layer == 1 { 3.0 } else { 1.0 }
    } else {
        2.5
    };
    let offset = sign * strength * (1.0 - base) * (1.0 + wallpaper_luminance * wall_gain);
    let scale = if l == 0.0 { 1.0 } else { (l + offset) / l };
    Rgba {
        rgb: rgb.map(|c| (c * scale).clamp(0.0, 1.0)),
        a: transparency.layers.clamp(0.0, 1.0),
    }
}

/// Soft shadow the shell casts into the output area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub rgb: [f32; 3],
    /// Peak alpha, reached at the shell's edge before the blur halves it.
    pub alpha: f32,
    pub sigma: f32,
}

/// INFERRED from one retained raster (donor `blurMax` 15 fits sigma 3.25):
/// the visible extent of the blur is about this many sigmas. Unknown for other
/// `blurMax` values; do not extrapolate.
pub const SHADOW_EXTENT_IN_SIGMAS: f32 = 4.6;

impl ShadowSpec {
    pub fn from_blur_max(rgb: [f32; 3], alpha: f32, blur_max: f32) -> Self {
        Self {
            rgb,
            alpha,
            sigma: blur_max / SHADOW_EXTENT_IN_SIGMAS,
        }
    }

    /// Remaining brightness factor of the content at `distance` px inside the
    /// hole from the shell's edge: a Gaussian-blurred half plane.
    pub fn brightness_factor(&self, distance: f32) -> f32 {
        1.0 - self.alpha * self.shadow_mask(distance)
    }

    /// Blurred half-plane mask, 0.5 at the edge and decaying inward.
    pub fn shadow_mask(&self, distance: f32) -> f32 {
        0.5 * erfc(distance / (self.sigma * std::f32::consts::SQRT_2))
    }

    /// Alpha to paint at inset `inset` (px from the edge, pixel centre).
    pub fn paint_alpha(&self, inset: f32) -> f32 {
        self.alpha * self.shadow_mask(inset)
    }

    /// Number of one-pixel insets that carry visible shadow (alpha >= 1/512).
    pub fn extent(&self) -> u32 {
        let mut d = 0;
        while self.paint_alpha(d as f32 + 0.5) >= 1.0 / 512.0 && d < 64 {
            d += 1;
        }
        d
    }
}

/// Complementary error function, Abramowitz & Stegun 7.1.26 (|err| < 1.5e-7),
/// evaluated in f64 so the published coefficients are exact.
pub fn erfc(x: f32) -> f32 {
    let z = f64::from(x).abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let e = poly * (-z * z).exp();
    (if x >= 0.0 { e } else { 2.0 - e }) as f32
}

/// What the compositor must do for the material to look as intended. The
/// client states it; applying it is the compositor's authority.
#[derive(Clone, Debug, PartialEq)]
pub struct BlurRequirement {
    /// Regions of the surface whose backdrop must be blurred: the shell strips.
    pub regions: Vec<Rect>,
    /// Pixels with alpha below this threshold must not be blurred (the shell
    /// alpha is above it; the translucent module layers over it are not asked
    /// for separately).
    pub ignore_alpha: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellMaterial {
    pub border: ShellBorder,
    pub surface: Rgba,
    pub container: Rgba,
    pub shadow: Option<ShadowSpec>,
    pub blur: Option<BlurRequirement>,
}

/// Palette inputs of the material (opaque palette roles, 8-bit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialPalette {
    pub surface: [u8; 3],
    pub surface_container: [u8; 3],
    pub shadow: [u8; 3],
}

/// Everything the material is derived from; no field is defaulted silently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialInputs {
    pub border: ShellBorder,
    pub output: (f32, f32),
    pub palette: MaterialPalette,
    pub transparency: Transparency,
    pub wallpaper_luminance: f32,
    pub light: bool,
    /// The reference shadow's maximum blur radius in pixels.
    pub shadow_blur_max: f32,
    pub shadow_alpha: f32,
}

impl ShellMaterial {
    pub fn derive(inputs: MaterialInputs) -> Self {
        let MaterialInputs {
            border,
            output,
            palette,
            transparency,
            wallpaper_luminance,
            light,
            ..
        } = inputs;
        let f = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
        let surface = layer_colour(
            f(palette.surface),
            0,
            transparency,
            wallpaper_luminance,
            light,
        );
        let container = layer_colour(
            f(palette.surface_container),
            1,
            transparency,
            wallpaper_luminance,
            light,
        );
        let blur = transparency.enabled.then(|| BlurRequirement {
            regions: border.strips(output).to_vec(),
            ignore_alpha: (transparency.base - 0.03).max(0.0),
        });
        Self {
            border,
            surface,
            container,
            shadow: Some(ShadowSpec::from_blur_max(
                f(palette.shadow),
                inputs.shadow_alpha,
                inputs.shadow_blur_max,
            )),
            blur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: (f32, f32) = (2560.0, 1440.0);

    fn border() -> ShellBorder {
        ShellBorder::idle(60.0, 10.0, 25.0)
    }

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn strips_tile_the_shell_and_leave_the_hole() {
        let b = border();
        let hole = b.hole(OUTPUT);
        assert_eq!(
            (hole.x0, hole.y0, hole.x1, hole.y1),
            (60.0, 10.0, 2550.0, 1430.0)
        );
        let strips = b.strips(OUTPUT);
        // 60 px left, 10 px top/bottom/right.
        assert_eq!(strips[0].width(), 60.0);
        assert_eq!(strips[1].height(), 10.0);
        assert_eq!(strips[2].height(), 10.0);
        assert_eq!(strips[3].width(), 10.0);
        // Strips plus fillets cover the shell exactly: strip area + fillet
        // area equals output area minus the rounded hole.
        let strip_area: f32 = strips.iter().map(|r| r.width() * r.height()).sum();
        let r = b.effective_radius(OUTPUT);
        let hole_area = hole.width() * hole.height() - (4.0 - std::f32::consts::PI) * r * r;
        let fillet_area = (4.0 - std::f32::consts::PI) * r * r / 4.0 * 4.0;
        assert!(close(
            strip_area + fillet_area,
            OUTPUT.0 * OUTPUT.1 - hole_area,
            0.5
        ));
    }

    #[test]
    fn an_output_without_a_bar_has_uniform_borders() {
        let with = ShellBorder::for_output(Some(60.0), 10.0, 25.0);
        let without = ShellBorder::for_output(None, 10.0, 25.0);
        assert_eq!(with, ShellBorder::idle(60.0, 10.0, 25.0));
        assert_eq!(
            (without.left, without.top, without.right, without.bottom),
            (10.0, 10.0, 10.0, 10.0)
        );
        let portrait = (1080.0, 1920.0);
        let hole = without.hole(portrait);
        assert_eq!(
            (hole.x0, hole.y0, hole.x1, hole.y1),
            (10.0, 10.0, 1070.0, 1910.0)
        );
        // The same radius applies; only the left border changed.
        assert_eq!(without.effective_radius(portrait), 25.0);
    }

    #[test]
    fn coverage_is_one_in_the_strips_zero_in_the_hole_and_partial_on_the_arc() {
        let b = border();
        assert_eq!(b.coverage(OUTPUT, 20, 700), 1.0);
        assert_eq!(b.coverage(OUTPUT, 1200, 4), 1.0);
        assert_eq!(b.coverage(OUTPUT, 1200, 1436), 1.0);
        assert_eq!(b.coverage(OUTPUT, 2556, 700), 1.0);
        assert_eq!(b.coverage(OUTPUT, 1200, 700), 0.0);
        // Straight edge on an integer boundary: the reference ramp is one pixel
        // wide and centred on the edge, so the pixel centres 0.5 px either
        // side read smoothstep(.25) = 0.156 of the way across.
        assert!(close(b.coverage(OUTPUT, 59, 700), 1.0 - 0.156_25, 1e-4));
        assert!(close(b.coverage(OUTPUT, 60, 700), 0.156_25, 1e-4));
        // Inner corner: the arc cuts the hole's square corner at 45 degrees.
        let r = 25.0_f32;
        let corner = (
            60.0 + r * (1.0 - std::f32::consts::FRAC_1_SQRT_2),
            10.0 + r * (1.0 - std::f32::consts::FRAC_1_SQRT_2),
        );
        let inside_square_corner = b.coverage(OUTPUT, 60, 10);
        assert!(
            inside_square_corner > 0.99,
            "the hole's square corner is shell"
        );
        let on_arc = b.coverage(OUTPUT, corner.0 as u32, corner.1 as u32);
        assert!(
            on_arc > 0.2 && on_arc < 0.8,
            "arc pixel is partial: {on_arc}"
        );
    }

    #[test]
    fn fillet_area_matches_the_circle_equation() {
        let b = border();
        let mut shell_in_corner = 0.0_f32;
        for y in 10..(10 + 25) {
            for x in 60..(60 + 25) {
                shell_in_corner += b.coverage(OUTPUT, x, y);
            }
        }
        let expected = (1.0 - std::f32::consts::FRAC_PI_4) * 25.0 * 25.0;
        assert!(
            close(shell_in_corner, expected, 3.0),
            "{shell_in_corner} vs {expected}"
        );
    }

    #[test]
    fn fillet_polygons_are_the_four_corner_slivers_in_the_shell() {
        let b = border();
        let fillets = b.fillets(OUTPUT, 32);
        assert_eq!(fillets.len(), 4);
        let r = 25.0_f32;
        let expected = (1.0 - std::f32::consts::FRAC_PI_4) * r * r;
        let mut corners = Vec::new();
        for polygon in &fillets {
            // Shoelace area.
            let mut twice = 0.0_f32;
            for i in 0..polygon.len() {
                let (x0, y0) = polygon[i];
                let (x1, y1) = polygon[(i + 1) % polygon.len()];
                twice += x0 * y1 - x1 * y0;
            }
            assert!(
                close(twice.abs() / 2.0, expected, 0.5),
                "{}",
                twice.abs() / 2.0
            );
            // Every arc vertex lies on the hole's boundary; the corner vertex
            // is the hole's square corner, inside the shell.
            for &(x, y) in &polygon[1..] {
                assert!(b.hole_distance(OUTPUT, x, y).abs() < 0.05, "({x}, {y})");
            }
            corners.push(polygon[0]);
            // Halfway between the square corner and the arc's 45-degree vertex
            // is in the shell, on the polygon's own side of the arc.
            let mid = polygon[1 + (polygon.len() - 1) / 2];
            let (cx, cy) = ((polygon[0].0 + mid.0) / 2.0, (polygon[0].1 + mid.1) / 2.0);
            assert!(
                b.hole_distance(OUTPUT, cx, cy) > 0.0,
                "({cx}, {cy}) is in the shell"
            );
        }
        corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            corners,
            vec![
                (60.0, 10.0),
                (60.0, 1430.0),
                (2550.0, 10.0),
                (2550.0, 1430.0)
            ]
        );
    }

    #[test]
    fn radius_degenerates_to_half_the_hole_for_tiny_outputs() {
        let b = ShellBorder::idle(60.0, 10.0, 25.0);
        assert_eq!(b.effective_radius((130.0, 40.0)), 10.0);
        assert_eq!(b.effective_radius(OUTPUT), 25.0);
    }

    #[test]
    fn layer_colour_follows_the_dark_equations() {
        let t = Transparency::OBSERVED;
        let surface = layer_colour([0.0745, 0.0745, 0.0902], 0, t, 0.1305, false);
        assert_eq!(surface.a, 0.85);
        assert_eq!(surface.rgb, [0.0745, 0.0745, 0.0902]);
        // Container: offset = 0.3 * (1 - 0.85) * (1 + 0.1305 * 2.5).
        let container_in = [
            0x20 as f32 / 255.0,
            0x1f as f32 / 255.0,
            0x23 as f32 / 255.0,
        ];
        let c = layer_colour(container_in, 1, t, 0.1305, false);
        assert_eq!(c.a, 0.4);
        let l = luminance(container_in);
        let offset = 0.3 * (1.0 - 0.85) * (1.0 + 0.1305 * 2.5);
        let scale = (l + offset) / l;
        for (got, input) in c.rgb.iter().zip(container_in) {
            assert!(close(*got, input * scale, 1e-6));
        }
        // Brighter wallpaper brightens the container further.
        let bright = layer_colour(container_in, 1, t, 0.8, false);
        assert!(bright.rgb[0] > c.rgb[0]);
        // Disabled transparency is opaque and unaltered.
        let off = Transparency {
            enabled: false,
            ..t
        };
        assert_eq!(
            layer_colour(container_in, 1, off, 0.5, false),
            Rgba {
                rgb: container_in,
                a: 1.0
            }
        );
    }

    #[test]
    fn light_mode_lowers_base_alpha_and_changes_the_offset_law() {
        let t = Transparency::OBSERVED;
        assert!(close(t.base_effective(true), 0.75, 1e-6));
        let c = [0.8, 0.8, 0.85];
        let light = layer_colour(c, 1, t, 0.5, true);
        let dark = layer_colour(c, 1, t, 0.5, false);
        assert_ne!(light.rgb, dark.rgb);
        assert_eq!(layer_colour(c, 0, t, 0.5, true).a, 0.75);
    }

    #[test]
    fn wallpaper_luminance_ignores_transparent_pixels_and_averages_the_rest() {
        let black_white = [[0, 0, 0, 255], [255, 255, 255, 255], [255, 0, 0, 0]];
        // sqrt(.299+.587+.114) = 1 for white; transparent red is excluded.
        assert!(close(wallpaper_luminance(&black_white), 0.5, 1e-6));
        assert_eq!(wallpaper_luminance(&[]), 0.0);
        assert_eq!(wallpaper_luminance(&[[9, 9, 9, 0]]), 0.0);
    }

    #[test]
    fn composite_equation_reproduces_the_reference_capsule_pixel() {
        // Retained raster: capsule padding rows read (31.6, 29.4, 36.1) over a
        // backdrop whose blurred wallpaper is about (32, 19, 43).
        let t = Transparency::OBSERVED;
        let surface = layer_colour(
            [
                0x13 as f32 / 255.0,
                0x13 as f32 / 255.0,
                0x17 as f32 / 255.0,
            ],
            0,
            t,
            0.1305,
            false,
        );
        let container = layer_colour(
            [
                0x20 as f32 / 255.0,
                0x1f as f32 / 255.0,
                0x23 as f32 / 255.0,
            ],
            1,
            t,
            0.1305,
            false,
        );
        let backdrop = [32.0 / 255.0, 19.0 / 255.0, 43.0 / 255.0];
        let under = surface.over(backdrop);
        let pixel = container.over(under).map(|c| c * 255.0);
        assert!(close(pixel[0], 31.6, 1.5), "{pixel:?}");
        assert!(close(pixel[1], 29.4, 1.5), "{pixel:?}");
        assert!(close(pixel[2], 36.1, 1.5), "{pixel:?}");
    }

    #[test]
    fn shadow_decays_with_distance_and_is_half_strength_at_the_edge() {
        let s = ShadowSpec::from_blur_max([0.0; 3], 0.7, 15.0);
        assert!(close(s.sigma, 15.0 / 4.6, 1e-6));
        assert!(close(s.shadow_mask(0.0), 0.5, 1e-6));
        let mut previous = s.brightness_factor(0.0);
        assert!(close(previous, 1.0 - 0.7 * 0.5, 1e-6));
        assert!(
            s.brightness_factor(3.0) > previous + 0.05,
            "recovery is real, not saturated"
        );
        for d in 1..30 {
            let factor = s.brightness_factor(d as f32);
            assert!(factor >= previous, "brightness recovers monotonically");
            assert!(factor <= 1.0);
            previous = factor;
        }
        // Reference raster factors (mean of the right and bottom gaps). The
        // Gaussian is a classified approximation of a multi-pass blur: it is
        // 0.041 too dark one pixel from the edge, then within 0.03.
        for (d, observed, tolerance) in [
            (1.5, 0.815, 0.045),
            (2.5, 0.864, 0.03),
            (3.5, 0.900, 0.03),
            (4.5, 0.925, 0.03),
        ] {
            let factor = s.brightness_factor(d);
            assert!(close(factor, observed, tolerance), "d={d}: {factor}");
        }
        assert!((8..=12).contains(&s.extent()), "extent {}", s.extent());
    }

    /// The material owns paint and compositor requirements only: its code
    /// (comments excluded) names neither the placement authority nor input or
    /// reservation concepts, so it cannot silently redefine them.
    #[test]
    fn material_code_never_touches_input_or_reservation_authority() {
        let code: String = include_str!("product_shell_material.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        for forbidden in [
            "placement",
            "input_region",
            "inputregion",
            "reservation",
            "exclusive",
            "set_opaque",
            "surface_edge",
        ] {
            assert!(
                !code.contains(forbidden),
                "material code mentions {forbidden}"
            );
        }
    }

    #[test]
    fn erfc_matches_known_values() {
        assert!(close(erfc(0.0), 1.0, 1e-6));
        assert!(close(erfc(1.0), 0.157_299, 1e-5));
        assert!(close(erfc(-1.0), 1.842_701, 1e-5));
    }

    #[test]
    fn blur_requirement_follows_transparency_and_covers_exactly_the_strips() {
        let palette = MaterialPalette {
            surface: [0x13, 0x13, 0x17],
            surface_container: [0x20, 0x1f, 0x23],
            shadow: [0, 0, 0],
        };
        let inputs = MaterialInputs {
            border: border(),
            output: OUTPUT,
            palette,
            transparency: Transparency::OBSERVED,
            wallpaper_luminance: 0.1305,
            light: false,
            shadow_blur_max: 15.0,
            shadow_alpha: 0.7,
        };
        let m = ShellMaterial::derive(inputs);
        let blur = m.blur.as_ref().unwrap();
        assert!(close(blur.ignore_alpha, 0.82, 1e-6));
        assert_eq!(blur.regions, border().strips(OUTPUT).to_vec());
        assert!(
            blur.ignore_alpha < m.surface.a,
            "the shell itself is blurred"
        );
        assert!(
            blur.regions
                .iter()
                .all(|r| r.x1 <= OUTPUT.0 && r.y1 <= OUTPUT.1)
        );
        let off = Transparency {
            enabled: false,
            ..Transparency::OBSERVED
        };
        let opaque = ShellMaterial::derive(MaterialInputs {
            transparency: off,
            ..inputs
        });
        assert!(opaque.blur.is_none());
        assert_eq!(opaque.surface.a, 1.0);
    }
}
