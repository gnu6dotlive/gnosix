//! Backend-independent soft spatial influence primitives.
//!
//! Influence is advisory. It is not a hard collision constraint and does not
//! change topology authority.

use super::{GeometryError, GeometryRect, Vector2};

pub const INFLUENCE_GRAMMAR_ID: &str = "gnos.ux.grammar.influences";
pub const INFLUENCE_GRAMMAR_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InfluenceSample {
    pub vector: Vector2,
    pub pressure: f32,
}

impl InfluenceSample {
    pub fn zero() -> Self {
        Self {
            vector: Vector2::new(0.0, 0.0),
            pressure: 0.0,
        }
    }

    pub fn try_new(vector: Vector2, pressure: f32) -> Result<Self, GeometryError> {
        if !pressure.is_finite() {
            return Err(GeometryError::NonFinite("influence.pressure"));
        }
        if pressure < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "influence.pressure",
                value: pressure,
                requirement: ">= 0",
            });
        }
        Ok(Self { vector, pressure })
    }

    pub fn scaled_vector(self, scale: f32) -> Result<Self, GeometryError> {
        if !scale.is_finite() {
            return Err(GeometryError::NonFinite("influence.vector_scale"));
        }
        Ok(Self {
            vector: self.vector.scaled(scale),
            pressure: self.pressure,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectInfluenceField {
    region: GeometryRect,
    radius: f32,
    strength: f32,
}

impl RectInfluenceField {
    pub fn try_new(
        region: GeometryRect,
        radius: f32,
        strength: f32,
    ) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("influence.radius", radius),
            ("influence.strength", strength),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
            if value < 0.0 {
                return Err(GeometryError::InvalidParameter {
                    name,
                    value,
                    requirement: ">= 0",
                });
            }
        }
        Ok(Self {
            region,
            radius,
            strength,
        })
    }

    pub const fn region(self) -> GeometryRect {
        self.region
    }

    pub const fn radius(self) -> f32 {
        self.radius
    }

    pub const fn strength(self) -> f32 {
        self.strength
    }

    pub fn sample(self, body: GeometryRect) -> InfluenceSample {
        if self.radius <= f32::EPSILON || self.strength <= f32::EPSILON {
            return InfluenceSample::zero();
        }

        let soft = self.radius.max(1.0);
        let rect = self.region;
        let vertical_near = body.bottom() > rect.top() - soft && body.top() < rect.bottom() + soft;
        let horizontal_near =
            body.right() > rect.left() - soft && body.left() < rect.right() + soft;

        let mut fx = 0.0_f32;
        let mut fy = 0.0_f32;
        let mut pressure = 0.0_f32;

        if vertical_near {
            if body.right() <= rect.left() {
                let gap = rect.left() - body.right();
                let weight = ((soft - gap) / soft).clamp(0.0, 1.0);
                fx -= weight * soft * self.strength;
                pressure += weight * soft;
            } else if body.left() >= rect.right() {
                let gap = body.left() - rect.right();
                let weight = ((soft - gap) / soft).clamp(0.0, 1.0);
                fx += weight * soft * self.strength;
                pressure += weight * soft;
            } else {
                let body_center = (body.left() + body.right()) * 0.5;
                let field_center = (rect.left() + rect.right()) * 0.5;
                let sign = if body_center <= field_center {
                    -1.0
                } else {
                    1.0
                };
                let overlap =
                    (body.right().min(rect.right()) - body.left().max(rect.left())).max(0.0);
                fx += sign * (soft + overlap) * self.strength;
                pressure += soft + overlap;
            }
        }

        if horizontal_near {
            if body.bottom() <= rect.top() {
                let gap = rect.top() - body.bottom();
                let weight = ((soft - gap) / soft).clamp(0.0, 1.0);
                fy -= weight * soft * self.strength;
                pressure += weight * soft;
            } else if body.top() >= rect.bottom() {
                let gap = body.top() - rect.bottom();
                let weight = ((soft - gap) / soft).clamp(0.0, 1.0);
                fy += weight * soft * self.strength;
                pressure += weight * soft;
            }
        }

        InfluenceSample {
            vector: Vector2::new(fx, fy),
            pressure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> GeometryRect {
        GeometryRect::try_new(left, top, right, bottom, 0.0).expect("rect")
    }

    #[test]
    fn field_rejects_negative_or_non_finite_parameters() {
        let region = rect(100.0, 100.0, 200.0, 200.0);
        assert!(RectInfluenceField::try_new(region, -1.0, 1.0).is_err());
        assert!(RectInfluenceField::try_new(region, 20.0, -1.0).is_err());
        assert!(RectInfluenceField::try_new(region, f32::NAN, 1.0).is_err());
    }

    #[test]
    fn outside_radius_has_zero_influence() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 20.0, 1.0)
            .expect("field");
        let sample = field.sample(rect(0.0, 100.0, 50.0, 150.0));
        assert_eq!(sample.vector, Vector2::new(0.0, 0.0));
        assert_eq!(sample.pressure, 0.0);
    }

    #[test]
    fn pressure_increases_while_approaching_field() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 1.0)
            .expect("field");
        let far = field.sample(rect(20.0, 110.0, 65.0, 150.0));
        let near = field.sample(rect(30.0, 110.0, 90.0, 150.0));
        assert!(near.pressure > far.pressure);
        assert!(near.vector.x().abs() > far.vector.x().abs());
    }

    #[test]
    fn left_and_right_sampling_are_symmetric() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 0.75)
            .expect("field");
        let left = field.sample(rect(30.0, 110.0, 90.0, 150.0));
        let right = field.sample(rect(210.0, 110.0, 270.0, 150.0));
        assert!((left.vector.x() + right.vector.x()).abs() < 0.001);
        assert!((left.pressure - right.pressure).abs() < 0.001);
    }

    #[test]
    fn top_and_bottom_sampling_are_symmetric() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 0.75)
            .expect("field");
        let top = field.sample(rect(110.0, 30.0, 150.0, 90.0));
        let bottom = field.sample(rect(110.0, 210.0, 150.0, 270.0));
        assert!((top.vector.y() + bottom.vector.y()).abs() < 0.001);
        assert!((top.pressure - bottom.pressure).abs() < 0.001);
    }

    #[test]
    fn overlap_yields_finite_escape_vector() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 1.0)
            .expect("field");
        let sample = field.sample(rect(70.0, 120.0, 130.0, 170.0));
        assert!(sample.vector.x().is_finite());
        assert!(sample.vector.y().is_finite());
        assert!(sample.pressure.is_finite());
        assert!(sample.pressure > 0.0);
        assert!(sample.vector.x() < 0.0);
    }

    #[test]
    fn zero_strength_has_zero_advisory_vector() {
        let field = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 0.0)
            .expect("field");
        let sample = field.sample(rect(30.0, 110.0, 90.0, 150.0));
        assert_eq!(sample.vector, Vector2::new(0.0, 0.0));
        assert_eq!(sample.pressure, 0.0);
    }

    #[test]
    fn uniformly_rebased_geometry_preserves_normalized_response() {
        let base = RectInfluenceField::try_new(rect(100.0, 100.0, 200.0, 200.0), 40.0, 0.8)
            .expect("field");
        let base_sample = base.sample(rect(30.0, 110.0, 90.0, 150.0));

        let scaled = RectInfluenceField::try_new(rect(200.0, 200.0, 400.0, 400.0), 80.0, 0.8)
            .expect("field");
        let scaled_sample = scaled.sample(rect(60.0, 220.0, 180.0, 300.0));

        assert!((scaled_sample.vector.x() / 2.0 - base_sample.vector.x()).abs() < 0.001);
        assert!((scaled_sample.vector.y() / 2.0 - base_sample.vector.y()).abs() < 0.001);
        assert!((scaled_sample.pressure / 2.0 - base_sample.pressure).abs() < 0.001);
    }
}
