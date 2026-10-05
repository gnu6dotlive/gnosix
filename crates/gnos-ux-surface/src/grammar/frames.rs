use super::{GeometryRect, SurfaceViewport, UnitInterval};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeIdentity {
    pub id: &'static str,
    pub revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameInvariantViolation {
    InvalidViewport,
    VisibleOutsideEnvelope,
    RevealOutsideViewport,
    InputMismatch,
    RadiusOutOfBounds,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FrameValidation {
    pub violations: Vec<FrameInvariantViolation>,
}

impl FrameValidation {
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    fn push_unique(&mut self, violation: FrameInvariantViolation) {
        if !self.violations.contains(&violation) {
            self.violations.push(violation);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockGeometryFrame {
    pub recipe: RecipeIdentity,
    pub viewport: SurfaceViewport,
    pub envelope: GeometryRect,
    pub visible_envelope: GeometryRect,
    pub reveal_region: GeometryRect,
    pub input_regions: Vec<GeometryRect>,
    pub visible_fraction: UnitInterval,
}

impl DockGeometryFrame {
    pub fn validate(&self) -> FrameValidation {
        let mut validation = FrameValidation::default();

        if self.viewport.validate().is_err() {
            validation.push_unique(FrameInvariantViolation::InvalidViewport);
        }
        if !self.envelope.contains(self.visible_envelope) {
            validation.push_unique(FrameInvariantViolation::VisibleOutsideEnvelope);
        }
        if self.reveal_region.left() < 0.0
            || self.reveal_region.right() > self.viewport.width
            || self.reveal_region.top() < 0.0
            || self.reveal_region.bottom() > self.viewport.height
        {
            validation.push_unique(FrameInvariantViolation::RevealOutsideViewport);
        }

        for rect in [self.envelope, self.visible_envelope, self.reveal_region] {
            if rect.radius() > rect.width() * 0.5 + 0.001
                || rect.radius() > rect.height() * 0.5 + 0.001
            {
                validation.push_unique(FrameInvariantViolation::RadiusOutOfBounds);
            }
        }

        let expected = if self.visible_envelope.height() > 0.5 {
            vec![self.visible_envelope, self.reveal_region]
        } else {
            vec![self.reveal_region]
        };
        if self.input_regions != expected {
            validation.push_unique(FrameInvariantViolation::InputMismatch);
        }

        validation
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarGeometryFrame {
    pub placement: super::ResolvedBarPlacement,
    pub recipe: RecipeIdentity,
    pub viewport: SurfaceViewport,
    pub envelope: GeometryRect,
    pub input_regions: Vec<GeometryRect>,
}

impl BarGeometryFrame {
    pub fn validate(&self) -> FrameValidation {
        let mut validation = FrameValidation::default();

        if self.viewport.validate().is_err() {
            validation.push_unique(FrameInvariantViolation::InvalidViewport);
        }
        if self.envelope.radius() > self.envelope.width() * 0.5 + 0.001
            || self.envelope.radius() > self.envelope.height() * 0.5 + 0.001
        {
            validation.push_unique(FrameInvariantViolation::RadiusOutOfBounds);
        }
        if self.envelope != self.placement.allocation() || self.input_regions != vec![self.envelope]
        {
            validation.push_unique(FrameInvariantViolation::InputMismatch);
        }

        validation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dock_input_is_reported_not_silently_accepted() {
        let viewport = SurfaceViewport::try_new(900.0, 560.0).expect("viewport");
        let envelope = GeometryRect::try_new(30.0, 392.0, 870.0, 496.0, 32.0).expect("envelope");
        let reveal = GeometryRect::try_new(0.0, 496.0, 900.0, 501.3333, 0.0).expect("reveal");
        let frame = DockGeometryFrame {
            recipe: RecipeIdentity {
                id: "test",
                revision: 1,
            },
            viewport,
            envelope,
            visible_envelope: envelope,
            reveal_region: reveal,
            input_regions: vec![reveal],
            visible_fraction: UnitInterval::ONE,
        };

        assert_eq!(
            frame.validate().violations,
            vec![FrameInvariantViolation::InputMismatch]
        );
    }
}
