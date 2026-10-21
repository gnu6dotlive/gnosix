//! Native Gnosix notification service (UXE-02).
//!
//! `model` holds the validated wire vocabulary, `store` the pure lifetime
//! rules, and `server` the `org.freedesktop.Notifications` export that never
//! displaces an existing owner.

pub mod model;
pub mod prefs;
pub mod server;
pub mod store;

pub use model::{Action, CloseReason, ImageSource, NotificationId, RawImage, Urgency};
pub use server::{
    BUS_NAME, BusEvent, CAPABILITIES, Command, CommandReceiver, NotificationControl, OBJECT_PATH,
    ServeError, ServerUpdate, control_channel, run_session, serve_on,
};
pub use store::{Notification, NotificationSnapshot, StoreConfig, Toast};
