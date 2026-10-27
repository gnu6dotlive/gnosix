use gnos_audio::PipeWireObserver;

fn main() {
    let result = PipeWireObserver::run(|snapshot| {
        let wire = serde_json::json!({
            "provider": snapshot.provider,
            "generation": snapshot.generation,
            "endpoints": snapshot.endpoints.values().collect::<Vec<_>>(),
            "defaults": snapshot.defaults,
        });
        println!("GNOS_AUDIO_SNAPSHOT {wire}");
    });
    if let Err(error) = result {
        eprintln!("GNOS_AUDIO_OBSERVER_FAIL error={error}");
        std::process::exit(1);
    }
}
