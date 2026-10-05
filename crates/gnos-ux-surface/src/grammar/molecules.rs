//! Reusable geometry behaviours composed from grammar atoms.

use super::{GeometryError, GeometryRect, ResponsiveLength, SurfaceViewport, UnitInterval};

pub fn visible_from_bottom(
    envelope: GeometryRect,
    visible_fraction: UnitInterval,
) -> Result<GeometryRect, GeometryError> {
    let visible_height = envelope.height() * visible_fraction.value();
    GeometryRect::try_new(
        envelope.left(),
        envelope.bottom() - visible_height,
        envelope.right(),
        envelope.bottom(),
        envelope.radius().min(visible_height * 0.5),
    )
}

pub fn reveal_strip_below(
    envelope: GeometryRect,
    viewport: SurfaceViewport,
    height: ResponsiveLength,
) -> Result<GeometryRect, GeometryError> {
    let requested_height = height.resolve(viewport)?;
    let available_height = (viewport.height - envelope.bottom()).max(0.0);
    let resolved_height = requested_height.min(available_height);
    GeometryRect::try_new(
        0.0,
        envelope.bottom(),
        viewport.width,
        envelope.bottom() + resolved_height,
        0.0,
    )
}

pub fn visible_plus_reveal_input(
    visible: GeometryRect,
    reveal: GeometryRect,
    minimum_visible_extent: f32,
) -> Result<Vec<GeometryRect>, GeometryError> {
    if !minimum_visible_extent.is_finite() {
        return Err(GeometryError::NonFinite("minimum_visible_extent"));
    }

    let mut regions = Vec::with_capacity(2);
    if visible.height() > minimum_visible_extent.max(0.0) {
        regions.push(visible);
    }
    regions.push(reveal);
    Ok(regions)
}

pub fn envelope_only_input(envelope: GeometryRect) -> Vec<GeometryRect> {
    vec![envelope]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_visibility_preserves_bottom_edge_and_bounds_radius() {
        let envelope =
            GeometryRect::try_new(40.0, 504.0, 1160.0, 637.7143, 41.142857).expect("envelope");
        let half = visible_from_bottom(envelope, UnitInterval::try_new(0.5).expect("fraction"))
            .expect("visible");

        assert!((half.bottom() - envelope.bottom()).abs() < 0.001);
        assert!((half.top() - 570.857_2).abs() < 0.01);
        assert!(half.radius() <= half.height() * 0.5 + 0.001);
    }

    #[test]
    fn hidden_visibility_collapses_without_inverting() {
        let envelope = GeometryRect::try_new(30.0, 392.0, 870.0, 496.0, 32.0).expect("envelope");
        let hidden = visible_from_bottom(envelope, UnitInterval::ZERO).expect("hidden");
        assert_eq!(hidden.top(), hidden.bottom());
        assert_eq!(hidden.radius(), 0.0);
    }

    #[test]
    fn reveal_strip_is_bounded_during_tiny_compositor_configures() {
        let viewport = SurfaceViewport::unit();
        let envelope = GeometryRect::try_new(
            viewport.width / 30.0,
            viewport.height * 0.70,
            viewport.width * (29.0 / 30.0),
            viewport.height * (31.0 / 35.0),
            viewport.min_axis() * (2.0 / 35.0),
        )
        .expect("envelope");
        let reveal = reveal_strip_below(
            envelope,
            viewport,
            ResponsiveLength::height(1.0 / 105.0).with_min(1.0),
        )
        .expect("reveal");

        assert!(reveal.bottom() <= viewport.height);
        assert!(reveal.top() >= 0.0);
        assert!(reveal.height() > 0.0);
    }

    #[test]
    fn reveal_input_remains_when_visible_extent_is_zero() {
        let envelope = GeometryRect::try_new(30.0, 392.0, 870.0, 496.0, 32.0).expect("envelope");
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let reveal = reveal_strip_below(
            envelope,
            viewport,
            ResponsiveLength::height(1.0 / 105.0).with_min(1.0),
        )
        .expect("reveal");
        let hidden = visible_from_bottom(envelope, UnitInterval::ZERO).expect("hidden");
        let input = visible_plus_reveal_input(hidden, reveal, 0.5).expect("input");

        assert_eq!(input, vec![reveal]);
    }
}
