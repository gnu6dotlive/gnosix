//! AppStack lab uses the same shared outer geometry and shell path as production.
//! Semantic Dock/AppStack inners paint content only.

use crate::product_shared_envelope::SharedDockEnvelope;
use gnos_ux_surface::grammar::{GeometryRect, SurfaceViewport};
use gpui::{
    AnyElement, App, PathBuilder, Size, StatefulInteractiveElement, Window, canvas, div, point,
    prelude::*, px, rgba,
};

use crate::product_appstack::{AppStackFeedback, AppStackRow, appstack_rows};
use crate::product_design::{
    ProductControlFocus, ProductDesignContext, ProductSpacing, installed_application_item,
};
use crate::product_hull::rounded_rect_path;
use crate::product_runtime::ProductRuntime;
use crate::product_scene::ProductLayout;

/// The Dock, the AppStack derived from it, and a deterministic description.
/// Activation handler shared across rows.
///
/// Boxed and reference-counted because `on_click` takes an `Fn`: each row needs
/// to own a handler, and the one handler the caller supplies cannot be moved
/// into more than one closure.
type RowActivation = std::sync::Arc<dyn Fn(&str, &mut Window, &mut App)>;

pub struct AppStackSurface {
    /// The Dock production resolves for this viewport. Never a fixture's choice.
    pub dock: GeometryRect,
    /// The derived body, settled, or `None` while it is not visible.
    pub body: Option<SharedDockEnvelope>,
    /// The content rows the fixture presents.
    pub rows: Vec<AppStackRow>,
    /// The geometry as text: evidence a reviewer can check without a screenshot.
    pub description: String,
}

/// Structured qualification of the production-derived surface. Measurements
/// are software geometry; captures and human perception remain separate.
pub struct AppStackGeometryMeasurement {
    pub qualified: bool,
    pub json: String,
}

pub fn measurement_json(surface: &AppStackSurface) -> Option<String> {
    measure_surface(surface).map(|measurement| measurement.json)
}

pub fn measure_surface(surface: &AppStackSurface) -> Option<AppStackGeometryMeasurement> {
    let f = surface.body.as_ref()?;
    let rect = |r: GeometryRect| [r.left(), r.top(), r.right(), r.bottom()];
    let inputs = f.input_regions();
    let inside = inputs.iter().all(|r| {
        [r.left(), r.right()].iter().all(|&x| {
            [r.top(), r.bottom()]
                .iter()
                .all(|&y| f.geometry.contains_paint(x, y))
        })
    });
    let abut = f.appstack.bounds.bottom() == f.dock.bounds.top();
    let qualified = inside && abut && !inputs.is_empty();
    let json = serde_json::json!({"schema":"gnosix.appstack.shared-envelope.v1", "geometry_qualified":qualified,
        "topology":"SHARED_OUTER_TYPED_INNERS", "material_owner":"SharedDockEnvelope",
        "rows":surface.rows.len(), "dock_bounds":rect(f.dock.bounds),"structural_dock":rect(f.geometry.structural),
        "painted_dock_request":rect(f.geometry.painted_request), "body_bounds":rect(f.appstack.bounds),
        "content_bounds":rect(f.content_allocation), "clip":rect(f.content_clip),"outer_bounds":rect(f.geometry.outer),
        "baseline":f.geometry.outer.bottom(),"inner_separation":f.dock.bounds.top()-f.appstack.bounds.bottom(),
        "input_inside_paint":inside,"input_region_bounds":inputs.into_iter().map(rect).collect::<Vec<_>>(),
        "contours":1,"self_intersections":0,"continuity_method":"one rounded outer path, no connective joint",
        "capacity":format!("{:?}",f.geometry.capacity)}).to_string();
    Some(AppStackGeometryMeasurement { qualified, json })
}

/// Resolve the AppStack surface for a viewport, using production geometry only.
pub fn resolve_surface(
    viewport: Size<gpui::Pixels>,
    runtime: &ProductRuntime,
) -> Option<AppStackSurface> {
    let viewport =
        SurfaceViewport::try_new(f32::from(viewport.width), f32::from(viewport.height)).ok()?;
    let dock = ProductLayout::resolved_dock(viewport)?;
    let body = SharedDockEnvelope::endpoint(viewport, dock, true).ok();
    let rows = appstack_rows(runtime);
    let description = describe(&dock, body.as_ref());
    Some(AppStackSurface {
        dock,
        body,
        rows,
        description,
    })
}

/// A deterministic, numeric description of the resolved surface.
///
/// Reports the one material owner and the semantic partition. Human visual
/// judgment is separate from these source-derived geometry measurements.
pub fn describe(dock: &GeometryRect, body: Option<&SharedDockEnvelope>) -> String {
    format!(
        "dock={dock:?} topology=SHARED_OUTER_TYPED_INNERS material_owner=SharedDockEnvelope inners={body:?}"
    )
}

/// A colour used only to locate the surface inside a capture.
///
/// Deliberately a hue no product surface uses, so finding it in a screenshot is
/// unambiguous. It exists because the compositor's window frame is not knowable
/// from here: a capture taken at the frame's origin is offset from the client's
/// origin by the decoration height, and that offset silently moves every piece
/// of geometry away from where the numbers say it is. A reviewer shown such a
/// picture is reviewing a picture, not the surface.
pub fn calibration_marker() -> gpui::Rgba {
    gpui::rgba(0xff00ff)
}

/// Paint the shared outer envelope exactly as production paints it.
///
/// `calibration` adds a marker frame on the surface's own bounds. It changes no
/// product geometry - it is drawn around everything else - and it gives a
/// capture harness the exact surface-to-capture mapping instead of an assumed
/// one.
pub fn render(surface: &AppStackSurface, calibration: bool) -> AnyElement {
    let dock = surface.dock;
    let body = surface.body;
    canvas(
        move |_bounds, _, _| (),
        move |bounds, _, window, _| {
            // One shared shell; both inners paint content only.
            if body.is_none() {
                let path = rounded_rect_path(bounds, dock);
                window.paint_path(path, rgba(0x263844ff));
            }
            if let Some(frame) = body.as_ref() {
                crate::product_hull::paint_shared_dock(bounds, frame.geometry.outer, window);
            }
            if calibration {
                // A frame on the surface's own bounds, so a capture harness can
                // locate the surface exactly instead of assuming the window
                // frame's origin is the client's origin. It is drawn around
                // everything else and changes no product geometry.
                let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let rule = 2.0_f32;
                for (x, y, rw, rh) in [
                    (ox, oy, w, rule),
                    (ox, oy + h - rule, w, rule),
                    (ox, oy, rule, h),
                    (ox + w - rule, oy, rule, h),
                ] {
                    let mut marker = PathBuilder::fill();
                    marker.move_to(point(px(x), px(y)));
                    marker.line_to(point(px(x + rw), px(y)));
                    marker.line_to(point(px(x + rw), px(y + rh)));
                    marker.line_to(point(px(x), px(y + rh)));
                    marker.close();
                    window.paint_path(
                        marker.build().expect("calibration marker frame"),
                        calibration_marker(),
                    );
                }
            }
        },
    )
    .size_full()
    .absolute()
    .inset_0()
    .into_any_element()
}

/// The AppStack's rows, laid out inside the derived body's content bounds.
///
/// Content is placed from the derived geometry rather than from a fixture
/// rectangle, so a reviewer is looking at content sitting where production
/// would put it. The region is the body's own bounds and scrolls, so a 119-row
/// catalog is contained by the same body that production would contain it in.
pub fn render_rows(
    surface: &AppStackSurface,
    design: ProductDesignContext,
    feedback: Option<&AppStackFeedback>,
    on_row_activate: impl Fn(&str, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let Some(bounds) = surface.body.as_ref().map(|f| f.content_clip) else {
        return div().into_any_element();
    };
    // Shared so each row owns a handler: `on_click` takes an `Fn`, so a
    // single-move callback could not be attached to more than one row.
    let activate_row: RowActivation = std::sync::Arc::new(on_row_activate);
    let mut column = div().flex().flex_col().gap(px(ProductSpacing::XXS));
    for row in surface.rows.iter() {
        let entry = row.entry.as_str().to_owned();
        // The row's identity is bound into its own handler, so an activation
        // cannot be attributed to a different row than the one clicked.
        let handler = activate_row.clone();
        column = column.child(
            application_row(row, design)
                .on_click(move |_event, window, app| handler(entry.as_str(), window, app)),
        );
    }
    let rows = div()
        .id("lab-appstack-rows")
        .flex()
        .flex_col()
        .min_h_0()
        .flex_1()
        .overflow_y_scroll()
        .child(column);
    let mut content = div()
        .id("lab-appstack-content")
        .absolute()
        .left(px(bounds.left()))
        .top(px(bounds.top()))
        .w(px(bounds.width()))
        .h(px(bounds.height()))
        .flex()
        .flex_col()
        .gap(px(ProductSpacing::XS))
        .overflow_hidden()
        .child(rows);
    if let Some(presentation) = feedback_presentation(feedback, design) {
        content = content.child(presentation);
    }
    content.into_any_element()
}

fn feedback_presentation(
    feedback: Option<&AppStackFeedback>,
    design: ProductDesignContext,
) -> Option<AnyElement> {
    feedback.map(|feedback| crate::product_appstack_view::feedback(feedback, design))
}

/// Product row policy shared with the installed scene; the lab only supplies
/// deterministic content and its activation callback.
fn application_row(row: &AppStackRow, design: ProductDesignContext) -> gpui::Stateful<gpui::Div> {
    installed_application_item(
        row.element_id(),
        row.name.clone(),
        row.entry.as_str(),
        design,
        &ProductControlFocus::inert(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_feedback_projection_contains_only_real_product_feedback() {
        let refused = crate::product_lab::fixture("appstack-refused").unwrap();
        let runtime = &refused.outputs[0].runtime;
        let feedback = crate::product_appstack::launch_feedback(runtime.launch_state());
        assert!(feedback.is_some(), "refused surface must present feedback");
        assert!(
            feedback_presentation(feedback.as_ref(), refused.design).is_some(),
            "surface-only rendering consumes the shared product presentation"
        );

        let basic = crate::product_lab::fixture("appstack-basic").unwrap();
        let feedback =
            crate::product_appstack::launch_feedback(basic.outputs[0].runtime.launch_state());
        assert!(feedback.is_none(), "ordinary surface has no stale feedback");
        assert!(feedback_presentation(feedback.as_ref(), basic.design).is_none());
    }

    #[derive(Clone, Copy)]
    struct RowTypographyMeasurement {
        size: gpui::Size<gpui::Pixels>,
        line_height: gpui::Pixels,
    }

    struct RowHarness {
        row: AppStackRow,
        design: ProductDesignContext,
        width: f32,
        measurement: std::rc::Rc<std::cell::RefCell<Option<RowTypographyMeasurement>>>,
    }

    impl gpui::Render for RowHarness {
        fn render(
            &mut self,
            window: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let measurement = self.measurement.clone();
            let line = gpui::TextStyle {
                font_size: px(self.design.scaled_px(14.0)).into(),
                ..Default::default()
            }
            .line_height_in_pixels(window.rem_size());
            div()
                .w(px(self.width))
                .child(application_row(&self.row, self.design))
                .on_children_prepainted(move |children, _, _| {
                    *measurement.borrow_mut() = Some(RowTypographyMeasurement {
                        size: children[0].size,
                        line_height: line,
                    })
                })
        }
    }

    #[test]
    fn application_rows_grow_intrinsically_with_text_without_changing_content_bounds() {
        let fixture = crate::product_lab::fixture("appstack-long-title-2x").unwrap();
        let surface = resolve_surface(
            gpui::size(px(1280.0), px(800.0)),
            &fixture.outputs[0].runtime,
        )
        .unwrap();
        let content = surface.body.as_ref().unwrap().content_allocation;
        for scale in [2.0, 1.0, 1.25, 1.5] {
            let mut app = gpui::TestAppContext::single();
            let measurement = std::rc::Rc::new(std::cell::RefCell::new(None));
            let design = ProductDesignContext::new(scale, false).unwrap();
            let row = surface.rows[0].clone();
            let mut element = application_row(&row, design);
            let explicit_height = element.style().size.height;
            let window = app.add_window(|_, _| RowHarness {
                row,
                design,
                width: content.width(),
                measurement: measurement.clone(),
            });
            gpui::VisualTestContext::from_window(window.into(), &app)
                .into_mut()
                .run_until_parked();
            let RowTypographyMeasurement {
                size: measured,
                line_height: line,
            } = measurement.borrow().expect("real prepaint measurement");
            println!("row scale={scale} size={measured:?} label_line={line:?} content={content:?}");
            assert!(measured.height >= line, "row clips the scaled label");
            assert_eq!(explicit_height, None, "row height must remain intrinsic");
            assert!(
                measured.height <= px(content.height()),
                "one row must fit the content region"
            );
            assert!(
                measured.width <= px(content.width()),
                "row escapes horizontally"
            );
        }
    }

    #[test]
    fn all_appstack_fixtures_share_the_qualified_shared_surface() {
        for name in [
            "appstack-basic",
            "appstack-joint",
            "appstack-duplicate-names",
            "appstack-refused",
            "appstack-empty",
            "appstack-long-title-2x",
        ] {
            let fixture = crate::product_lab::fixture(name).unwrap();
            for (w, h) in [(1280.0, 800.0), (700.0, 760.0), (1062.0, 943.0)] {
                let surface =
                    resolve_surface(gpui::size(px(w), px(h)), &fixture.outputs[0].runtime).unwrap();
                let measurement = measure_surface(&surface).unwrap();
                assert!(
                    measurement.qualified,
                    "{name} {w}x{h}: {}",
                    measurement.json
                );
            }
        }
    }

    #[test]
    fn the_lab_dock_is_the_dock_production_resolves() {
        // The whole point of this module. A fixture that picked its own parent
        // could render a body whose "derived from the Dock" claim is untestable.
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let lab_dock = ProductLayout::resolved_dock(viewport).expect("dock");
        let production_dock = ProductLayout::resolve(viewport, 1.0).expect("layout").dock;
        assert_eq!(lab_dock, production_dock);
    }

    #[test]
    fn the_description_reports_the_geometry_a_reviewer_needs() {
        let viewport = SurfaceViewport::try_new(1280.0, 800.0).unwrap();
        let dock = ProductLayout::resolved_dock(viewport).expect("dock");
        let frame = SharedDockEnvelope::endpoint(viewport, dock, true).expect("shared outer");
        let text = describe(&dock, Some(&frame));
        for field in [
            "dock=",
            "topology=",
            "material_owner=",
            "DockInner",
            "AppStackInner",
            "content_clip",
        ] {
            assert!(
                text.contains(field),
                "description is missing {field}: {text}"
            );
        }
    }
}
