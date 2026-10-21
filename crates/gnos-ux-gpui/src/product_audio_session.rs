use futures::{StreamExt, channel::mpsc};
use gnos_audio::{AudioDefaultRoles, AudioProviderState, AudioSnapshot, PipeWireObserver};
use gpui::{AppContext, Context, Entity};

pub struct AudioSession {
    snapshot: AudioSnapshot,
    started: bool,
}

#[derive(Clone)]
struct AudioGlobal(Entity<AudioSession>);
impl gpui::Global for AudioGlobal {}

fn unavailable_snapshot() -> AudioSnapshot {
    AudioSnapshot {
        provider: AudioProviderState::Unavailable,
        generation: 0,
        endpoints: Default::default(),
        defaults: AudioDefaultRoles::default(),
    }
}

impl AudioSession {
    fn new() -> Self {
        Self {
            snapshot: unavailable_snapshot(),
            started: false,
        }
    }

    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<AudioGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self::new());
        cx.set_global(AudioGlobal(entity.clone()));
        entity
    }

    pub fn snapshot(&self) -> &AudioSnapshot {
        &self.snapshot
    }

    pub const fn started(&self) -> bool {
        self.started
    }

    fn set_snapshot(&mut self, snapshot: AudioSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

pub fn ensure_observation(session: &Entity<AudioSession>, cx: &mut gpui::App) {
    let session = session.clone();
    session.update(cx, |this, cx| {
        if this.started {
            return;
        }
        this.started = true;
        let entity = cx.entity();
        let (sender, mut receiver) = mpsc::unbounded::<AudioSnapshot>();
        let thread_sender = sender.clone();
        let spawn = std::thread::Builder::new()
            .name("gnos-audio-observer".into())
            .spawn(move || {
                // Embedded on a worker thread: the scene owns process signals.
                let result = PipeWireObserver::run_embedded(move |snapshot| {
                    let _ = thread_sender.unbounded_send(snapshot);
                });
                if let Err(error) = result {
                    eprintln!("GNOS_UX_AUDIO_OBSERVER_FAIL error={error}");
                    let _ = sender.unbounded_send(unavailable_snapshot());
                }
            });
        if let Err(error) = spawn {
            this.started = false;
            eprintln!("GNOS_UX_AUDIO_THREAD_FAIL error={error}");
            return;
        }
        cx.spawn(async move |_this, task_cx| {
            while let Some(snapshot) = receiver.next().await {
                entity.update(&mut *task_cx, |this, cx| this.set_snapshot(snapshot, cx));
            }
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_starts_unavailable_and_not_started() {
        let session = AudioSession::new();
        assert_eq!(
            session.snapshot().provider,
            gnos_audio::AudioProviderState::Unavailable
        );
        assert!(!session.started());
    }
}
