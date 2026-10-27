use gnos_audio::PipeWireObserver;
use gnos_ux_gpui::{
    product_audio::ProductAudioEndpointState,
    product_audio_action::{
        AudioIntent, AudioIntentOutcome, ExactAudioEndpointRef, ProductVolume, dispatch_intent,
    },
    product_audio_osd::{AudioOsdPlacement, AudioOsdPresentationState},
    product_bar::BarAudioStatus,
    product_runtime::ProductRuntime,
};
use std::time::{Duration, Instant};

/// The single output the qualification resolves OSD placement against.
const QUALIFY_OUTPUT: &str = "DP-1";

#[derive(Clone, Copy, Debug, PartialEq)]
enum ActionMode {
    Observe,
    SetVolume(f32),
    ToggleOutputMute,
    StaleToggle,
}

fn action_mode(args: &[String]) -> Result<ActionMode, String> {
    match args {
        [] => Ok(ActionMode::Observe),
        [flag, value] if flag == "--set-volume" => {
            let value: f32 = value.parse().map_err(|_| "invalid volume".to_owned())?;
            ProductVolume::new(value).map_err(|_| "volume must be within 0..=1".to_owned())?;
            Ok(ActionMode::SetVolume(value))
        }
        [flag] if flag == "--toggle-output-mute" => Ok(ActionMode::ToggleOutputMute),
        [flag] if flag == "--stale-toggle" => Ok(ActionMode::StaleToggle),
        _ => Err(
            "usage: gnos-ux-audio-qualify [--set-volume 0..1 | --toggle-output-mute | --stale-toggle]"
                .to_owned(),
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match action_mode(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let mut runtime = ProductRuntime::default();
    let mut action_started = false;
    // AUDIO-01 Phase 5: the production OSD state machine, driven by the same
    // accepted observation that ProductRuntime accepts. One output is resolved
    // so placement is exercised; the gate asserts exactly one surface.
    let mut osd = AudioOsdPresentationState::new(
        AudioOsdPlacement::new(vec![QUALIFY_OUTPUT.to_owned()]),
        gnos_ux_gpui::product_audio_osd::AUDIO_OSD_DWELL,
        Duration::ZERO,
        Duration::ZERO,
    );
    if let Err(error) = PipeWireObserver::run(move |snapshot| {
        let now = Instant::now();
        let osd_event = osd.observe(&snapshot, now);
        // Every presentation decision is journalled, including the ones that
        // correctly produce nothing, so the gate can distinguish "no event" from
        // "event the gate missed".
        println!(
            "GNOS_UX_AUDIO_OSD event={} phase={} visible={} owner={} observed_level={} observed_muted={} generation={}",
            match &osd_event {
                Some(event) => event.cause.as_str(),
                None => "none",
            },
            format!("{:?}", osd.phase()).to_lowercase(),
            osd.visible(),
            osd.owner().unwrap_or("UNRESOLVED"),
            osd_event
                .as_ref()
                .and_then(|event| event.truth.level)
                .map_or_else(|| "none".to_owned(), |level| format!("{level:.3}")),
            osd_event.as_ref().is_some_and(|event| event.truth.muted),
            osd_event.as_ref().map_or(0, |event| event.truth.generation),
        );
        if !runtime.set_audio(snapshot) {
            return;
        }
        let presentation = runtime.audio_presentation();
        let status = BarAudioStatus::derive(&presentation);
        let muted = match &presentation.output {
            ProductAudioEndpointState::Ready(output) => Some(output.muted),
            _ => None,
        };
        println!(
            "GNOS_UX_AUDIO_PRODUCT generation={} available={} text={} glyph={} muted={:?}",
            runtime.audio().generation,
            status.available,
            status.text,
            status.glyph,
            muted,
        );

        if mode != ActionMode::Observe
            && !action_started
            && let ProductAudioEndpointState::Ready(output) = &presentation.output
        {
            let snapshot = runtime.audio().clone();
            let current = output.id;
            let intent = match mode {
                ActionMode::Observe => unreachable!(),
                ActionMode::SetVolume(level) => AudioIntent::SetOutputVolume {
                    target: ExactAudioEndpointRef::output(current),
                    level: ProductVolume::new(level).expect("validated action mode"),
                },
                ActionMode::ToggleOutputMute => AudioIntent::ToggleOutputMute {
                    target: ExactAudioEndpointRef::output(current),
                },
                ActionMode::StaleToggle => AudioIntent::ToggleOutputMute {
                    target: ExactAudioEndpointRef::output(gnos_audio::AudioEndpointId::new(
                        current.server_cookie,
                        u64::MAX,
                    )),
                },
            };
            action_started = true;
            std::thread::spawn(move || {
                let outcome = dispatch_intent(&snapshot, &intent);
                println!("GNOS_UX_AUDIO_ACTION mode={mode:?} outcome={outcome:?}");
                let accepted = match mode {
                    ActionMode::SetVolume(_) | ActionMode::ToggleOutputMute => {
                        outcome == AudioIntentOutcome::Completed
                    }
                    ActionMode::StaleToggle => {
                        matches!(outcome, AudioIntentOutcome::RefusedStale(_))
                    }
                    ActionMode::Observe => false,
                };
                std::process::exit(if accepted { 0 } else { 4 });
            });
        }
    }) {
        eprintln!("GNOS_UX_AUDIO_PRODUCT_FAIL error={error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualification_action_modes_are_explicit() {
        assert_eq!(action_mode(&[]).unwrap(), ActionMode::Observe);
        assert_eq!(
            action_mode(&["--set-volume".into(), "0.87".into()]).unwrap(),
            ActionMode::SetVolume(0.87)
        );
        assert_eq!(
            action_mode(&["--toggle-output-mute".into()]).unwrap(),
            ActionMode::ToggleOutputMute
        );
        assert_eq!(
            action_mode(&["--stale-toggle".into()]).unwrap(),
            ActionMode::StaleToggle
        );
        assert!(action_mode(&["--set-volume".into(), "1.01".into()]).is_err());
    }
}
