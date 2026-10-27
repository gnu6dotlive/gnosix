//! UXE-02B: hosts the native notification service for the scene.
//!
//! The service runs on its own thread with its own runtime and publishes
//! every transition into this entity. When another service owns
//! `org.freedesktop.Notifications`, the state is `NameTaken` and no toast is
//! ever shown: Gnosix never displaces the running service (G01).

use futures::{StreamExt, channel::mpsc};
use gnos_notifications::{
    BusEvent, Command, NotificationControl, ServeError, ServerUpdate, StoreConfig,
};
use gpui::{AppContext, Context, Entity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationServiceState {
    NotStarted,
    Starting,
    Serving,
    NameTaken { owner: Option<String> },
    Failed(String),
}

pub struct NotificationSession {
    state: NotificationServiceState,
    latest: Option<ServerUpdate>,
    control: Option<NotificationControl>,
}

#[derive(Clone)]
struct NotificationGlobal(Entity<NotificationSession>);
impl gpui::Global for NotificationGlobal {}

enum Message {
    Update(ServerUpdate),
    Ended(ServeError),
}

impl NotificationSession {
    fn new() -> Self {
        Self {
            state: NotificationServiceState::NotStarted,
            latest: None,
            control: None,
        }
    }

    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<NotificationGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self::new());
        cx.set_global(NotificationGlobal(entity.clone()));
        entity
    }

    pub fn state(&self) -> &NotificationServiceState {
        &self.state
    }

    pub fn latest(&self) -> Option<&ServerUpdate> {
        self.latest.as_ref()
    }

    /// Sends one surface command; `false` when no service is serving.
    pub fn send(&self, command: Command) -> bool {
        self.state == NotificationServiceState::Serving
            && self.control.as_ref().is_some_and(|c| c.send(command))
    }

    fn receive(&mut self, message: Message, cx: &mut Context<Self>) {
        match message {
            Message::Update(update) => {
                if self.state != NotificationServiceState::Serving {
                    println!("GNOS_UX_UXE02_SERVICE state=serving");
                }
                self.state = NotificationServiceState::Serving;
                for event in &update.events {
                    match event {
                        BusEvent::Notified { id, replaced } => println!(
                            "GNOS_UX_UXE02_BUS event=notified id={} replaced={replaced}",
                            id.get()
                        ),
                        BusEvent::Closed(closed) => println!(
                            "GNOS_UX_UXE02_BUS event=closed id={} reason={}",
                            closed.id.get(),
                            closed.reason.wire()
                        ),
                        BusEvent::ActionInvoked { id, key } => println!(
                            "GNOS_UX_UXE02_BUS event=action_invoked id={} key={key}",
                            id.get()
                        ),
                    }
                }
                self.latest = Some(update);
            }
            Message::Ended(error) => {
                self.state = match error {
                    ServeError::NameTaken { owner } => {
                        println!(
                            "GNOS_UX_UXE02_SERVICE state=name_taken owner={}",
                            owner.as_deref().unwrap_or("unknown")
                        );
                        NotificationServiceState::NameTaken { owner }
                    }
                    other => {
                        eprintln!("GNOS_UX_UXE02_SERVICE state=failed error={other}");
                        NotificationServiceState::Failed(other.to_string())
                    }
                };
                self.latest = None;
                self.control = None;
            }
        }
        cx.notify();
    }
}

/// Starts the service once per process.
pub fn ensure_service(session: &Entity<NotificationSession>, cx: &mut gpui::App) {
    session.update(cx, |this, cx| {
        if this.state != NotificationServiceState::NotStarted {
            return;
        }
        this.state = NotificationServiceState::Starting;
        let (control, commands) = gnos_notifications::control_channel();
        this.control = Some(control);
        let entity = cx.entity();
        let (sender, mut receiver) = mpsc::unbounded::<Message>();
        let publisher = sender.clone();
        let spawn = std::thread::Builder::new()
            .name("gnos-notifications".into())
            .spawn(move || {
                let result = gnos_notifications::run_session(
                    StoreConfig::default(),
                    move |update| {
                        let _ = publisher.unbounded_send(Message::Update(update));
                    },
                    commands,
                );
                if let Err(error) = result {
                    let _ = sender.unbounded_send(Message::Ended(error));
                }
            });
        if let Err(error) = spawn {
            this.state = NotificationServiceState::Failed(error.to_string());
            this.control = None;
            eprintln!("GNOS_UX_UXE02_SERVICE state=failed error={error}");
            return;
        }
        cx.spawn(async move |_this, task_cx| {
            while let Some(message) = receiver.next().await {
                entity.update(&mut *task_cx, |this, cx| this.receive(message, cx));
            }
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_session_serves_nothing() {
        let session = NotificationSession::new();
        assert_eq!(session.state(), &NotificationServiceState::NotStarted);
        assert!(session.latest().is_none());
        assert!(!session.send(Command::ClearAll));
    }
}
