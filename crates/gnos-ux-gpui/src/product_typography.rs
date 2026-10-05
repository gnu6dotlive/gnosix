//! Private, deterministic typography scale for the GNOS.UX product scene.
//!
//! This is a qualification control only. It does not read output scale, desktop
//! settings, or any persistent user preference.

const BASE_FONT_SIZE_PX: f32 = 14.0;
const MIN_SCALE: f32 = 1.0;
const MAX_SCALE: f32 = 2.0;
const ARG_PREFIX: &str = "--product-text-scale=";
pub(super) const WORKSPACE_CLOSE_LABEL: &str = "Fermer";
pub(super) const WORKSPACE_CLOSE_GAP_PX: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ProductTextScale {
    factor: f32,
}

impl Default for ProductTextScale {
    fn default() -> Self {
        Self { factor: 1.0 }
    }
}

impl ProductTextScale {
    pub(super) fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && (MIN_SCALE..=MAX_SCALE).contains(&factor)).then_some(Self { factor })
    }

    pub(super) fn from_args(args: &[String]) -> Result<Self, String> {
        let Some(value) = args.iter().find_map(|arg| arg.strip_prefix(ARG_PREFIX)) else {
            return Ok(Self::default());
        };
        let factor = value
            .parse::<f32>()
            .map_err(|_| format!("invalid text-scale value: {value}"))?;
        Self::new(factor).ok_or_else(|| {
            format!("text-scale factor must be finite and within {MIN_SCALE:.2}..={MAX_SCALE:.2}")
        })
    }

    pub(super) fn factor(self) -> f32 {
        self.factor
    }

    pub(super) fn font_size_px(self) -> f32 {
        BASE_FONT_SIZE_PX * self.factor
    }
}

#[cfg(test)]
mod tests {
    use super::{ProductTextScale, WORKSPACE_CLOSE_GAP_PX, WORKSPACE_CLOSE_LABEL};
    use crate::product_scene::ProductLayout;
    use gnos_ux_surface::grammar::SurfaceViewport;
    use gpui::{SharedString, TestAppContext, TextRun, TruncateFrom, font, px};

    #[test]
    fn product_text_scale_has_one_bounded_candidate_factor() {
        for (factor, expected_px) in [(1.0, 14.0), (1.25, 17.5), (1.5, 21.0), (2.0, 28.0)] {
            let scale = ProductTextScale::new(factor).unwrap();
            assert_eq!(scale.factor(), factor);
            assert_eq!(scale.font_size_px(), expected_px);
        }
        assert!(ProductTextScale::new(0.99).is_none());
        assert!(ProductTextScale::new(2.01).is_none());
        assert!(ProductTextScale::new(f32::NAN).is_none());
    }

    #[test]
    fn qualification_argument_is_explicit_and_does_not_use_rem_or_output_scale() {
        assert_eq!(
            ProductTextScale::from_args(&[]).unwrap(),
            ProductTextScale::default()
        );
        assert_eq!(
            ProductTextScale::from_args(&["--product-text-scale=1.5".to_owned()])
                .unwrap()
                .factor(),
            1.5
        );
        assert!(ProductTextScale::from_args(&["--product-text-scale=2.1".to_owned()]).is_err());
        assert!(ProductTextScale::from_args(&["--product-text-scale=large".to_owned()]).is_err());
    }

    #[test]
    fn long_product_labels_truncate_to_measured_node_widths_at_every_candidate_scale() {
        let app = TestAppContext::single();
        let viewports = [
            SurfaceViewport::try_new(1280.0, 720.0).unwrap(),
            SurfaceViewport::try_new(2560.0, 1440.0).unwrap(),
        ];
        let labels = [
            "gnos.ux / DISPLAYPORT-ULTRAWIDE-OUTPUT-NAME / workspace-with-an-intentionally-long-name",
            "workspace-one · workspace-two-with-a-long-name · workspace-three · workspace-four",
        ];
        let workspace_detail = "A very long workspace name · 28 application(s) visibles · An application title that is deliberately much longer than the Bar";

        app.update(|cx| {
            let text_system = cx.text_system().clone();
            for viewport in viewports {
                let layout = ProductLayout::resolve(viewport, 1.0).unwrap();
                let node_widths = [
                    layout.bar.width() - 40.0,
                    layout.dock.width(),
                    layout.workspace.unwrap().width() - 40.0,
                ];
                for factor in [1.0_f32, 1.25, 1.5, 2.0] {
                    let font_size = px(ProductTextScale::new(factor).unwrap().font_size_px());
                    for (label, node_width) in labels.iter().zip(node_widths) {
                        let text = SharedString::from(*label);
                        let font = font(".SystemUIFont");
                        let runs = [TextRun {
                            len: text.len(),
                            font: font.clone(),
                            ..Default::default()
                        }];
                        let mut wrapper = text_system.line_wrapper(font, font_size);
                        let (truncated, _) = wrapper.truncate_line(
                            text,
                            px(node_width),
                            "…",
                            &runs,
                            TruncateFrom::End,
                        );
                        let (fit, _) = wrapper.truncate_line(
                            truncated.clone(),
                            px(node_width),
                            "…",
                            &runs,
                            TruncateFrom::End,
                        );
                        assert_eq!(fit, truncated, "text exceeded node at {factor}×");
                    }

                    let font = font(".SystemUIFont");
                    let mut close_wrapper = text_system.line_wrapper(font.clone(), font_size);
                    let mut close_width = 1.0_f32;
                    while close_wrapper
                        .should_truncate_line(
                            WORKSPACE_CLOSE_LABEL,
                            px(close_width),
                            "",
                            TruncateFrom::End,
                        )
                        .is_some()
                    {
                        close_width += 1.0;
                        assert!(
                            close_width <= node_widths[2],
                            "close affordance must fit inside the workspace node at {factor}×"
                        );
                    }
                    let detail_width = node_widths[2] - WORKSPACE_CLOSE_GAP_PX - close_width;
                    assert!(
                        detail_width > 0.0,
                        "reserved close affordance must fit at {factor}×"
                    );

                    let text = SharedString::from(workspace_detail);
                    let runs = [TextRun {
                        len: text.len(),
                        font: font.clone(),
                        ..Default::default()
                    }];
                    let mut wrapper = text_system.line_wrapper(font, font_size);
                    let width = px(detail_width);
                    let (truncated, _) =
                        wrapper.truncate_line(text, width, "…", &runs, TruncateFrom::End);
                    let (fit, _) = wrapper.truncate_line(
                        truncated.clone(),
                        width,
                        "…",
                        &runs,
                        TruncateFrom::End,
                    );
                    assert_eq!(fit, truncated, "workspace detail must fit at {factor}×");
                }
            }
        });
    }
}
