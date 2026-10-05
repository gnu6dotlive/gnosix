//! Surface catalogue exposed by the internal GNOS.UX SDK façade.

use gnos_ux_gpui::{
    geometry_lab::{AtlasCoverageLevel, surface_atlas_coverage},
    try_surface_plan,
};
use gnos_ux_surface::SurfaceRole;

use crate::lab::{LabTarget, scenarios_for};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductRuntimeLevel {
    GpuiProduct,
    ProbeOnly,
}

impl ProductRuntimeLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GpuiProduct => "GPUI_PRODUCT",
            Self::ProbeOnly => "PROBE_ONLY",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSdkEntry {
    pub role: SurfaceRole,
    pub identity: &'static str,
    pub namespace: &'static str,
    pub runtime: ProductRuntimeLevel,
    pub atlas: AtlasCoverageLevel,
    pub scenario_count: usize,
}

fn target_for_role(role: SurfaceRole) -> Option<LabTarget> {
    match role {
        SurfaceRole::Bar => Some(LabTarget::Bar),
        SurfaceRole::Dock => Some(LabTarget::Dock),
        SurfaceRole::Rail | SurfaceRole::Overlay => None,
    }
}

pub fn surface_entry(role: SurfaceRole) -> SurfaceSdkEntry {
    let runtime = if try_surface_plan(role).is_some() {
        ProductRuntimeLevel::GpuiProduct
    } else {
        ProductRuntimeLevel::ProbeOnly
    };
    let scenario_count = target_for_role(role)
        .map(|target| scenarios_for(target).count())
        .unwrap_or(0);

    SurfaceSdkEntry {
        role,
        identity: role.product_identity(),
        namespace: role.namespace(),
        runtime,
        atlas: surface_atlas_coverage(role),
        scenario_count,
    }
}

pub fn surface_catalog() -> [SurfaceSdkEntry; 4] {
    [
        surface_entry(SurfaceRole::Bar),
        surface_entry(SurfaceRole::Dock),
        surface_entry(SurfaceRole::Rail),
        surface_entry(SurfaceRole::Overlay),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_matches_current_product_and_probe_boundaries() {
        let catalog = surface_catalog();

        assert_eq!(catalog[0].role, SurfaceRole::Bar);
        assert_eq!(catalog[0].runtime, ProductRuntimeLevel::GpuiProduct);
        assert_eq!(catalog[0].atlas, AtlasCoverageLevel::AtlasRich);
        assert!(catalog[0].scenario_count > 0);

        assert_eq!(catalog[1].role, SurfaceRole::Dock);
        assert_eq!(catalog[1].runtime, ProductRuntimeLevel::GpuiProduct);
        assert_eq!(catalog[1].atlas, AtlasCoverageLevel::AtlasRich);
        assert!(catalog[1].scenario_count > 0);

        for entry in &catalog[2..] {
            assert_eq!(entry.runtime, ProductRuntimeLevel::ProbeOnly);
            assert_eq!(entry.atlas, AtlasCoverageLevel::ProbeOnly);
            assert_eq!(entry.scenario_count, 0);
        }
    }

    #[test]
    fn catalogue_preserves_canonical_identity_and_namespace() {
        let dock = surface_entry(SurfaceRole::Dock);
        assert_eq!(dock.identity, "gnos.ux.dock");
        assert_eq!(dock.namespace, "gnos.ux:dock");

        let bar = surface_entry(SurfaceRole::Bar);
        assert_eq!(bar.identity, "gnos.ux.bar");
        assert_eq!(bar.namespace, "gnos.ux:bar");
    }
}
