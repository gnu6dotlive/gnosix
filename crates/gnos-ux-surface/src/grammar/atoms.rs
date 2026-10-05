use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryError {
    InvalidDirectionalPlacement {
        signed_gap: f32,
    },
    InvalidOuterContour {
        index: usize,
        reason: &'static str,
    },
    NonFinite(&'static str),
    NonPositiveViewport {
        width: f32,
        height: f32,
    },
    UnitIntervalOutOfRange(f32),
    InvertedRect {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    NegativeRadius(f32),
    InvalidClamp {
        min: f32,
        max: f32,
    },
    InvalidParameter {
        name: &'static str,
        value: f32,
        requirement: &'static str,
    },
}

impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirectionalPlacement { signed_gap } => {
                write!(f, "child crosses the parent joint: signed gap {signed_gap}")
            }
            Self::InvalidOuterContour { index, reason } => {
                write!(f, "invalid outer contour point {index}: {reason}")
            }
            Self::NonFinite(field) => write!(f, "non-finite geometry value: {field}"),
            Self::NonPositiveViewport { width, height } => {
                write!(f, "viewport must be positive, got {width}x{height}")
            }
            Self::UnitIntervalOutOfRange(value) => {
                write!(f, "unit interval value must be in 0..=1, got {value}")
            }
            Self::InvertedRect {
                left,
                top,
                right,
                bottom,
            } => write!(
                f,
                "rectangle is inverted: left={left} top={top} right={right} bottom={bottom}"
            ),
            Self::NegativeRadius(radius) => write!(f, "radius must be non-negative, got {radius}"),
            Self::InvalidClamp { min, max } => {
                write!(f, "invalid clamp: min={min} exceeds max={max}")
            }
            Self::InvalidParameter {
                name,
                value,
                requirement,
            } => write!(
                f,
                "invalid geometry parameter {name}={value}: expected {requirement}"
            ),
        }
    }
}

impl Error for GeometryError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceViewport {
    pub width: f32,
    pub height: f32,
}

impl SurfaceViewport {
    pub const fn unit() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
        }
    }

    pub fn try_new(width: f32, height: f32) -> Result<Self, GeometryError> {
        if !width.is_finite() {
            return Err(GeometryError::NonFinite("viewport.width"));
        }
        if !height.is_finite() {
            return Err(GeometryError::NonFinite("viewport.height"));
        }
        if width <= 0.0 || height <= 0.0 {
            return Err(GeometryError::NonPositiveViewport { width, height });
        }
        Ok(Self { width, height })
    }

    pub fn validate(self) -> Result<Self, GeometryError> {
        Self::try_new(self.width, self.height)
    }

    pub fn min_axis(self) -> f32 {
        self.width.min(self.height)
    }

    pub fn max_axis(self) -> f32 {
        self.width.max(self.height)
    }

    pub fn approx_eq(self, other: Self) -> bool {
        (self.width - other.width).abs() < 0.5 && (self.height - other.height).abs() < 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct UnitInterval(f32);

impl UnitInterval {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn try_new(value: f32) -> Result<Self, GeometryError> {
        if !value.is_finite() {
            return Err(GeometryError::NonFinite("unit_interval"));
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(GeometryError::UnitIntervalOutOfRange(value));
        }
        Ok(Self(value))
    }

    pub fn clamped(value: f32) -> Self {
        if value.is_nan() {
            return Self::ZERO;
        }
        Self(value.clamp(0.0, 1.0))
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthBasis {
    Absolute,
    ViewportWidth,
    ViewportHeight,
    MinAxis,
    MaxAxis,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsiveLength {
    pub basis: LengthBasis,
    pub factor: f32,
    pub offset: f32,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl ResponsiveLength {
    pub const fn absolute(value: f32) -> Self {
        Self {
            basis: LengthBasis::Absolute,
            factor: 0.0,
            offset: value,
            min: None,
            max: None,
        }
    }

    pub const fn width(factor: f32) -> Self {
        Self {
            basis: LengthBasis::ViewportWidth,
            factor,
            offset: 0.0,
            min: None,
            max: None,
        }
    }

    pub const fn height(factor: f32) -> Self {
        Self {
            basis: LengthBasis::ViewportHeight,
            factor,
            offset: 0.0,
            min: None,
            max: None,
        }
    }

    pub const fn min_axis(factor: f32) -> Self {
        Self {
            basis: LengthBasis::MinAxis,
            factor,
            offset: 0.0,
            min: None,
            max: None,
        }
    }

    pub const fn max_axis(factor: f32) -> Self {
        Self {
            basis: LengthBasis::MaxAxis,
            factor,
            offset: 0.0,
            min: None,
            max: None,
        }
    }

    pub const fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub const fn with_min(mut self, min: f32) -> Self {
        self.min = Some(min);
        self
    }

    pub const fn with_max(mut self, max: f32) -> Self {
        self.max = Some(max);
        self
    }

    pub fn resolve(self, viewport: SurfaceViewport) -> Result<f32, GeometryError> {
        viewport.validate()?;
        if !self.factor.is_finite() {
            return Err(GeometryError::NonFinite("responsive_length.factor"));
        }
        if !self.offset.is_finite() {
            return Err(GeometryError::NonFinite("responsive_length.offset"));
        }
        if let Some(min) = self.min
            && !min.is_finite()
        {
            return Err(GeometryError::NonFinite("responsive_length.min"));
        }
        if let Some(max) = self.max
            && !max.is_finite()
        {
            return Err(GeometryError::NonFinite("responsive_length.max"));
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min > max
        {
            return Err(GeometryError::InvalidClamp { min, max });
        }

        let basis = match self.basis {
            LengthBasis::Absolute => 0.0,
            LengthBasis::ViewportWidth => viewport.width,
            LengthBasis::ViewportHeight => viewport.height,
            LengthBasis::MinAxis => viewport.min_axis(),
            LengthBasis::MaxAxis => viewport.max_axis(),
        };
        let mut value = basis * self.factor + self.offset;
        if let Some(min) = self.min {
            value = value.max(min);
        }
        if let Some(max) = self.max {
            value = value.min(max);
        }
        if !value.is_finite() {
            return Err(GeometryError::NonFinite("responsive_length.resolved"));
        }
        Ok(value)
    }
}

/// Internal, fluid visual-radius token resolved independently for each output.
///
/// Values are candidate defaults. The short-axis reference and scale bounds are
/// dimensionless design calibration, not a fixed radius or user preference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsiveRadiusToken {
    reference_min_axis: f32,
    reference_radius_ratio: f32,
    minimum_scale: f32,
    maximum_scale: f32,
}

impl ResponsiveRadiusToken {
    /// Candidate calibration recovered from the earlier Geometry Lab silhouette.
    pub const fn candidate_default() -> Self {
        Self {
            reference_min_axis: 720.0,
            reference_radius_ratio: 2.0 / 35.0,
            minimum_scale: 0.55,
            maximum_scale: 1.15,
        }
    }

    pub fn resolve(self, viewport: SurfaceViewport) -> Result<f32, GeometryError> {
        viewport.validate()?;
        for (name, value) in [
            (
                "responsive_radius.reference_min_axis",
                self.reference_min_axis,
            ),
            (
                "responsive_radius.reference_ratio",
                self.reference_radius_ratio,
            ),
            ("responsive_radius.minimum_scale", self.minimum_scale),
            ("responsive_radius.maximum_scale", self.maximum_scale),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if self.reference_min_axis <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "responsive_radius.reference_min_axis",
                value: self.reference_min_axis,
                requirement: "> 0",
            });
        }
        if self.reference_radius_ratio <= 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "responsive_radius.reference_ratio",
                value: self.reference_radius_ratio,
                requirement: "> 0",
            });
        }
        if self.minimum_scale <= 0.0 || self.maximum_scale < self.minimum_scale {
            return Err(GeometryError::InvalidClamp {
                min: self.minimum_scale,
                max: self.maximum_scale,
            });
        }
        let scale = (viewport.min_axis() / self.reference_min_axis)
            .clamp(self.minimum_scale, self.maximum_scale);
        let radius = self.reference_min_axis * self.reference_radius_ratio * scale;
        if !radius.is_finite() {
            return Err(GeometryError::NonFinite("responsive_radius.resolved"));
        }
        Ok(radius)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
}

impl GeometryRect {
    pub fn try_new(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        radius: f32,
    ) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("rect.left", left),
            ("rect.top", top),
            ("rect.right", right),
            ("rect.bottom", bottom),
            ("rect.radius", radius),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if right < left || bottom < top {
            return Err(GeometryError::InvertedRect {
                left,
                top,
                right,
                bottom,
            });
        }
        if radius < 0.0 {
            return Err(GeometryError::NegativeRadius(radius));
        }
        let max_radius = ((right - left) * 0.5).min((bottom - top) * 0.5);
        Ok(Self {
            left,
            top,
            right,
            bottom,
            radius: radius.min(max_radius.max(0.0)),
        })
    }

    pub const fn left(self) -> f32 {
        self.left
    }

    pub const fn top(self) -> f32 {
        self.top
    }

    pub const fn right(self) -> f32 {
        self.right
    }

    pub const fn bottom(self) -> f32 {
        self.bottom
    }

    pub const fn radius(self) -> f32 {
        self.radius
    }

    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }

    pub fn contains(self, other: Self) -> bool {
        other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn contains_point(self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }

    pub fn bounded_radius(self, requested: f32) -> Result<Self, GeometryError> {
        Self::try_new(self.left, self.top, self.right, self.bottom, requested)
    }

    pub fn translate(self, dx: f32, dy: f32) -> Result<Self, GeometryError> {
        Self::try_new(
            self.left + dx,
            self.top + dy,
            self.right + dx,
            self.bottom + dy,
            self.radius,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_interval_rejects_out_of_range_and_non_finite_values() {
        assert_eq!(UnitInterval::try_new(0.5).expect("valid").value(), 0.5);
        assert!(UnitInterval::try_new(-0.1).is_err());
        assert!(UnitInterval::try_new(1.1).is_err());
        assert!(UnitInterval::try_new(f32::NAN).is_err());
    }

    #[test]
    fn responsive_length_is_viewport_driven_without_reference_resolution() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let resolved_width = ResponsiveLength::width(1.0 / 30.0)
            .resolve(viewport)
            .expect("width");
        assert!((resolved_width - 30.0).abs() < 0.001);
        assert_eq!(
            ResponsiveLength::height(1.0 / 105.0)
                .with_min(1.0)
                .resolve(viewport)
                .expect("height"),
            560.0 / 105.0
        );
    }

    #[test]
    fn responsive_radius_tracks_output_short_axis_and_saturates_at_scale_bounds() {
        let token = ResponsiveRadiusToken::candidate_default();
        let compact = token
            .resolve(SurfaceViewport::try_new(1280.0, 320.0).unwrap())
            .unwrap();
        let reference = token
            .resolve(SurfaceViewport::try_new(1280.0, 720.0).unwrap())
            .unwrap();
        let wide = token
            .resolve(SurfaceViewport::try_new(1920.0, 1080.0).unwrap())
            .unwrap();
        let very_wide = token
            .resolve(SurfaceViewport::try_new(3840.0, 2160.0).unwrap())
            .unwrap();
        let rotated_reference = token
            .resolve(SurfaceViewport::try_new(720.0, 1280.0).unwrap())
            .unwrap();

        assert!(compact < reference);
        assert!(reference < wide);
        assert_eq!(wide, very_wide);
        assert_eq!(reference, rotated_reference);
    }

    #[test]
    fn geometry_rect_bounds_radius_to_resolved_box() {
        let rect = GeometryRect::try_new(0.0, 0.0, 100.0, 20.0, 40.0).expect("rect");
        assert_eq!(rect.radius(), 10.0);
        assert_eq!(rect.width(), 100.0);
        assert_eq!(rect.height(), 20.0);
    }

    #[test]
    fn viewport_validation_rejects_non_positive_dimensions() {
        assert!(SurfaceViewport::try_new(0.0, 10.0).is_err());
        assert!(SurfaceViewport::try_new(10.0, -1.0).is_err());
        assert!(SurfaceViewport::try_new(f32::INFINITY, 10.0).is_err());
    }
}
