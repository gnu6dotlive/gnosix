//! The scene process's lifecycle: shutdown signals, health receipts and
//! the performance counters.

use super::*;

pub(super) struct ShutdownSignals {
    pub(super) sigterm: Arc<AtomicBool>,
    pub(super) sigint: Arc<AtomicBool>,
    pub(super) receiver: async_mpsc::UnboundedReceiver<&'static str>,
    pub(super) handle: SignalHandle,
    pub(super) thread: Option<thread::JoinHandle<()>>,
}

impl ShutdownSignals {
    pub(super) fn install() -> Result<Self, String> {
        let sigterm = Arc::new(AtomicBool::new(false));
        let sigint = Arc::new(AtomicBool::new(false));
        let mut signals = Signals::new([SIGTERM, SIGINT])
            .map_err(|error| format!("register_shutdown_signals:{error}"))?;
        let handle = signals.handle();
        let (sender, receiver) = async_mpsc::unbounded();
        let term = Arc::clone(&sigterm);
        let interrupt = Arc::clone(&sigint);
        let signal_thread = thread::Builder::new()
            .name("gnos-ux-shutdown-signals".to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    let reason = if signal == SIGTERM {
                        term.store(true, Ordering::Release);
                        "SIGTERM"
                    } else {
                        interrupt.store(true, Ordering::Release);
                        "SIGINT"
                    };
                    if sender.unbounded_send(reason).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("shutdown_signal_thread:{error}"))?;
        Ok(Self {
            sigterm,
            sigint,
            receiver,
            handle,
            thread: Some(signal_thread),
        })
    }

    pub(super) fn requested_reason(&self) -> Option<&'static str> {
        if self.sigterm.load(Ordering::Acquire) {
            Some("SIGTERM")
        } else if self.sigint.load(Ordering::Acquire) {
            Some("SIGINT")
        } else {
            None
        }
    }
}

impl Drop for ShutdownSignals {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn emit_health_receipt(
    lifecycle: &SharedLifecycle,
    phase: RuntimeLifecyclePhase,
    generation: Option<u64>,
    route_count: usize,
    event_stream: EventStreamState,
    committed: bool,
    reason: impl Into<String>,
) -> Result<RuntimeHealthReceipt, String> {
    let reason = reason.into();
    let receipt = lifecycle
        .lock()
        .map_err(|_| "lifecycle_lock_poisoned".to_owned())?
        .transition(
            phase,
            generation,
            route_count,
            event_stream,
            committed,
            reason.clone(),
        )
        .map_err(|error| format!("lifecycle_transition:{error:?}"))?;

    crate::receipt!(
        "GNOS_UX_HEALTH_RECEIPT sequence={} phase={} health={} ready={} topology_generation={} route_count={} event_stream={} reason={}",
        receipt.sequence,
        receipt.phase.as_str(),
        receipt.health.as_str(),
        receipt.ready,
        receipt
            .topology_generation
            .map(|value| value.to_string())
            .unwrap_or_else(|| "NONE".to_owned()),
        receipt.route_count,
        receipt.event_stream.as_str(),
        receipt.reason.replace(char::is_whitespace, "_"),
    );
    Ok(receipt)
}

pub(super) fn emit_initial_health_receipt(lifecycle: &SharedLifecycle) -> Result<(), String> {
    let receipt = lifecycle
        .lock()
        .map_err(|_| "lifecycle_lock_poisoned".to_owned())?
        .initial_receipt("process_start")
        .map_err(|error| format!("lifecycle_initial:{error:?}"))?;
    println!(
        "GNOS_UX_HEALTH_RECEIPT sequence={} phase={} health={} ready={} topology_generation=NONE route_count=0 event_stream={} reason={}",
        receipt.sequence,
        receipt.phase.as_str(),
        receipt.health.as_str(),
        receipt.ready,
        receipt.event_stream.as_str(),
        receipt.reason,
    );
    Ok(())
}

#[derive(Default)]
pub(super) struct ProductPerfCounters {
    pub(super) started: Option<Instant>,
    pub(super) timer_ticks: AtomicU64,
    pub(super) motion_notifies: AtomicU64,
    pub(super) projection_notifies: AtomicU64,
    pub(super) interaction_notifies: AtomicU64,
    pub(super) pointer_changes: AtomicU64,
    pub(super) renders: AtomicU64,
    pub(super) solver_resolves: AtomicU64,
    pub(super) appstack_solver_resolves: AtomicU64,
    pub(super) largest_motion_tick_gap_us: AtomicU64,
    pub(super) bar_transition_ticks: AtomicU64,
    pub(super) bar_transition_gaps_us: Mutex<Vec<u64>>,
    pub(super) bar_transition_trace_emitted: AtomicBool,
    pub(super) paints: AtomicU64,
    /// LIVE-02: while a motion runs, when each frame was painted and how
    /// long each motion step took, for one `GNOS_UX_FRAME_STATS` receipt
    /// when it settles.
    pub(super) motion_active: AtomicBool,
    pub(super) motion_paints: Mutex<Vec<Instant>>,
    pub(super) motion_tick_work_us: Mutex<Vec<u64>>,
}

/// LIVE-02: the median, 95th percentile and maximum of `values`.
pub(super) fn spread(values: &mut [u64]) -> (u64, u64, u64) {
    if values.is_empty() {
        return (0, 0, 0);
    }
    values.sort_unstable();
    let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
    (at(0.5), at(0.95), values[values.len() - 1])
}
