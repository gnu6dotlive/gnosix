//! Session-scoped Hyprland topology observation and reconciliation.
//!
//! Events are wake-up hints only. Fresh complete snapshots remain runtime truth.

use crate::ObservedRuntimeOutput;
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{self, BufRead, BufReader, ErrorKind},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, mpsc::Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

const RECONNECT_BACKOFF_MIN: Duration = Duration::from_millis(100);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyWakeHint {
    Startup,
    SafetyResync,
    StreamConnected,
    StreamDisconnected,
    MonitorAdded(String),
    MonitorRemoved(String),
    ConfigReloaded,
    SemanticChanged(String),
}

pub fn parse_socket2_topology_hint(line: &str) -> Option<TopologyWakeHint> {
    let (event, data) = line.trim_end_matches(['\r', '\n']).split_once(">>")?;
    match event {
        "monitoradded" => non_empty(data).map(TopologyWakeHint::MonitorAdded),
        "monitorremoved" => non_empty(data).map(TopologyWakeHint::MonitorRemoved),
        "monitoraddedv2" => v2_monitor_name(data).map(TopologyWakeHint::MonitorAdded),
        "monitorremovedv2" => v2_monitor_name(data).map(TopologyWakeHint::MonitorRemoved),
        "configreloaded" => Some(TopologyWakeHint::ConfigReloaded),
        "workspace" | "workspacev2" | "focusedmon" | "focusedmonv2" | "activewindow"
        | "activewindowv2" | "fullscreen" | "createworkspace" | "createworkspacev2"
        | "destroyworkspace" | "destroyworkspacev2" | "moveworkspace" | "moveworkspacev2"
        | "renameworkspace" | "activespecial" | "activespecialv2" | "openwindow"
        | "closewindow" | "kill" | "movewindow" | "movewindowv2" | "openlayer" | "closelayer"
        | "changefloatingmode" | "windowtitle" | "windowtitlev2" | "pin" | "minimized" => {
            Some(TopologyWakeHint::SemanticChanged(event.to_owned()))
        }
        _ => None,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn v2_monitor_name(data: &str) -> Option<String> {
    non_empty(data.split(',').nth(1)?)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopologyWakeBatch {
    pub hint_count: usize,
    pub added_names: BTreeSet<String>,
    pub removed_names: BTreeSet<String>,
    pub force_reattach_names: BTreeSet<String>,
    pub semantic_events: BTreeSet<String>,
    pub stream_connected: bool,
    pub stream_disconnected: bool,
    pub last_stream_state: Option<bool>,
    pub safety_resync: bool,
    pub startup: bool,
    pub config_reloaded: bool,
}

impl TopologyWakeBatch {
    pub fn absorb(&mut self, hint: TopologyWakeHint) {
        self.hint_count += 1;
        match hint {
            TopologyWakeHint::Startup => self.startup = true,
            TopologyWakeHint::SafetyResync => self.safety_resync = true,
            TopologyWakeHint::StreamConnected => {
                self.stream_connected = true;
                self.last_stream_state = Some(true);
            }
            TopologyWakeHint::StreamDisconnected => {
                self.stream_disconnected = true;
                self.last_stream_state = Some(false);
            }
            TopologyWakeHint::MonitorAdded(name) => {
                if self.removed_names.contains(&name) {
                    self.force_reattach_names.insert(name.clone());
                }
                self.added_names.insert(name);
            }
            TopologyWakeHint::MonitorRemoved(name) => {
                if self.added_names.contains(&name) {
                    self.force_reattach_names.insert(name.clone());
                }
                self.removed_names.insert(name);
            }
            TopologyWakeHint::ConfigReloaded => self.config_reloaded = true,
            TopologyWakeHint::SemanticChanged(event) => {
                self.semantic_events.insert(event);
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyAction {
    Add {
        current: ObservedRuntimeOutput,
    },
    Remove {
        previous: ObservedRuntimeOutput,
    },
    Preserve {
        current: ObservedRuntimeOutput,
    },
    Reconfigure {
        previous: ObservedRuntimeOutput,
        current: ObservedRuntimeOutput,
    },
    Reattach {
        previous: ObservedRuntimeOutput,
        current: ObservedRuntimeOutput,
    },
}

impl TopologyAction {
    pub fn output_name(&self) -> &str {
        match self {
            Self::Add { current }
            | Self::Preserve { current }
            | Self::Reconfigure { current, .. }
            | Self::Reattach { current, .. } => &current.name,
            Self::Remove { previous } => &previous.name,
        }
    }

    pub fn mutates_routes(&self) -> bool {
        !matches!(self, Self::Preserve { .. } | Self::Reconfigure { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTopologyPlan {
    pub base_generation: u64,
    pub target_generation: u64,
    pub actions: Vec<TopologyAction>,
    pub target_outputs: Vec<ObservedRuntimeOutput>,
    session_signature: String,
}

impl SessionTopologyPlan {
    pub fn changed(&self) -> bool {
        self.target_generation != self.base_generation
    }

    pub fn route_mutation_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| action.mutates_routes())
            .count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyReconcileError {
    EmptySession,
    StaleSession,
    DuplicateOutput,
    DuplicateRuntimeMonitorId,
    GenerationExhausted,
    StalePlan,
}

#[derive(Debug, Default)]
pub struct SessionTopologyReconciler {
    generation: u64,
    session_signature: Option<String>,
    current: BTreeMap<String, ObservedRuntimeOutput>,
}

impl SessionTopologyReconciler {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn current_outputs(&self) -> Vec<ObservedRuntimeOutput> {
        self.current.values().cloned().collect()
    }

    pub fn plan(
        &self,
        expected_session: &str,
        observed: Vec<ObservedRuntimeOutput>,
        force_reattach_names: &BTreeSet<String>,
        rebuild_all: bool,
    ) -> Result<SessionTopologyPlan, TopologyReconcileError> {
        if expected_session.trim().is_empty() {
            return Err(TopologyReconcileError::EmptySession);
        }
        if self
            .session_signature
            .as_deref()
            .is_some_and(|session| session != expected_session)
        {
            return Err(TopologyReconcileError::StaleSession);
        }

        let target = normalize_outputs(expected_session, observed)?;
        let mut names = self.current.keys().cloned().collect::<BTreeSet<_>>();
        names.extend(target.keys().cloned());

        let mut actions = Vec::with_capacity(names.len());
        for name in names {
            match (self.current.get(&name), target.get(&name)) {
                (None, Some(current)) => actions.push(TopologyAction::Add {
                    current: current.clone(),
                }),
                (Some(previous), None) => actions.push(TopologyAction::Remove {
                    previous: previous.clone(),
                }),
                (Some(previous), Some(current)) => {
                    if rebuild_all
                        || force_reattach_names.contains(&name)
                        || previous.runtime_monitor_id != current.runtime_monitor_id
                    {
                        actions.push(TopologyAction::Reattach {
                            previous: previous.clone(),
                            current: current.clone(),
                        });
                    } else if previous != current {
                        actions.push(TopologyAction::Reconfigure {
                            previous: previous.clone(),
                            current: current.clone(),
                        });
                    } else {
                        actions.push(TopologyAction::Preserve {
                            current: current.clone(),
                        });
                    }
                }
                (None, None) => unreachable!("union key must exist in one snapshot"),
            }
        }

        let changed = actions
            .iter()
            .any(|action| !matches!(action, TopologyAction::Preserve { .. }));
        let target_generation = if changed {
            self.generation
                .checked_add(1)
                .ok_or(TopologyReconcileError::GenerationExhausted)?
        } else {
            self.generation
        };

        Ok(SessionTopologyPlan {
            base_generation: self.generation,
            target_generation,
            actions,
            target_outputs: target.values().cloned().collect(),
            session_signature: expected_session.to_owned(),
        })
    }

    pub fn commit(&mut self, plan: &SessionTopologyPlan) -> Result<(), TopologyReconcileError> {
        if plan.base_generation != self.generation {
            return Err(TopologyReconcileError::StalePlan);
        }
        if self
            .session_signature
            .as_deref()
            .is_some_and(|session| session != plan.session_signature)
        {
            return Err(TopologyReconcileError::StaleSession);
        }

        self.current = plan
            .target_outputs
            .iter()
            .cloned()
            .map(|output| (output.name.clone(), output))
            .collect();
        self.generation = plan.target_generation;
        self.session_signature = Some(plan.session_signature.clone());
        Ok(())
    }
}

fn normalize_outputs(
    expected_session: &str,
    observed: Vec<ObservedRuntimeOutput>,
) -> Result<BTreeMap<String, ObservedRuntimeOutput>, TopologyReconcileError> {
    let mut normalized = BTreeMap::new();
    let mut runtime_monitor_ids = BTreeSet::new();
    for output in observed {
        if output.session_signature != expected_session {
            return Err(TopologyReconcileError::StaleSession);
        }
        if !runtime_monitor_ids.insert(output.runtime_monitor_id) {
            return Err(TopologyReconcileError::DuplicateRuntimeMonitorId);
        }
        if normalized.insert(output.name.clone(), output).is_some() {
            return Err(TopologyReconcileError::DuplicateOutput);
        }
    }
    Ok(normalized)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyEventSourceError {
    EmptyInstanceSignature,
    MissingRuntimeDir,
    SpawnFailed,
}

// One latched cancellation owns both the active socket wake and reconnect wait.
// Registering a socket and cancellation share the lock: cancellation cannot be
// lost between connect and the first blocked read, or affect a reused fd.
#[derive(Default)]
struct EventCancellation {
    state: Mutex<EventCancellationState>,
    wake: Condvar,
}

#[derive(Default)]
struct EventCancellationState {
    cancelled: bool,
    stream: Option<UnixStream>,
}

impl EventCancellation {
    fn cancelled(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled
    }

    fn register(&self, stream: &UnixStream) -> io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled {
            return Err(io::Error::new(
                ErrorKind::Interrupted,
                "event subscription cancelled",
            ));
        }
        state.stream = Some(stream.try_clone()?);
        Ok(())
    }

    fn clear_stream(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stream = None;
    }

    fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.cancelled = true;
        if let Some(stream) = &state.stream {
            // Local read shutdown wakes the owned blocked reader without
            // waiting for peer traffic; it is not a compositor command.
            let _ = stream.shutdown(Shutdown::Read);
        }
        self.wake.notify_all();
    }

    fn wait_backoff(&self, duration: Duration) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = self
            .wake
            .wait_timeout_while(state, duration, |s| !s.cancelled)
            .unwrap_or_else(|e| e.into_inner());
    }
}

pub struct TopologyEventSubscription {
    stop: Arc<EventCancellation>,
    join: Option<JoinHandle<()>>,
}

impl TopologyEventSubscription {
    pub fn spawn(
        instance_signature: String,
        sender: Sender<TopologyWakeHint>,
    ) -> Result<Self, TopologyEventSourceError> {
        Self::spawn_with_sink(instance_signature, move |hint| sender.send(hint).is_ok())
    }

    /// Deliver hints to a consumer's native wake mechanism without a polling bridge.
    /// Returning false ends the subscription when that consumer is gone.
    pub fn spawn_with_sink(
        instance_signature: String,
        sink: impl Fn(TopologyWakeHint) -> bool + Send + 'static,
    ) -> Result<Self, TopologyEventSourceError> {
        if instance_signature.trim().is_empty() {
            return Err(TopologyEventSourceError::EmptyInstanceSignature);
        }
        let runtime_dir =
            env::var_os("XDG_RUNTIME_DIR").ok_or(TopologyEventSourceError::MissingRuntimeDir)?;
        let socket_path = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(instance_signature)
            .join(".socket2.sock");

        Self::spawn_at(socket_path, sink)
    }

    fn spawn_at(
        socket_path: PathBuf,
        sink: impl Fn(TopologyWakeHint) -> bool + Send + 'static,
    ) -> Result<Self, TopologyEventSourceError> {
        let stop = Arc::new(EventCancellation::default());
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("gnosix-hyprland-topology-events".to_owned())
            .spawn(move || {
                run_event_loop(
                    || {
                        let stream =
                            crate::observation_transport::connect_nonblocking(&socket_path)?;
                        thread_stop.register(&stream)?;
                        Ok(BufReader::new(stream))
                    },
                    &sink,
                    &thread_stop,
                    None,
                    RECONNECT_BACKOFF_MIN,
                );
            })
            .map_err(|_| TopologyEventSourceError::SpawnFailed)?;

        Ok(Self {
            stop,
            join: Some(join),
        })
    }
}

impl Drop for TopologyEventSubscription {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_event_loop<C, R>(
    mut connect: C,
    sink: &impl Fn(TopologyWakeHint) -> bool,
    stop: &EventCancellation,
    max_connects: Option<usize>,
    initial_backoff: Duration,
) where
    C: FnMut() -> io::Result<R>,
    R: BufRead,
{
    let mut connect_count = 0usize;
    let mut connection_state = None;
    let mut backoff = initial_backoff;

    while !stop.cancelled() {
        if max_connects.is_some_and(|max| connect_count >= max) {
            break;
        }
        connect_count += 1;

        match connect() {
            Ok(mut reader) => {
                if !emit_connection_state(sink, &mut connection_state, true) {
                    return;
                }
                backoff = initial_backoff;

                loop {
                    if stop.cancelled() {
                        return;
                    }
                    let mut line = String::new();
                    let result = reader.read_line(&mut line);
                    if stop.cancelled() {
                        return;
                    }
                    match result {
                        Ok(0) => {
                            if !emit_connection_state(sink, &mut connection_state, false) {
                                return;
                            }
                            break;
                        }
                        Ok(_) => {
                            if let Some(hint) = parse_socket2_topology_hint(&line)
                                && !sink(hint)
                            {
                                return;
                            }
                        }
                        Err(error) if error.kind() == ErrorKind::Interrupted => {}
                        Err(_) => {
                            if !emit_connection_state(sink, &mut connection_state, false) {
                                return;
                            }
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                if !emit_connection_state(sink, &mut connection_state, false) {
                    return;
                }
            }
        }

        stop.clear_stream();
        stop.wait_backoff(backoff);
        backoff = backoff.saturating_mul(2).min(RECONNECT_BACKOFF_MAX);
    }
}

fn emit_connection_state(
    sink: &impl Fn(TopologyWakeHint) -> bool,
    state: &mut Option<bool>,
    connected: bool,
) -> bool {
    if *state == Some(connected) {
        return true;
    }
    *state = Some(connected);
    let hint = if connected {
        TopologyWakeHint::StreamConnected
    } else {
        TopologyWakeHint::StreamDisconnected
    };
    sink(hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{BTreeSet, VecDeque},
        io::Cursor,
        sync::{Arc, mpsc},
    };

    fn output(session: &str, id: i64, name: &str, x: i64, width: i64) -> ObservedRuntimeOutput {
        ObservedRuntimeOutput {
            session_signature: session.to_owned(),
            runtime_monitor_id: id,
            name: name.to_owned(),
            x,
            y: 0,
            width,
            height: 1080,
            refresh_millihz: 60_000,
            scale_milli: 1000,
            transform: 0,
        }
    }

    #[test]
    fn parses_exact_locally_observed_monitor_event_forms() {
        assert_eq!(
            parse_socket2_topology_hint("monitoradded>>GNOSIX-SDK8-HEADLESS\n"),
            Some(TopologyWakeHint::MonitorAdded(
                "GNOSIX-SDK8-HEADLESS".to_owned()
            ))
        );
        assert_eq!(
            parse_socket2_topology_hint("monitoraddedv2>>2,GNOSIX-SDK8-HEADLESS,\n"),
            Some(TopologyWakeHint::MonitorAdded(
                "GNOSIX-SDK8-HEADLESS".to_owned()
            ))
        );
        assert_eq!(
            parse_socket2_topology_hint("monitorremoved>>GNOSIX-SDK8-HEADLESS\n"),
            Some(TopologyWakeHint::MonitorRemoved(
                "GNOSIX-SDK8-HEADLESS".to_owned()
            ))
        );
        assert_eq!(
            parse_socket2_topology_hint("monitorremovedv2>>2,GNOSIX-SDK8-HEADLESS,\n"),
            Some(TopologyWakeHint::MonitorRemoved(
                "GNOSIX-SDK8-HEADLESS".to_owned()
            ))
        );
        assert_eq!(
            parse_socket2_topology_hint("workspacev2>>22,22\n"),
            Some(TopologyWakeHint::SemanticChanged("workspacev2".to_owned()))
        );
        assert_eq!(
            parse_socket2_topology_hint("activewindowv2>>0xabc\n"),
            Some(TopologyWakeHint::SemanticChanged(
                "activewindowv2".to_owned()
            ))
        );
        assert_eq!(
            parse_socket2_topology_hint("openlayer>>gnos.ux:scene\n"),
            Some(TopologyWakeHint::SemanticChanged("openlayer".to_owned()))
        );
    }

    #[test]
    fn reorder_only_is_noop_and_does_not_advance_generation() {
        let mut reconciler = SessionTopologyReconciler::default();
        let force = BTreeSet::new();
        let initial = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 1, "DP-2", 1920, 1920),
                    output("session-a", 0, "HDMI-A-1", 0, 1920),
                ],
                &force,
                false,
            )
            .expect("initial plan");
        reconciler.commit(&initial).expect("initial commit");
        assert_eq!(reconciler.generation(), 1);

        let reordered = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 0, "HDMI-A-1", 0, 1920),
                    output("session-a", 1, "DP-2", 1920, 1920),
                ],
                &force,
                false,
            )
            .expect("reordered plan");

        assert!(!reordered.changed());
        assert_eq!(reordered.target_generation, 1);
        assert!(
            reordered
                .actions
                .iter()
                .all(|action| matches!(action, TopologyAction::Preserve { .. }))
        );
    }

    #[test]
    fn add_remove_preserves_unaffected_output() {
        let mut reconciler = SessionTopologyReconciler::default();
        let force = BTreeSet::new();
        let initial = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 0, "HDMI-A-1", 0, 1920),
                    output("session-a", 1, "DP-2", 1920, 1920),
                ],
                &force,
                false,
            )
            .unwrap();
        reconciler.commit(&initial).unwrap();

        let changed = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 1, "DP-2", 1920, 1920),
                    output("session-a", 2, "HEADLESS-1", 3840, 1920),
                ],
                &force,
                false,
            )
            .unwrap();

        assert!(changed.actions.iter().any(|action| matches!(
            action,
            TopologyAction::Remove { previous } if previous.name == "HDMI-A-1"
        )));
        assert!(changed.actions.iter().any(|action| matches!(
            action,
            TopologyAction::Preserve { current } if current.name == "DP-2"
        )));
        assert!(changed.actions.iter().any(|action| matches!(
            action,
            TopologyAction::Add { current } if current.name == "HEADLESS-1"
        )));
    }

    #[test]
    fn geometry_change_reconfigures_without_route_replacement() {
        let mut reconciler = SessionTopologyReconciler::default();
        let force = BTreeSet::new();
        let initial = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 0, 1920)],
                &force,
                false,
            )
            .unwrap();
        reconciler.commit(&initial).unwrap();

        let changed = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 200, 2560)],
                &force,
                false,
            )
            .unwrap();

        assert!(matches!(
            changed.actions.as_slice(),
            [TopologyAction::Reconfigure { .. }]
        ));
        assert_eq!(changed.route_mutation_count(), 0);
        assert_eq!(changed.target_generation, 2);
    }

    #[test]
    fn forced_reattach_replaces_same_named_attachment() {
        let mut reconciler = SessionTopologyReconciler::default();
        let initial = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 0, 1920)],
                &BTreeSet::new(),
                false,
            )
            .unwrap();
        reconciler.commit(&initial).unwrap();

        let force = BTreeSet::from(["DP-2".to_owned()]);
        let plan = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 0, 1920)],
                &force,
                false,
            )
            .unwrap();
        assert!(matches!(
            plan.actions.as_slice(),
            [TopologyAction::Reattach { .. }]
        ));
    }

    #[test]
    fn plan_does_not_mutate_state_until_commit() {
        let reconciler = SessionTopologyReconciler::default();
        let plan = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 0, 1920)],
                &BTreeSet::new(),
                false,
            )
            .unwrap();

        assert_eq!(reconciler.generation(), 0);
        assert!(reconciler.current_outputs().is_empty());
        assert_eq!(plan.target_generation, 1);
    }

    #[test]
    fn stale_session_and_duplicate_output_are_rejected() {
        let mut reconciler = SessionTopologyReconciler::default();
        let initial = reconciler
            .plan(
                "session-a",
                vec![output("session-a", 1, "DP-2", 0, 1920)],
                &BTreeSet::new(),
                false,
            )
            .unwrap();
        reconciler.commit(&initial).unwrap();

        assert_eq!(
            reconciler.plan(
                "session-b",
                vec![output("session-b", 1, "DP-2", 0, 1920)],
                &BTreeSet::new(),
                false,
            ),
            Err(TopologyReconcileError::StaleSession)
        );

        let duplicate = SessionTopologyReconciler::default().plan(
            "session-a",
            vec![
                output("session-a", 1, "DP-2", 0, 1920),
                output("session-a", 2, "DP-2", 0, 1920),
            ],
            &BTreeSet::new(),
            false,
        );
        assert_eq!(duplicate, Err(TopologyReconcileError::DuplicateOutput));

        let duplicate_runtime_id = SessionTopologyReconciler::default().plan(
            "session-a",
            vec![
                output("session-a", 1, "DP-2", 0, 1920),
                output("session-a", 1, "HDMI-A-1", 1920, 1920),
            ],
            &BTreeSet::new(),
            false,
        );
        assert_eq!(
            duplicate_runtime_id,
            Err(TopologyReconcileError::DuplicateRuntimeMonitorId)
        );
    }

    #[test]
    fn refresh_change_advances_generation_without_route_replacement() {
        let mut reconciler = SessionTopologyReconciler::default();
        let force = BTreeSet::new();
        let initial_output = output("session-a", 1, "DP-2", 0, 1920);
        let initial = reconciler
            .plan("session-a", vec![initial_output.clone()], &force, false)
            .unwrap();
        reconciler.commit(&initial).unwrap();

        let mut changed_output = initial_output;
        changed_output.refresh_millihz = 144_000;
        let changed = reconciler
            .plan("session-a", vec![changed_output], &force, false)
            .unwrap();

        assert!(matches!(
            changed.actions.as_slice(),
            [TopologyAction::Reconfigure { .. }]
        ));
        assert_eq!(changed.route_mutation_count(), 0);
        assert_eq!(changed.target_generation, 2);
    }

    #[test]
    fn remove_then_add_same_name_in_one_batch_forces_reattach() {
        let mut batch = TopologyWakeBatch::default();
        batch.absorb(TopologyWakeHint::MonitorRemoved("DP-2".to_owned()));
        batch.absorb(TopologyWakeHint::MonitorAdded("DP-2".to_owned()));

        assert!(batch.removed_names.contains("DP-2"));
        assert!(batch.added_names.contains("DP-2"));
        assert!(batch.force_reattach_names.contains("DP-2"));
    }

    #[test]
    fn continuity_gap_rebuild_reattaches_entire_snapshot() {
        let mut reconciler = SessionTopologyReconciler::default();
        let initial = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 1, "DP-2", 0, 1920),
                    output("session-a", 2, "HDMI-A-1", 1920, 2560),
                ],
                &BTreeSet::new(),
                false,
            )
            .unwrap();
        reconciler.commit(&initial).unwrap();

        let rebuild = reconciler
            .plan(
                "session-a",
                vec![
                    output("session-a", 1, "DP-2", 0, 1920),
                    output("session-a", 2, "HDMI-A-1", 1920, 2560),
                ],
                &BTreeSet::new(),
                true,
            )
            .unwrap();

        assert_eq!(rebuild.target_generation, 2);
        assert_eq!(rebuild.route_mutation_count(), 2);
        assert!(
            rebuild
                .actions
                .iter()
                .all(|action| { matches!(action, TopologyAction::Reattach { .. }) })
        );
    }

    #[test]
    fn blocked_event_subscription_cancels_without_waiting_for_timeout() {
        use std::{os::unix::net::UnixListener, time::Instant};
        let path = std::env::temp_dir().join(format!("gp3-{}", uuid::Uuid::new_v4().simple()));
        let listener = UnixListener::bind(&path).unwrap();
        let (sender, receiver) = mpsc::channel();
        let subscription = TopologyEventSubscription::spawn_at(path.clone(), move |hint| {
            sender.send(hint).is_ok()
        })
        .unwrap();
        let (_peer, _) = listener.accept().unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::StreamConnected
        );
        thread::sleep(Duration::from_millis(25));
        let started = Instant::now();
        drop(subscription);
        std::fs::remove_file(path).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(125),
            "blocked cancellation took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn cancellation_is_latched_and_repeated_cancel_is_safe() {
        let cancellation = EventCancellation::default();
        let (stream, _peer) = UnixStream::pair().unwrap();
        cancellation.cancel();
        cancellation.cancel();
        assert!(cancellation.cancelled());
        assert_eq!(
            cancellation.register(&stream).unwrap_err().kind(),
            ErrorKind::Interrupted
        );
        let started = std::time::Instant::now();
        cancellation.wait_backoff(Duration::from_secs(2));
        assert!(started.elapsed() < Duration::from_millis(125));
    }

    #[test]
    fn explicit_cancel_interrupts_reconnect_backoff() {
        let cancellation = Arc::new(EventCancellation::default());
        let worker_cancel = Arc::clone(&cancellation);
        let (started, received) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_event_loop(
                || -> io::Result<Cursor<Vec<u8>>> { Err(io::Error::from(ErrorKind::NotFound)) },
                &|hint| started.send(hint).is_ok(),
                &worker_cancel,
                None,
                Duration::from_secs(2),
            );
        });
        assert_eq!(
            received.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::StreamDisconnected
        );
        let now = std::time::Instant::now();
        cancellation.cancel();
        worker.join().unwrap();
        assert!(now.elapsed() < Duration::from_millis(125));
    }

    #[test]
    fn event_delivery_and_reconnection_need_no_read_timer() {
        use std::{io::Write, os::unix::net::UnixListener};
        let path = std::env::temp_dir().join(format!("gp3-{}", uuid::Uuid::new_v4().simple()));
        let listener = UnixListener::bind(&path).unwrap();
        let (sender, receiver) = mpsc::channel();
        let subscription = TopologyEventSubscription::spawn_at(path.clone(), move |hint| {
            sender.send(hint).is_ok()
        })
        .unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::StreamConnected
        );
        assert_eq!(
            subscription
                .stop
                .state
                .lock()
                .unwrap()
                .stream
                .as_ref()
                .unwrap()
                .read_timeout()
                .unwrap(),
            None
        );
        peer.write_all(b"monitoradded>>HEADLESS-A\n").unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::MonitorAdded("HEADLESS-A".into())
        );
        drop(peer);
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::StreamDisconnected
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::StreamConnected
        );
        let (mut second_peer, _) = listener.accept().unwrap();
        second_peer
            .write_all(b"monitorremoved>>HEADLESS-A\n")
            .unwrap();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            TopologyWakeHint::MonitorRemoved("HEADLESS-A".into())
        );
        // A partial line must also be cancellable without peer completion.
        second_peer.write_all(b"monitoradded>>incomplete").unwrap();
        subscription.stop.cancel();
        subscription.stop.cancel();
        drop(subscription);
        assert!(
            receiver.try_recv().is_err(),
            "cancellation is not a compositor disconnect event"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn event_loop_stops_when_native_wake_consumer_is_gone() {
        let connects = std::cell::Cell::new(0);
        run_event_loop(
            || {
                connects.set(connects.get() + 1);
                Ok(Cursor::new(b"monitoradded>>HEADLESS-A\n".to_vec()))
            },
            &|_| false,
            &EventCancellation::default(),
            Some(2),
            Duration::ZERO,
        );
        assert_eq!(connects.get(), 1);
    }

    #[test]
    fn scripted_event_loop_reconnects_and_continues_delivery() {
        let readers = Arc::new(std::sync::Mutex::new(VecDeque::from([
            Cursor::new(b"monitoradded>>HEADLESS-A\n".to_vec()),
            Cursor::new(b"monitorremoved>>HEADLESS-A\n".to_vec()),
        ])));
        let readers_for_connect = Arc::clone(&readers);
        let (sender, receiver) = mpsc::channel();
        let stop = EventCancellation::default();

        run_event_loop(
            move || {
                readers_for_connect
                    .lock()
                    .expect("reader queue")
                    .pop_front()
                    .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no scripted reader"))
            },
            &|hint| sender.send(hint).is_ok(),
            &stop,
            Some(2),
            Duration::ZERO,
        );

        let received = receiver.try_iter().collect::<Vec<_>>();
        assert_eq!(
            received,
            vec![
                TopologyWakeHint::StreamConnected,
                TopologyWakeHint::MonitorAdded("HEADLESS-A".to_owned()),
                TopologyWakeHint::StreamDisconnected,
                TopologyWakeHint::StreamConnected,
                TopologyWakeHint::MonitorRemoved("HEADLESS-A".to_owned()),
                TopologyWakeHint::StreamDisconnected,
            ]
        );
    }
}
