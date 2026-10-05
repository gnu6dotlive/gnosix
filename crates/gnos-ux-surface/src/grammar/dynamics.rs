//! Backend-independent temporal dynamics for reactive GNOS.UX surfaces.

use super::{GeometryError, UnitInterval};

pub const DYNAMICS_GRAMMAR_ID: &str = "gnos.ux.grammar.dynamics";
pub const DYNAMICS_GRAMMAR_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondOrderSpec {
    response: f32,
    damping: f32,
}

impl SecondOrderSpec {
    pub fn try_new(response: f32, damping: f32) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("second_order.response", response),
            ("second_order.damping", damping),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if response < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "second_order.response",
                value: response,
                requirement: ">= 0",
            });
        }
        if damping < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "second_order.damping",
                value: damping,
                requirement: ">= 0",
            });
        }
        Ok(Self { response, damping })
    }

    pub fn new(response: f32, damping: f32) -> Self {
        Self::try_new(response, damping).expect("valid second-order specification")
    }

    pub const fn response(self) -> f32 {
        self.response
    }

    pub const fn damping(self) -> f32 {
        self.damping
    }

    pub fn scaled(self, response_scale: f32, damping_scale: f32) -> Result<Self, GeometryError> {
        if !response_scale.is_finite() {
            return Err(GeometryError::NonFinite("second_order.response_scale"));
        }
        if !damping_scale.is_finite() {
            return Err(GeometryError::NonFinite("second_order.damping_scale"));
        }
        if response_scale < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "second_order.response_scale",
                value: response_scale,
                requirement: ">= 0",
            });
        }
        if damping_scale < 0.0 {
            return Err(GeometryError::InvalidParameter {
                name: "second_order.damping_scale",
                value: damping_scale,
                requirement: ">= 0",
            });
        }
        Self::try_new(self.response * response_scale, self.damping * damping_scale)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondOrderChannel {
    position: f32,
    velocity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondOrderStep {
    pub acceleration: f32,
    pub position: f32,
    pub velocity: f32,
}

impl SecondOrderChannel {
    pub fn try_new(position: f32, velocity: f32) -> Result<Self, GeometryError> {
        if !position.is_finite() {
            return Err(GeometryError::NonFinite("second_order.position"));
        }
        if !velocity.is_finite() {
            return Err(GeometryError::NonFinite("second_order.velocity"));
        }
        Ok(Self { position, velocity })
    }

    pub fn new(position: f32, velocity: f32) -> Self {
        Self::try_new(position, velocity).expect("finite second-order channel")
    }

    pub const fn position(self) -> f32 {
        self.position
    }

    pub const fn velocity(self) -> f32 {
        self.velocity
    }

    pub fn set_state(&mut self, position: f32, velocity: f32) -> Result<(), GeometryError> {
        *self = Self::try_new(position, velocity)?;
        Ok(())
    }

    pub fn step(
        &mut self,
        target: f32,
        spec: SecondOrderSpec,
        dt: f32,
    ) -> Result<SecondOrderStep, GeometryError> {
        if !target.is_finite() {
            return Err(GeometryError::NonFinite("second_order.target"));
        }
        validate_dt(dt)?;

        let acceleration = (target - self.position) * spec.response - self.velocity * spec.damping;
        let velocity = self.velocity + acceleration * dt;
        let position = self.position + velocity * dt;

        if !acceleration.is_finite() {
            return Err(GeometryError::NonFinite("second_order.acceleration"));
        }
        if !velocity.is_finite() {
            return Err(GeometryError::NonFinite("second_order.next_velocity"));
        }
        if !position.is_finite() {
            return Err(GeometryError::NonFinite("second_order.next_position"));
        }

        self.velocity = velocity;
        self.position = position;

        Ok(SecondOrderStep {
            acceleration,
            position,
            velocity,
        })
    }
}

pub fn step_second_order_pair(
    position: &mut f32,
    velocity: &mut f32,
    target: f32,
    spec: SecondOrderSpec,
    dt: f32,
) -> Result<SecondOrderStep, GeometryError> {
    let mut channel = SecondOrderChannel::try_new(*position, *velocity)?;
    let step = channel.step(target, spec, dt)?;
    *position = channel.position;
    *velocity = channel.velocity;
    Ok(step)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalResponseMode {
    Full,
    Reduced,
    Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettlePolicy {
    position_epsilon: f32,
    velocity_epsilon: f32,
    snap_to_target: bool,
}

impl SettlePolicy {
    pub fn try_new(
        position_epsilon: f32,
        velocity_epsilon: f32,
        snap_to_target: bool,
    ) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("settle.position_epsilon", position_epsilon),
            ("settle.velocity_epsilon", velocity_epsilon),
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
            position_epsilon,
            velocity_epsilon,
            snap_to_target,
        })
    }

    pub fn new(position_epsilon: f32, velocity_epsilon: f32, snap_to_target: bool) -> Self {
        Self::try_new(position_epsilon, velocity_epsilon, snap_to_target)
            .expect("valid settle policy")
    }

    pub const fn position_epsilon(self) -> f32 {
        self.position_epsilon
    }

    pub const fn velocity_epsilon(self) -> f32 {
        self.velocity_epsilon
    }

    pub const fn snap_to_target(self) -> bool {
        self.snap_to_target
    }

    pub fn is_settled(self, position: f32, velocity: f32, target: f32) -> bool {
        (position - target).abs() <= self.position_epsilon
            && velocity.abs() <= self.velocity_epsilon
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseResponseProfile {
    pub full: SecondOrderSpec,
    pub reduced: SecondOrderSpec,
    pub settle: SettlePolicy,
}

impl PhaseResponseProfile {
    pub const fn new(
        full: SecondOrderSpec,
        reduced: SecondOrderSpec,
        settle: SettlePolicy,
    ) -> Self {
        Self {
            full,
            reduced,
            settle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseChannel {
    phase: UnitInterval,
    velocity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseStep {
    pub phase: UnitInterval,
    pub velocity: f32,
    pub settled: bool,
}

impl PhaseChannel {
    pub fn try_new(phase: f32, velocity: f32) -> Result<Self, GeometryError> {
        if !velocity.is_finite() {
            return Err(GeometryError::NonFinite("phase.velocity"));
        }
        Ok(Self {
            phase: UnitInterval::try_new(phase)?,
            velocity,
        })
    }

    pub fn clamped(phase: f32, velocity: f32) -> Result<Self, GeometryError> {
        if !velocity.is_finite() {
            return Err(GeometryError::NonFinite("phase.velocity"));
        }
        Ok(Self {
            phase: UnitInterval::clamped(phase),
            velocity,
        })
    }

    pub const fn phase(self) -> UnitInterval {
        self.phase
    }

    pub const fn velocity(self) -> f32 {
        self.velocity
    }

    pub fn step(
        &mut self,
        target: UnitInterval,
        profile: PhaseResponseProfile,
        mode: TemporalResponseMode,
        dt: f32,
    ) -> Result<PhaseStep, GeometryError> {
        let target_value = target.value();

        if mode == TemporalResponseMode::Instant {
            self.phase = target;
            self.velocity = 0.0;
            return Ok(PhaseStep {
                phase: self.phase,
                velocity: self.velocity,
                settled: true,
            });
        }

        let spec = match mode {
            TemporalResponseMode::Full => profile.full,
            TemporalResponseMode::Reduced => profile.reduced,
            TemporalResponseMode::Instant => unreachable!(),
        };

        let mut channel = SecondOrderChannel::new(self.phase.value(), self.velocity);
        channel.step(target_value, spec, dt)?;
        self.phase = UnitInterval::clamped(channel.position());
        self.velocity = channel.velocity();

        let settled = profile
            .settle
            .is_settled(self.phase.value(), self.velocity, target_value);
        if settled && profile.settle.snap_to_target {
            self.phase = target;
            self.velocity = 0.0;
        }

        Ok(PhaseStep {
            phase: self.phase,
            velocity: self.velocity,
            settled,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicsDiagnostics {
    pub final_position: f32,
    pub final_velocity: f32,
    pub peak_overshoot: f32,
    pub settle_time: Option<f32>,
    pub finite: bool,
}

pub fn simulate_step_response(
    initial_position: f32,
    initial_velocity: f32,
    target: f32,
    spec: SecondOrderSpec,
    settle: SettlePolicy,
    dt_sequence: &[f32],
    duration: f32,
) -> Result<DynamicsDiagnostics, GeometryError> {
    if dt_sequence.is_empty() {
        return Err(GeometryError::InvalidParameter {
            name: "dynamics.dt_sequence_len",
            value: 0.0,
            requirement: "> 0",
        });
    }
    if !duration.is_finite() {
        return Err(GeometryError::NonFinite("dynamics.duration"));
    }
    if duration <= 0.0 {
        return Err(GeometryError::InvalidParameter {
            name: "dynamics.duration",
            value: duration,
            requirement: "> 0",
        });
    }
    for &dt in dt_sequence {
        validate_dt(dt)?;
    }

    let mut channel = SecondOrderChannel::try_new(initial_position, initial_velocity)?;
    let mut elapsed = 0.0_f32;
    let mut index = 0usize;
    let mut peak_overshoot = 0.0_f32;
    let mut settle_time = None;
    let direction = (target - initial_position).signum();

    while elapsed < duration {
        let requested = dt_sequence[index % dt_sequence.len()];
        index += 1;
        let dt = requested.min(duration - elapsed);
        if dt <= 0.0 {
            break;
        }
        channel.step(target, spec, dt)?;
        elapsed += dt;

        let directed_error = (channel.position() - target) * direction;
        peak_overshoot = peak_overshoot.max(directed_error.max(0.0));

        if settle_time.is_none()
            && settle.is_settled(channel.position(), channel.velocity(), target)
        {
            settle_time = Some(elapsed);
        }
    }

    Ok(DynamicsDiagnostics {
        final_position: channel.position(),
        final_velocity: channel.velocity(),
        peak_overshoot,
        settle_time,
        finite: channel.position().is_finite() && channel.velocity().is_finite(),
    })
}

fn validate_dt(dt: f32) -> Result<(), GeometryError> {
    if !dt.is_finite() {
        return Err(GeometryError::NonFinite("dynamics.dt"));
    }
    if dt <= 0.0 {
        return Err(GeometryError::InvalidParameter {
            name: "dynamics.dt",
            value: dt,
            requirement: "> 0",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qualified_spec() -> SecondOrderSpec {
        SecondOrderSpec::new(92.0, 13.0)
    }

    fn settle() -> SettlePolicy {
        SettlePolicy::new(0.001, 0.01, true)
    }

    #[test]
    fn second_order_rejects_invalid_state_parameters_and_dt() {
        assert!(SecondOrderSpec::try_new(-1.0, 3.0).is_err());
        assert!(SecondOrderSpec::try_new(1.0, -3.0).is_err());
        assert!(SecondOrderChannel::try_new(f32::NAN, 0.0).is_err());

        let mut channel = SecondOrderChannel::new(0.0, 0.0);
        assert!(channel.step(1.0, qualified_spec(), 0.0).is_err());
        assert!(channel.step(1.0, qualified_spec(), f32::NAN).is_err());
    }

    #[test]
    fn identical_inputs_produce_identical_steps() {
        let spec = qualified_spec();
        let mut a = SecondOrderChannel::new(0.2, -0.1);
        let mut b = a;
        for _ in 0..120 {
            assert_eq!(
                a.step(1.0, spec, 1.0 / 60.0).expect("a"),
                b.step(1.0, spec, 1.0 / 60.0).expect("b")
            );
        }
        assert_eq!(a, b);
    }

    #[test]
    fn qualified_30_60_120_hz_domain_converges_and_stays_finite() {
        for hz in [30.0_f32, 60.0, 120.0] {
            let diagnostics =
                simulate_step_response(0.0, 0.0, 1.0, qualified_spec(), settle(), &[1.0 / hz], 3.0)
                    .expect("diagnostics");
            assert!(diagnostics.finite, "{hz}Hz");
            assert!((diagnostics.final_position - 1.0).abs() < 0.001, "{hz}Hz");
            assert!(diagnostics.final_velocity.abs() < 0.01, "{hz}Hz");
            assert!(diagnostics.settle_time.is_some(), "{hz}Hz");
            assert!(diagnostics.peak_overshoot < 0.06, "{hz}Hz");
        }
    }

    #[test]
    fn bounded_variable_dt_converges() {
        let diagnostics = simulate_step_response(
            0.0,
            0.0,
            1.0,
            qualified_spec(),
            settle(),
            &[1.0 / 120.0, 1.0 / 60.0, 1.0 / 30.0, 1.0 / 75.0],
            3.0,
        )
        .expect("diagnostics");
        assert!(diagnostics.finite);
        assert!((diagnostics.final_position - 1.0).abs() < 0.001);
        assert!(diagnostics.final_velocity.abs() < 0.01);
        assert!(diagnostics.settle_time.is_some());
    }

    #[test]
    fn qualified_step_response_exposes_bounded_time_step_sensitivity() {
        let runs = [30.0_f32, 60.0, 120.0].map(|hz| {
            simulate_step_response(0.0, 0.0, 1.0, qualified_spec(), settle(), &[1.0 / hz], 3.0)
                .expect("diagnostics")
        });
        let min_peak = runs
            .iter()
            .map(|run| run.peak_overshoot)
            .fold(f32::INFINITY, f32::min);
        let max_peak = runs
            .iter()
            .map(|run| run.peak_overshoot)
            .fold(0.0_f32, f32::max);
        assert!(max_peak - min_peak < 0.04);
    }

    #[test]
    fn step_disturbance_a_to_b_to_a_returns_and_settles() {
        let spec = qualified_spec();
        let mut channel = SecondOrderChannel::new(0.0, 0.0);

        for _ in 0..60 {
            channel.step(1.0, spec, 1.0 / 60.0).expect("toward b");
        }
        assert!((channel.position() - 1.0).abs() < 0.01);

        for _ in 0..120 {
            channel.step(0.0, spec, 1.0 / 60.0).expect("return a");
        }
        assert!(channel.position().abs() < 0.001);
        assert!(channel.velocity().abs() < 0.01);
    }

    #[test]
    fn phase_channel_preserves_instant_and_dynamic_semantics() {
        let profile = PhaseResponseProfile::new(
            SecondOrderSpec::new(78.0, 18.0),
            SecondOrderSpec::new(78.0 * 2.25, 18.0 * 1.65),
            SettlePolicy::new(0.0005, 0.01, true),
        );

        let mut instant = PhaseChannel::try_new(0.25, 3.0).expect("phase");
        let step = instant
            .step(
                UnitInterval::ONE,
                profile,
                TemporalResponseMode::Instant,
                1.0 / 60.0,
            )
            .expect("instant");
        assert_eq!(step.phase, UnitInterval::ONE);
        assert_eq!(step.velocity, 0.0);

        let mut full = PhaseChannel::try_new(0.0, 0.0).expect("phase");
        for _ in 0..180 {
            full.step(
                UnitInterval::ONE,
                profile,
                TemporalResponseMode::Full,
                1.0 / 60.0,
            )
            .expect("full");
        }
        assert_eq!(full.phase(), UnitInterval::ONE);
        assert_eq!(full.velocity(), 0.0);
    }
}
