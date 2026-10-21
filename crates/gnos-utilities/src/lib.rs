//! SURF-01A (U01, U02): the Utilities powers that no observer crate owns.
//!
//! - **Keep awake** (`idle`): a logind idle inhibitor, held as long as its
//!   descriptor lives.
//! - **Brightness** (`brightness`): laptop backlights through logind
//!   `Session.SetBrightness`, external displays through DDC/CI (`ddcutil`,
//!   VCP 0x10).
//!
//! - **Session and power** (`session`, SURF-02A): lock, suspend, log out,
//!   reboot and power off through logind, revalidated against its
//!   capabilities and inhibitors.
//!
//! - **The dashboard's facts** (`resources`, SURF-03A): CPU, memory, disk,
//!   temperature and the system, read only.
//!
//! Each power is a typed request with a typed outcome; the product shows a
//! new state only once it is observed.

mod brightness;
mod displays;
mod idle;
mod resources;
mod session;

pub use brightness::*;
pub use displays::*;
pub use idle::*;
pub use resources::*;
pub use session::*;
