//! Typed bridge from Gnosix shell-material requirements to GPUI's Wayland
//! background-effect region API. Request != compositor-applied proof.

use gpui::{Bounds, Pixels, Window, WindowBackgroundAppearance, point, px, size};

use crate::product_shell_material::{BlurRequirement, ShellMaterial};

/// Translate output-local strip rectangles to surface-local logical pixels.
/// Neither the paint geometry nor the input or reserved regions are mutated.
pub fn requested_blur_regions(blur: &BlurRequirement) -> Vec<Bounds<Pixels>> {
    blur.regions
        .iter()
        .filter(|r| {
            [r.x0, r.y0, r.x1, r.y1].into_iter().all(f32::is_finite) && r.x1 > r.x0 && r.y1 > r.y0
        })
        .map(|r| {
            Bounds::new(
                point(px(r.x0), px(r.y0)),
                size(px(r.x1 - r.x0), px(r.y1 - r.y0)),
            )
        })
        .collect()
}

/// Express an explicit blur request, not a claim that Hyprland applied it.
/// `ignore_alpha` is retained in the model but has no equivalent knob in the
/// ext-background-effect-v1 protocol; only rectangles are expressed here.
///
/// `regions: false` (SHELL-FIELD-01, the pixel mask): the compositor blurs by
/// the painted alpha, so no rectangle is sent.
pub fn request_shell_blur(
    window: &Window,
    material: &ShellMaterial,
    surfaces: &[Bounds<Pixels>],
    regions: bool,
) {
    if !regions {
        window.set_background_blur_regions(&[]);
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        return;
    }
    if let Some(blur) = &material.blur {
        let mut regions = requested_blur_regions(blur);
        // SET-02B: the surfaces extruded from the Bar stand over the desktop
        // too, so what lies behind them is blurred as it is behind the frame.
        regions.extend_from_slice(surfaces);
        // Install regions before switching appearance to avoid a transient
        // whole-surface blur on the initial compositor commit.
        window.set_background_blur_regions(&regions);
        window.set_background_appearance(WindowBackgroundAppearance::Blurred);
    } else {
        window.set_background_blur_regions(&[]);
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_shell_material::{Rect, ShellBorder};

    #[test]
    fn strip_regions_preserve_material_coordinates_and_never_expand_the_hole() {
        let rects = ShellBorder::idle(60.0, 10.0, 25.0).strips((2560.0, 1440.0));
        let result = requested_blur_regions(&BlurRequirement {
            regions: rects.to_vec(),
            ignore_alpha: 0.82,
        });
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].size.width, px(60.0));
        assert_eq!(result[0].size.height, px(1440.0));
        assert_eq!(result[1].origin.x, px(60.0));
        assert_eq!(result[1].size.height, px(10.0));
        assert_eq!(result[2].origin.y, px(1430.0));
        assert_eq!(result[3].origin.x, px(2550.0));
    }

    #[test]
    fn malformed_or_empty_regions_never_produce_a_full_window_request() {
        let blur = BlurRequirement {
            regions: vec![
                Rect {
                    x0: 8.0,
                    y0: 8.0,
                    x1: 8.0,
                    y1: 12.0,
                },
                Rect {
                    x0: f32::NAN,
                    y0: 0.0,
                    x1: 10.0,
                    y1: 10.0,
                },
            ],
            ignore_alpha: 0.82,
        };
        assert!(requested_blur_regions(&blur).is_empty());
    }
}
