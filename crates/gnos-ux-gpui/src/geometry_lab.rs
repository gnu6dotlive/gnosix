//! Provisional internal geometry/debug adapter shared by GNOS.UX qualification surfaces.
//!
//! Backend-agnostic geometry atoms/recipes/frames live in
//! `gnos_ux_surface::grammar`. This module only adds GPUI-facing conversion,
//! debug selection and development telemetry metadata.

pub use gnos_ux_surface::grammar::{
    BarGeometryFrame, BarGeometryRecipe, DockGeometryFrame, DockGeometryRecipe, GeometryError,
    GeometryRect, LengthBasis, RecipeIdentity, ResponsiveLength, SurfaceGeometryRecipe,
    SurfaceViewport, UnitInterval,
};
use gnos_ux_surface::{SurfaceRole, pointer_regions::PointerRegionSet};
use gpui::{Pixels, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryDebugView {
    Hull,
    Topology,
    Constraints,
    Motion,
    Content,
    All,
}

impl GeometryDebugView {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hull" => Some(Self::Hull),
            "topology" => Some(Self::Topology),
            "constraints" => Some(Self::Constraints),
            "motion" => Some(Self::Motion),
            "content" => Some(Self::Content),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn from_process_args() -> Self {
        std::env::args()
            .find_map(|arg| arg.strip_prefix("--geometry-view=").and_then(Self::parse))
            .unwrap_or(Self::All)
    }

    pub fn includes(self, view: Self) -> bool {
        self == Self::All || self == view
    }
}

pub fn viewport_from_size(size: Size<Pixels>) -> Option<SurfaceViewport> {
    SurfaceViewport::try_new(f32::from(size.width), f32::from(size.height)).ok()
}

pub fn resolve_dock_frame(viewport: SurfaceViewport, visible_fraction: f32) -> DockGeometryFrame {
    DockGeometryRecipe::atlas_baseline()
        .resolve(viewport, UnitInterval::clamped(visible_fraction))
        .expect("validated Dock geometry recipe")
}

pub fn pointer_region_diagnostic(surface: &str, revision: u64, set: &PointerRegionSet) -> String {
    let regions = set
        .regions()
        .iter()
        .map(|region| {
            let bounds = region.bounds();
            format!(
                "{}:{}:{:.2},{:.2},{:.2},{:.2}:decoratable={}",
                region.id().as_str(),
                region.role().as_str(),
                bounds.left(),
                bounds.top(),
                bounds.right(),
                bounds.bottom(),
                region.decoratable(),
            )
        })
        .collect::<Vec<_>>()
        .join(";");

    format!(
        "GNOS_UX_POINTER_REGIONS surface={} revision={} viewport={:.2}x{:.2} count={} regions={}",
        surface,
        revision,
        set.viewport().width,
        set.viewport().height,
        set.regions().len(),
        regions,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasCoverageLevel {
    AtlasRich,
    ProductRuntimeOnly,
    ProbeOnly,
    DeclaredOnly,
}

pub fn surface_atlas_coverage(role: SurfaceRole) -> AtlasCoverageLevel {
    match role {
        SurfaceRole::Bar | SurfaceRole::Dock => AtlasCoverageLevel::AtlasRich,
        SurfaceRole::Rail | SurfaceRole::Overlay => AtlasCoverageLevel::ProbeOnly,
    }
}

pub fn surface_atlas_ledger() -> [(SurfaceRole, AtlasCoverageLevel); 4] {
    [
        (SurfaceRole::Bar, surface_atlas_coverage(SurfaceRole::Bar)),
        (SurfaceRole::Dock, surface_atlas_coverage(SurfaceRole::Dock)),
        (SurfaceRole::Rail, surface_atlas_coverage(SurfaceRole::Rail)),
        (
            SurfaceRole::Overlay,
            surface_atlas_coverage(SurfaceRole::Overlay),
        ),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryFrameHeader {
    pub sample: u64,
    pub namespace: &'static str,
    pub viewport: SurfaceViewport,
    pub compositor_revision: u64,
}

impl GeometryFrameHeader {
    pub const fn new(
        sample: u64,
        namespace: &'static str,
        viewport: SurfaceViewport,
        compositor_revision: u64,
    ) -> Self {
        Self {
            sample,
            namespace,
            viewport,
            compositor_revision,
        }
    }

    pub fn json_fields(self) -> String {
        format!(
            "\"sample\":{},\"namespace\":\"{}\",\"compositor_revision\":{},\"viewport\":[{:.2},{:.2}]",
            self.sample,
            self.namespace,
            self.compositor_revision,
            self.viewport.width,
            self.viewport.height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn debug_views_parse_and_all_includes_every_view() {
        assert_eq!(
            GeometryDebugView::parse("hull"),
            Some(GeometryDebugView::Hull)
        );
        assert_eq!(GeometryDebugView::parse("unknown"), None);
        assert!(GeometryDebugView::All.includes(GeometryDebugView::Motion));
        assert!(!GeometryDebugView::Hull.includes(GeometryDebugView::Motion));
    }

    #[test]
    fn viewport_comes_from_live_pixel_size_without_reference_resolution() {
        let viewport =
            viewport_from_size(Size::new(px(1050.0), px(650.0))).expect("valid viewport");
        assert_eq!(viewport.width, 1050.0);
        assert_eq!(viewport.height, 650.0);
        assert_eq!(viewport.min_axis(), 650.0);
        assert!(
            viewport.approx_eq(SurfaceViewport::try_new(1050.2, 649.8).expect("near viewport"))
        );
    }

    #[test]
    fn dock_geometry_is_recipe_driven_and_input_tracks_visibility() {
        let viewport = SurfaceViewport::try_new(1200.0, 720.0).expect("viewport");
        let recipe = DockGeometryRecipe::atlas_baseline();
        let visible = recipe
            .resolve(viewport, UnitInterval::ONE)
            .expect("visible");
        assert!((visible.envelope.left() - 40.0).abs() < 0.01);
        assert!((visible.envelope.right() - 1160.0).abs() < 0.01);
        assert_eq!(visible.input_regions.len(), 2);
        assert_eq!(visible.visible_envelope, visible.envelope);

        let hidden = recipe
            .resolve(viewport, UnitInterval::ZERO)
            .expect("hidden");
        assert_eq!(hidden.input_regions.len(), 1);
        assert_eq!(hidden.input_regions[0], hidden.reveal_region);
        assert!(hidden.visible_envelope.height().abs() < 0.01);

        let resized_viewport = SurfaceViewport::try_new(900.0, 560.0).expect("resized viewport");
        let resized = recipe
            .resolve(resized_viewport, UnitInterval::ONE)
            .expect("resized");
        assert!((resized.envelope.left() / 900.0 - visible.envelope.left() / 1200.0).abs() < 0.001);
        assert!((resized.envelope.top() / 560.0 - visible.envelope.top() / 720.0).abs() < 0.001);
    }

    #[test]
    fn coverage_ledger_distinguishes_live_atlas_from_probe_only_roles() {
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Bar),
            AtlasCoverageLevel::AtlasRich
        );
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Dock),
            AtlasCoverageLevel::AtlasRich
        );
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Rail),
            AtlasCoverageLevel::ProbeOnly
        );
        assert_eq!(
            surface_atlas_coverage(SurfaceRole::Overlay),
            AtlasCoverageLevel::ProbeOnly
        );
    }

    #[test]
    fn telemetry_header_is_surface_and_viewport_specific() {
        let header = GeometryFrameHeader::new(
            42,
            "gnos.ux:bar:qualification",
            SurfaceViewport::try_new(900.0, 150.0).expect("viewport"),
            3,
        );
        let json = header.json_fields();
        assert!(json.contains("\"sample\":42"));
        assert!(json.contains("\"namespace\":\"gnos.ux:bar:qualification\""));
        assert!(json.contains("\"viewport\":[900.00,150.00]"));
    }

    #[test]
    fn pointer_region_diagnostic_is_declaration_only() {
        let frame = resolve_dock_frame(
            SurfaceViewport::try_new(900.0, 560.0).expect("viewport"),
            1.0,
        );
        let set = gnos_ux_surface::pointer_regions::project_dock_pointer_regions(&frame)
            .expect("pointer-region projection");
        let diagnostic = pointer_region_diagnostic("gnos.ux:dock", 7, &set);

        assert!(diagnostic.contains("surface=gnos.ux:dock"));
        assert!(diagnostic.contains("revision=7"));
        assert!(diagnostic.contains("dock.input.visible"));
        assert!(diagnostic.contains("dock.input.reveal"));
        for forbidden in ["hover", "focus", "active", "pressed", "targeted"] {
            assert!(!diagnostic.contains(forbidden));
        }
    }
}
