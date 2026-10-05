//! Backend-independent connector/bridge geometry molecules.

use super::GeometryError;

pub const CONNECTOR_GRAMMAR_ID: &str = "gnos.ux.grammar.connectors";
pub const CONNECTOR_GRAMMAR_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectorSpan {
    center: f32,
    neck_half: f32,
    attach_half: f32,
}

impl ConnectorSpan {
    pub fn try_new(center: f32, neck_half: f32, attach_half: f32) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("connector.center", center),
            ("connector.neck_half", neck_half),
            ("connector.attach_half", attach_half),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if neck_half < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "connector.neck_half",
                value: neck_half,
                requirement: ">= 0",
            });
        }
        if attach_half < neck_half {
            return Err(GeometryError::InvalidParameter {
                name: "connector.attach_half",
                value: attach_half,
                requirement: ">= neck_half",
            });
        }
        Ok(Self {
            center,
            neck_half,
            attach_half,
        })
    }

    pub const fn center(self) -> f32 {
        self.center
    }
    pub const fn neck_half(self) -> f32 {
        self.neck_half
    }
    pub const fn attach_half(self) -> f32 {
        self.attach_half
    }
    pub fn neck_left(self) -> f32 {
        self.center - self.neck_half
    }
    pub fn neck_right(self) -> f32 {
        self.center + self.neck_half
    }
    pub fn attach_left(self) -> f32 {
        self.center - self.attach_half
    }
    pub fn attach_right(self) -> f32 {
        self.center + self.attach_half
    }
    pub fn transition_width(self) -> f32 {
        (self.attach_half - self.neck_half).max(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BridgeShoulderRecipe {
    pub transition_factor: f32,
    pub gap_factor: f32,
}

impl BridgeShoulderRecipe {
    pub const fn baseline() -> Self {
        Self {
            transition_factor: 0.72,
            gap_factor: 0.58,
        }
    }

    pub fn resolve(
        self,
        span: ConnectorSpan,
        gap: f32,
        min_run: f32,
        max_run: f32,
    ) -> Result<f32, GeometryError> {
        for (name, value) in [
            ("bridge.transition_factor", self.transition_factor),
            ("bridge.gap_factor", self.gap_factor),
            ("bridge.gap", gap),
            ("bridge.min_run", min_run),
            ("bridge.max_run", max_run),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if self.transition_factor < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bridge.transition_factor",
                value: self.transition_factor,
                requirement: ">= 0",
            });
        }
        if self.gap_factor < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bridge.gap_factor",
                value: self.gap_factor,
                requirement: ">= 0",
            });
        }
        if gap < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "bridge.gap",
                value: gap,
                requirement: ">= 0",
            });
        }
        if min_run < 0.0 || max_run < 0.0 || min_run > max_run {
            return Err(GeometryError::InvalidClamp {
                min: min_run,
                max: max_run,
            });
        }

        let horizontal_transition = span.transition_width().max(0.01);
        Ok((horizontal_transition * self.transition_factor)
            .min(gap * self.gap_factor)
            .clamp(min_run, max_run))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_span_is_symmetric_and_neck_stays_inside_attachment() {
        let span = ConnectorSpan::try_new(100.0, 10.0, 30.0).expect("span");
        assert_eq!(span.neck_left(), 90.0);
        assert_eq!(span.neck_right(), 110.0);
        assert_eq!(span.attach_left(), 70.0);
        assert_eq!(span.attach_right(), 130.0);
        assert!(span.neck_left() >= span.attach_left());
        assert!(span.neck_right() <= span.attach_right());
    }

    #[test]
    fn connector_span_rejects_inverted_neck_and_attachment() {
        assert!(ConnectorSpan::try_new(100.0, 30.0, 10.0).is_err());
        assert!(ConnectorSpan::try_new(100.0, -1.0, 10.0).is_err());
    }

    #[test]
    fn shoulder_recipe_matches_existing_baseline_formula() {
        let span = ConnectorSpan::try_new(100.0, 10.0, 30.0).expect("span");
        let run = BridgeShoulderRecipe::baseline()
            .resolve(span, 60.0, 8.0, 40.0)
            .expect("run");
        let expected = ((30.0_f32 - 10.0) * 0.72_f32)
            .min(60.0_f32 * 0.58_f32)
            .clamp(8.0_f32, 40.0_f32);
        assert!((run - expected).abs() < 0.001);
    }
}
