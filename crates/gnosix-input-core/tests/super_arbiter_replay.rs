use gnosix_input_core::arbiter::{ModifierTap, StateEvent, SuperArbiter};
use serde::Deserialize;

#[derive(Deserialize)]
struct Trace {
    name: String,
    events: Vec<StateEvent>,
    expected_taps: Vec<ModifierTap>,
}

#[test]
fn normalized_state_event_traces_replay_through_the_qualified_arbiter() {
    let traces: Vec<Trace> =
        serde_json::from_str(include_str!("../fixtures/super-arbiter-traces.json"))
            .expect("replay fixture corpus parses");

    assert_eq!(traces.len(), 10, "the required gesture corpus is present");

    for trace in traces {
        let mut arbiter = SuperArbiter::new();
        let actual_taps: Vec<_> = trace
            .events
            .into_iter()
            .filter_map(|event| arbiter.advance(event))
            .collect();

        assert_eq!(actual_taps, trace.expected_taps, "trace {}", trace.name);
    }
}
