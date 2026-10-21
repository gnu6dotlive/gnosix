use gnos_audio::{
    AudioEndpointId, AudioEndpointKind, ExactAudioMutation, ExactAudioMutationOutcome,
    ExactAudioTarget, execute_exact_mutation, execute_select_default,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 6 {
        eprintln!(
            "usage: gnos-audio-mutate-exact <cookie> <serial> <output|input> <volume|mute|default> <value|node-name> [channels]"
        );
        std::process::exit(2);
    }
    let cookie: u32 = args[1].parse().expect("cookie");
    let serial: u64 = args[2].parse().expect("serial");
    let kind = match args[3].as_str() {
        "output" => AudioEndpointKind::Output,
        "input" => AudioEndpointKind::Input,
        other => panic!("unsupported kind {other}"),
    };
    if args[4] == "default" {
        let target = ExactAudioTarget {
            id: AudioEndpointId::new(cookie, serial),
            kind,
        };
        match execute_select_default(target, &args[5]) {
            Ok(outcome) => {
                println!("GNOS_AUDIO_MUTATION outcome={outcome:?}");
                if outcome != ExactAudioMutationOutcome::Completed {
                    std::process::exit(3);
                }
            }
            Err(error) => {
                eprintln!("GNOS_AUDIO_MUTATION_FAIL error={error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mutation = match args[4].as_str() {
        "volume" => ExactAudioMutation::SetUserVolume {
            level: args[5].parse().expect("volume"),
            channels: args
                .get(6)
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
        },
        "mute" => ExactAudioMutation::SetMute {
            muted: args[5].parse().expect("mute bool"),
        },
        other => panic!("unsupported mutation {other}"),
    };
    let target = ExactAudioTarget {
        id: AudioEndpointId::new(cookie, serial),
        kind,
    };
    match execute_exact_mutation(target, mutation) {
        Ok(outcome) => {
            println!("GNOS_AUDIO_MUTATION outcome={outcome:?}");
            if outcome != ExactAudioMutationOutcome::Completed {
                std::process::exit(3);
            }
        }
        Err(error) => {
            eprintln!("GNOS_AUDIO_MUTATION_FAIL error={error}");
            std::process::exit(1);
        }
    }
}
