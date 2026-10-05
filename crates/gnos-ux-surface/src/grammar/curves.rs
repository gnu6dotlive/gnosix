//! Backend-independent curved-geometry primitives and continuity molecules.

use super::GeometryError;

pub const CURVE_GRAMMAR_ID: &str = "gnos.ux.grammar.curves";
pub const CURVE_GRAMMAR_REVISION: u32 = 1;

const VECTOR_EPSILON: f32 = 0.000_1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2 {
    x: f32,
    y: f32,
}

impl Point2 {
    pub fn try_new(x: f32, y: f32) -> Result<Self, GeometryError> {
        if !x.is_finite() {
            return Err(GeometryError::NonFinite("point.x"));
        }
        if !y.is_finite() {
            return Err(GeometryError::NonFinite("point.y"));
        }
        Ok(Self { x, y })
    }

    pub fn new(x: f32, y: f32) -> Self {
        Self::try_new(x, y).expect("finite Point2")
    }

    pub const fn x(self) -> f32 {
        self.x
    }
    pub const fn y(self) -> f32 {
        self.y
    }
    pub const fn tuple(self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    pub fn vector_to(self, other: Self) -> Vector2 {
        Vector2::new(other.x - self.x, other.y - self.y)
    }

    pub fn translated(self, vector: Vector2) -> Self {
        Self::new(self.x + vector.x, self.y + vector.y)
    }

    pub fn minus(self, vector: Vector2) -> Self {
        Self::new(self.x - vector.x, self.y - vector.y)
    }

    pub fn distance_to(self, other: Self) -> f32 {
        self.vector_to(other).length()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector2 {
    x: f32,
    y: f32,
}

impl Vector2 {
    pub fn try_new(x: f32, y: f32) -> Result<Self, GeometryError> {
        if !x.is_finite() {
            return Err(GeometryError::NonFinite("vector.x"));
        }
        if !y.is_finite() {
            return Err(GeometryError::NonFinite("vector.y"));
        }
        Ok(Self { x, y })
    }

    pub fn new(x: f32, y: f32) -> Self {
        Self::try_new(x, y).expect("finite Vector2")
    }

    pub const fn x(self) -> f32 {
        self.x
    }
    pub const fn y(self) -> f32 {
        self.y
    }
    pub const fn tuple(self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        if length <= VECTOR_EPSILON {
            None
        } else {
            Some(Self::new(self.x / length, self.y / length))
        }
    }

    pub fn normalized_or_zero(self) -> Self {
        self.normalized().unwrap_or_else(|| Self::new(0.0, 0.0))
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn scaled(self, scale: f32) -> Self {
        assert!(scale.is_finite(), "finite Vector2 scale");
        Self::new(self.x * scale, self.y * scale)
    }

    pub fn plus(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub fn lerp_direction(self, other: Self, t: f32) -> Self {
        let a = self.normalized_or_zero();
        let b = other.normalized_or_zero();
        Self::new(
            a.x + (b.x - a.x) * t.clamp(0.0, 1.0),
            a.y + (b.y - a.y) * t.clamp(0.0, 1.0),
        )
        .normalized_or_zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub p0: Point2,
    pub c1: Point2,
    pub c2: Point2,
    pub p3: Point2,
}

impl CubicBezier {
    pub const fn new(p0: Point2, c1: Point2, c2: Point2, p3: Point2) -> Self {
        Self { p0, c1, c2, p3 }
    }

    pub fn point(self, t: f32) -> Point2 {
        let t = t.clamp(0.0, 1.0);
        let u = 1.0 - t;
        Point2::new(
            u * u * u * self.p0.x
                + 3.0 * u * u * t * self.c1.x
                + 3.0 * u * t * t * self.c2.x
                + t * t * t * self.p3.x,
            u * u * u * self.p0.y
                + 3.0 * u * u * t * self.c1.y
                + 3.0 * u * t * t * self.c2.y
                + t * t * t * self.p3.y,
        )
    }

    pub fn derivative(self, t: f32) -> Vector2 {
        let t = t.clamp(0.0, 1.0);
        let u = 1.0 - t;
        Vector2::new(
            3.0 * u * u * (self.c1.x - self.p0.x)
                + 6.0 * u * t * (self.c2.x - self.c1.x)
                + 3.0 * t * t * (self.p3.x - self.c2.x),
            3.0 * u * u * (self.c1.y - self.p0.y)
                + 6.0 * u * t * (self.c2.y - self.c1.y)
                + 3.0 * t * t * (self.p3.y - self.c2.y),
        )
    }

    pub fn start_tangent(self) -> Vector2 {
        self.p0.vector_to(self.c1)
    }
    pub fn end_tangent(self) -> Vector2 {
        self.c2.vector_to(self.p3)
    }
    pub fn chord(self) -> Vector2 {
        self.p0.vector_to(self.p3)
    }

    pub fn split_half(self) -> (Self, Self) {
        let a = self.p0.lerp(self.c1, 0.5);
        let b = self.c1.lerp(self.c2, 0.5);
        let c = self.c2.lerp(self.p3, 0.5);
        let d = a.lerp(b, 0.5);
        let e = b.lerp(c, 0.5);
        let mid = d.lerp(e, 0.5);
        (Self::new(self.p0, a, d, mid), Self::new(mid, e, c, self.p3))
    }

    pub fn subsegment(self, start_t: f32, end_t: f32) -> Self {
        let a = start_t.clamp(0.0, 1.0);
        let b = end_t.clamp(a, 1.0);
        let span = b - a;
        let q0 = self.point(a);
        let q3 = self.point(b);
        let d0 = self.derivative(a);
        let d1 = self.derivative(b);
        let q1 = q0.translated(d0.scaled(span / 3.0));
        let q2 = q3.minus(d1.scaled(span / 3.0));
        Self::new(q0, q1, q2, q3)
    }

    pub fn is_directionally_monotone(self, samples: usize, tolerance: f32) -> bool {
        let chord = self.chord().normalized_or_zero();
        if chord.length() <= VECTOR_EPSILON {
            return false;
        }
        let samples = samples.max(2);
        (0..=samples).all(|index| {
            let t = index as f32 / samples as f32;
            self.derivative(t).dot(chord) >= -tolerance.abs()
        })
    }

    pub fn stays_within_chord_slab(self, samples: usize, tolerance: f32) -> bool {
        let chord_vector = self.chord();
        let chord_length = chord_vector.length();
        if chord_length <= VECTOR_EPSILON {
            return false;
        }
        let chord = chord_vector.normalized_or_zero();
        let tolerance = tolerance.abs();
        let samples = samples.max(2);
        let mut previous_projection = -tolerance;

        for index in 0..=samples {
            let t = index as f32 / samples as f32;
            let projection = self.p0.vector_to(self.point(t)).dot(chord);
            if projection < -tolerance || projection > chord_length + tolerance {
                return false;
            }
            if projection + tolerance < previous_projection {
                return false;
            }
            previous_projection = projection;
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicJoinPair {
    pub first: CubicBezier,
    pub second: CubicBezier,
}

impl CubicJoinPair {
    pub fn from_controls(
        p0: Point2,
        c1: Point2,
        c2: Point2,
        p1: Point2,
        c3: Point2,
        c4: Point2,
        p2: Point2,
    ) -> Self {
        Self {
            first: CubicBezier::new(p0, c1, c2, p1),
            second: CubicBezier::new(p1, c3, c4, p2),
        }
    }

    pub fn p0(self) -> Point2 {
        self.first.p0
    }
    pub fn c1(self) -> Point2 {
        self.first.c1
    }
    pub fn c2(self) -> Point2 {
        self.first.c2
    }
    pub fn p1(self) -> Point2 {
        self.first.p3
    }
    pub fn c3(self) -> Point2 {
        self.second.c1
    }
    pub fn c4(self) -> Point2 {
        self.second.c2
    }
    pub fn p2(self) -> Point2 {
        self.second.p3
    }

    pub fn validate(self, samples: usize, tolerance: f32) -> CurveValidation {
        let mut validation = CurveValidation::default();
        if self.first.chord().length() <= VECTOR_EPSILON
            || self.second.chord().length() <= VECTOR_EPSILON
        {
            validation.push_unique(CurveInvariantViolation::DegenerateSegment);
        }

        let incoming = self.first.end_tangent().normalized_or_zero();
        let outgoing = self.second.start_tangent().normalized_or_zero();
        if incoming.length() <= VECTOR_EPSILON
            || outgoing.length() <= VECTOR_EPSILON
            || incoming.dot(outgoing) < 1.0 - tolerance.abs().max(0.001)
        {
            validation.push_unique(CurveInvariantViolation::G1Discontinuity);
        }

        if !self.first.is_directionally_monotone(samples, tolerance)
            || !self.second.is_directionally_monotone(samples, tolerance)
        {
            validation.push_unique(CurveInvariantViolation::DirectionalBacktracking);
        }
        if !self.first.stays_within_chord_slab(samples, tolerance)
            || !self.second.stays_within_chord_slab(samples, tolerance)
        {
            validation.push_unique(CurveInvariantViolation::ChordSlabEscape);
        }
        validation
    }

    pub fn monotonic_morph(self, other: Self, t: f32) -> Self {
        monotonic_join_morph(self, other, t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveInvariantViolation {
    DegenerateSegment,
    G1Discontinuity,
    DirectionalBacktracking,
    ChordSlabEscape,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CurveValidation {
    pub violations: Vec<CurveInvariantViolation>,
}

impl CurveValidation {
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    fn push_unique(&mut self, violation: CurveInvariantViolation) {
        if !self.violations.contains(&violation) {
            self.violations.push(violation);
        }
    }
}

pub fn admissible_direction(preferred: Vector2, chord: Vector2) -> Vector2 {
    let chord = chord.normalized_or_zero();
    let preferred = preferred.normalized_or_zero();
    if preferred.dot(chord) >= 0.08 {
        return preferred;
    }
    preferred
        .scaled(0.20)
        .plus(chord.scaled(0.80))
        .normalized_or_zero()
}

pub fn shared_mid_direction(
    preferred: Vector2,
    first_chord: Vector2,
    second_chord: Vector2,
) -> Vector2 {
    let first = first_chord.normalized_or_zero();
    let second = second_chord.normalized_or_zero();
    let preferred = preferred.normalized_or_zero();
    if preferred.dot(first) >= 0.08 && preferred.dot(second) >= 0.08 {
        return preferred;
    }

    let bisector = first.plus(second).normalized_or_zero();
    if bisector.dot(first) >= 0.0 && bisector.dot(second) >= 0.0 {
        bisector
    } else {
        second
    }
}

pub fn g1_transition_controls(
    start: Point2,
    incoming_direction: Vector2,
    end: Point2,
    outgoing_direction: Vector2,
) -> Option<(Point2, Point2)> {
    let distance = start.distance_to(end);
    if distance <= 0.001 {
        return None;
    }
    let incoming = incoming_direction.normalized_or_zero();
    let outgoing = outgoing_direction.normalized_or_zero();
    if incoming.length() <= VECTOR_EPSILON || outgoing.length() <= VECTOR_EPSILON {
        return None;
    }
    let handle = distance * 0.32;
    Some((
        start.translated(incoming.scaled(handle)),
        end.minus(outgoing.scaled(handle)),
    ))
}

pub fn monotonic_join_morph(a: CubicJoinPair, b: CubicJoinPair, t: f32) -> CubicJoinPair {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.000_001 {
        return a;
    }
    if t >= 0.999_999 {
        return b;
    }

    let p0 = a.p0().lerp(b.p0(), t);
    let p1 = a.p1().lerp(b.p1(), t);
    let p2 = a.p2().lerp(b.p2(), t);
    let chord1 = p0.vector_to(p1);
    let chord2 = p1.vector_to(p2);
    let length1 = chord1.length().max(0.001);
    let length2 = chord2.length().max(0.001);

    let start_direction = admissible_direction(
        a.first
            .start_tangent()
            .lerp_direction(b.first.start_tangent(), t),
        chord1,
    );
    let end_direction = admissible_direction(
        a.second
            .end_tangent()
            .lerp_direction(b.second.end_tangent(), t),
        chord2,
    );
    let a_mid = a
        .first
        .end_tangent()
        .plus(a.second.start_tangent())
        .normalized_or_zero();
    let b_mid = b
        .first
        .end_tangent()
        .plus(b.second.start_tangent())
        .normalized_or_zero();
    let mid_direction = shared_mid_direction(a_mid.lerp_direction(b_mid, t), chord1, chord2);

    let h0 = (a.p0().distance_to(a.c1())
        + (b.p0().distance_to(b.c1()) - a.p0().distance_to(a.c1())) * t)
        .min(length1 * 0.30);
    let h1_in = (a.p1().distance_to(a.c2())
        + (b.p1().distance_to(b.c2()) - a.p1().distance_to(a.c2())) * t)
        .min(length1 * 0.30);
    let h1_out = (a.p1().distance_to(a.c3())
        + (b.p1().distance_to(b.c3()) - a.p1().distance_to(a.c3())) * t)
        .min(length2 * 0.30);
    let h2 = (a.p2().distance_to(a.c4())
        + (b.p2().distance_to(b.c4()) - a.p2().distance_to(a.c4())) * t)
        .min(length2 * 0.30);

    CubicJoinPair::from_controls(
        p0,
        p0.translated(start_direction.scaled(h0)),
        p1.minus(mid_direction.scaled(h1_in)),
        p1,
        p1.translated(mid_direction.scaled(h1_out)),
        p2.minus(end_direction.scaled(h2)),
        p2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn cubic_endpoints_and_derivatives_are_exact() {
        let curve = CubicBezier::new(p(0.0, 0.0), p(10.0, 0.0), p(20.0, 20.0), p(30.0, 20.0));
        assert_eq!(curve.point(0.0), curve.p0);
        assert_eq!(curve.point(1.0), curve.p3);
        assert_eq!(curve.derivative(0.0), Vector2::new(30.0, 0.0));
        assert_eq!(curve.derivative(1.0), Vector2::new(30.0, 0.0));
    }

    #[test]
    fn split_and_subsegment_preserve_curve_endpoints() {
        let curve = CubicBezier::new(p(0.0, 0.0), p(10.0, 0.0), p(20.0, 20.0), p(30.0, 20.0));
        let (left, right) = curve.split_half();
        assert_eq!(left.p0, curve.p0);
        assert_eq!(right.p3, curve.p3);
        assert_eq!(left.p3, right.p0);
        assert_eq!(left.p3, curve.point(0.5));
        let sub = curve.subsegment(0.25, 0.75);
        assert_eq!(sub.p0, curve.point(0.25));
        assert_eq!(sub.p3, curve.point(0.75));
    }

    #[test]
    fn g1_controls_align_with_declared_endpoint_tangents() {
        let start = p(0.0, 0.0);
        let end = p(20.0, 20.0);
        let (c1, c2) =
            g1_transition_controls(start, Vector2::new(1.0, 0.0), end, Vector2::new(0.0, 1.0))
                .expect("controls");
        let curve = CubicBezier::new(start, c1, c2, end);
        assert!(
            curve
                .start_tangent()
                .normalized_or_zero()
                .dot(Vector2::new(1.0, 0.0))
                > 0.999
        );
        assert!(
            curve
                .end_tangent()
                .normalized_or_zero()
                .dot(Vector2::new(0.0, 1.0))
                > 0.999
        );
    }

    #[test]
    fn deliberate_backtracking_is_detected() {
        let curve = CubicBezier::new(p(0.0, 0.0), p(-20.0, 0.0), p(80.0, 0.0), p(100.0, 0.0));
        assert!(!curve.is_directionally_monotone(32, 0.001));
        assert!(!curve.stays_within_chord_slab(32, 0.001));
    }

    #[test]
    fn monotonic_join_morph_keeps_intermediate_segments_forward() {
        let a = CubicJoinPair::from_controls(
            p(0.0, 0.0),
            p(20.0, 0.0),
            p(35.0, 20.0),
            p(50.0, 30.0),
            p(65.0, 40.0),
            p(80.0, 60.0),
            p(100.0, 60.0),
        );
        let b = CubicJoinPair::from_controls(
            p(0.0, 0.0),
            p(15.0, 5.0),
            p(35.0, 10.0),
            p(50.0, 30.0),
            p(65.0, 50.0),
            p(85.0, 55.0),
            p(100.0, 60.0),
        );
        for phase in [0.0_f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let morphed = monotonic_join_morph(a, b, phase);
            assert!(morphed.first.is_directionally_monotone(32, 0.001));
            assert!(morphed.second.is_directionally_monotone(32, 0.001));
        }
    }
}
