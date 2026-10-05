//! In-process lifecycle and health semantics for the GNOS.UX scene runtime.
//!
//! This module defines what the runtime may truthfully report. It does not
//! select a production supervisor, restart policy, systemd unit or service
//! owner.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeLifecyclePhase {
    Starting,
    Connecting,
    Reconciling,
    Ready,
    Degraded,
    Recovering,
    Stopping,
    Stopped,
    Failed,
}

impl RuntimeLifecyclePhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Connecting => "CONNECTING",
            Self::Reconciling => "RECONCILING",
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
            Self::Recovering => "RECOVERING",
            Self::Stopping => "STOPPING",
            Self::Stopped => "STOPPED",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeHealth {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
    Unavailable,
}

impl RuntimeHealth {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Healthy => "HEALTHY",
            Self::Degraded => "DEGRADED",
            Self::Unhealthy => "UNHEALTHY",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStreamState {
    Unknown,
    Connected,
    Degraded,
    Disabled,
    Disconnected,
    Unavailable,
}

impl EventStreamState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Connected => "CONNECTED",
            Self::Degraded => "DEGRADED",
            Self::Disabled => "DISABLED",
            Self::Disconnected => "DISCONNECTED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeHealthReceipt {
    pub sequence: u64,
    pub phase: RuntimeLifecyclePhase,
    pub health: RuntimeHealth,
    pub ready: bool,
    pub topology_generation: Option<u64>,
    pub route_count: usize,
    pub event_stream: EventStreamState,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleTransitionError {
    InvalidTransition {
        from: RuntimeLifecyclePhase,
        to: RuntimeLifecyclePhase,
    },
    SequenceExhausted,
}

#[derive(Clone, Debug)]
pub struct RuntimeLifecycle {
    phase: RuntimeLifecyclePhase,
    sequence: u64,
    ever_ready: bool,
    topology_generation: Option<u64>,
    route_count: usize,
    event_stream: EventStreamState,
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self {
            phase: RuntimeLifecyclePhase::Starting,
            sequence: 0,
            ever_ready: false,
            topology_generation: None,
            route_count: 0,
            event_stream: EventStreamState::Unknown,
        }
    }
}

impl RuntimeLifecycle {
    pub fn phase(&self) -> RuntimeLifecyclePhase {
        self.phase
    }

    pub fn ever_ready(&self) -> bool {
        self.ever_ready
    }

    pub fn topology_generation(&self) -> Option<u64> {
        self.topology_generation
    }

    pub fn route_count(&self) -> usize {
        self.route_count
    }

    pub fn event_stream(&self) -> EventStreamState {
        self.event_stream
    }

    pub fn transition(
        &mut self,
        next: RuntimeLifecyclePhase,
        topology_generation: Option<u64>,
        route_count: usize,
        event_stream: EventStreamState,
        authoritative_reconcile_committed: bool,
        reason: impl Into<String>,
    ) -> Result<RuntimeHealthReceipt, LifecycleTransitionError> {
        if self.phase != next && !transition_allowed(self.phase, next) {
            return Err(LifecycleTransitionError::InvalidTransition {
                from: self.phase,
                to: next,
            });
        }

        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(LifecycleTransitionError::SequenceExhausted)?;
        self.phase = next;
        self.topology_generation = topology_generation;
        self.route_count = route_count;
        self.event_stream = event_stream;
        if authoritative_reconcile_committed {
            self.ever_ready = true;
        }

        Ok(RuntimeHealthReceipt {
            sequence: self.sequence,
            phase: self.phase,
            health: health_for_phase(self.phase, self.ever_ready),
            ready: readiness_for_phase(self.phase, self.ever_ready),
            topology_generation: self.topology_generation,
            route_count: self.route_count,
            event_stream: self.event_stream,
            reason: reason.into(),
        })
    }

    pub fn initial_receipt(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<RuntimeHealthReceipt, LifecycleTransitionError> {
        self.transition(
            RuntimeLifecyclePhase::Starting,
            None,
            0,
            EventStreamState::Unknown,
            false,
            reason,
        )
    }
}

const fn health_for_phase(phase: RuntimeLifecyclePhase, ever_ready: bool) -> RuntimeHealth {
    match phase {
        RuntimeLifecyclePhase::Starting | RuntimeLifecyclePhase::Connecting => {
            RuntimeHealth::Unknown
        }
        RuntimeLifecyclePhase::Reconciling => {
            if ever_ready {
                RuntimeHealth::Degraded
            } else {
                RuntimeHealth::Unknown
            }
        }
        RuntimeLifecyclePhase::Ready => RuntimeHealth::Healthy,
        RuntimeLifecyclePhase::Degraded | RuntimeLifecyclePhase::Recovering => {
            RuntimeHealth::Degraded
        }
        RuntimeLifecyclePhase::Failed => RuntimeHealth::Unhealthy,
        RuntimeLifecyclePhase::Stopping | RuntimeLifecyclePhase::Stopped => {
            RuntimeHealth::Unavailable
        }
    }
}

const fn readiness_for_phase(phase: RuntimeLifecyclePhase, ever_ready: bool) -> bool {
    if !ever_ready {
        return false;
    }
    !matches!(
        phase,
        RuntimeLifecyclePhase::Starting
            | RuntimeLifecyclePhase::Connecting
            | RuntimeLifecyclePhase::Stopping
            | RuntimeLifecyclePhase::Stopped
            | RuntimeLifecyclePhase::Failed
    )
}

const fn transition_allowed(from: RuntimeLifecyclePhase, to: RuntimeLifecyclePhase) -> bool {
    use RuntimeLifecyclePhase as P;
    match from {
        P::Starting => matches!(to, P::Connecting | P::Failed | P::Stopping),
        P::Connecting => matches!(
            to,
            P::Reconciling | P::Degraded | P::Recovering | P::Failed | P::Stopping
        ),
        P::Reconciling => matches!(
            to,
            P::Ready | P::Degraded | P::Recovering | P::Failed | P::Stopping
        ),
        P::Ready => matches!(
            to,
            P::Reconciling | P::Degraded | P::Recovering | P::Failed | P::Stopping
        ),
        P::Degraded => matches!(
            to,
            P::Reconciling | P::Recovering | P::Ready | P::Failed | P::Stopping
        ),
        P::Recovering => matches!(
            to,
            P::Reconciling | P::Ready | P::Degraded | P::Failed | P::Stopping
        ),
        P::Stopping => matches!(to, P::Stopped | P::Failed),
        P::Failed => matches!(to, P::Stopping | P::Stopped),
        P::Stopped => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(
        lifecycle: &mut RuntimeLifecycle,
        phase: RuntimeLifecyclePhase,
        committed: bool,
    ) -> RuntimeHealthReceipt {
        lifecycle
            .transition(
                phase,
                committed.then_some(1),
                usize::from(committed),
                EventStreamState::Connected,
                committed,
                "test",
            )
            .expect("transition")
    }

    #[test]
    fn pid_start_does_not_imply_readiness() {
        let mut lifecycle = RuntimeLifecycle::default();
        let receipt = lifecycle.initial_receipt("start").expect("receipt");
        assert_eq!(receipt.phase, RuntimeLifecyclePhase::Starting);
        assert_eq!(receipt.health, RuntimeHealth::Unknown);
        assert!(!receipt.ready);
    }

    #[test]
    fn readiness_requires_authoritative_reconcile_commit() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        step(&mut lifecycle, RuntimeLifecyclePhase::Connecting, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        let receipt = step(&mut lifecycle, RuntimeLifecyclePhase::Ready, true);
        assert!(receipt.ready);
        assert_eq!(receipt.health, RuntimeHealth::Healthy);
        assert!(lifecycle.ever_ready());
    }

    #[test]
    fn degraded_event_rail_can_remain_ready() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        step(&mut lifecycle, RuntimeLifecyclePhase::Connecting, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Ready, true);
        let receipt = lifecycle
            .transition(
                RuntimeLifecyclePhase::Degraded,
                Some(1),
                1,
                EventStreamState::Disconnected,
                false,
                "event_stream_lost",
            )
            .expect("degraded");
        assert!(receipt.ready);
        assert_eq!(receipt.health, RuntimeHealth::Degraded);
    }

    #[test]
    fn recovery_after_initial_readiness_remains_serving_but_degraded() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        step(&mut lifecycle, RuntimeLifecyclePhase::Connecting, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Ready, true);
        let receipt = step(&mut lifecycle, RuntimeLifecyclePhase::Recovering, false);
        assert!(receipt.ready);
        assert_eq!(receipt.health, RuntimeHealth::Degraded);
    }

    #[test]
    fn reconciliation_after_readiness_is_degraded_not_unknown() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        step(&mut lifecycle, RuntimeLifecyclePhase::Connecting, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Ready, true);
        let receipt = step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        assert!(receipt.ready);
        assert_eq!(receipt.health, RuntimeHealth::Degraded);
    }

    #[test]
    fn stopping_revokes_readiness() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        step(&mut lifecycle, RuntimeLifecyclePhase::Connecting, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Reconciling, false);
        step(&mut lifecycle, RuntimeLifecyclePhase::Ready, true);
        let stopping = step(&mut lifecycle, RuntimeLifecyclePhase::Stopping, false);
        assert!(!stopping.ready);
        assert_eq!(stopping.health, RuntimeHealth::Unavailable);
        let stopped = step(&mut lifecycle, RuntimeLifecyclePhase::Stopped, false);
        assert!(!stopped.ready);
    }

    #[test]
    fn illegal_transition_is_rejected_without_mutating_phase() {
        let mut lifecycle = RuntimeLifecycle::default();
        lifecycle.initial_receipt("start").expect("start");
        let error = lifecycle
            .transition(
                RuntimeLifecyclePhase::Ready,
                Some(1),
                1,
                EventStreamState::Connected,
                true,
                "invalid",
            )
            .expect_err("must reject");
        assert_eq!(
            error,
            LifecycleTransitionError::InvalidTransition {
                from: RuntimeLifecyclePhase::Starting,
                to: RuntimeLifecyclePhase::Ready,
            }
        );
        assert_eq!(lifecycle.phase(), RuntimeLifecyclePhase::Starting);
        assert!(!lifecycle.ever_ready());
    }
}
