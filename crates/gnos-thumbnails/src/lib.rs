//! WS-04: live thumbnails of windows, for the workspaces panel.
//!
//! A worker thread holds its own Wayland connection and captures, with the
//! standard `ext-image-copy-capture-v1` on `ext-foreign-toplevel-list-v1`
//! sources, only the windows the shell asks for, only while it asks:
//!
//! - a window is named by its Hyprland address, which
//!   `hyprland-toplevel-mapping-v1` gives for each toplevel;
//! - a capture is taken without the cursor, brought down to the size asked
//!   for (`scale`), and handed over in memory (BGRA). Nothing is written
//!   anywhere, and the full-size buffer is reused, never kept;
//! - the next capture of a window waits for the refresh period the operator
//!   chose. Hyprland 0.56 completes every frame at once and reports the whole
//!   window damaged, so a still window is still copied once per period; its
//!   thumbnail, unchanged, is not handed over again (no work for the shell,
//!   no new texture);
//! - before every capture the worker asks whether the session is locked.
//!   Locked, or unknown, it captures nothing and says so
//!   ([`Event::Locked`]), so the shell drops what it shows;
//! - asking for nothing ends every capture session and frees its buffers.
//!
//! Which windows may be captured at all is the shell's to decide
//! ([`policy`]): an excluded application is never asked for.

pub mod policy;
pub mod scale;
mod worker;

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

/// A window the shell wants a thumbnail of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Wanted {
    /// The window's Hyprland address.
    pub address: u64,
    /// The largest thumbnail that fits its miniature, in device pixels.
    pub max: (u32, u32),
}

/// What the worker reports.
#[derive(Debug)]
pub enum Event {
    /// A fresh thumbnail: `width` × `height`, 4 bytes per pixel, BGRA.
    Ready {
        address: u64,
        width: u32,
        height: u32,
        bgra: Vec<u8>,
    },
    /// The window cannot be captured (closed, refused, no usable format).
    Refused { address: u64, reason: &'static str },
    /// The session is locked, or its state unknown: nothing is captured.
    Locked,
    /// The compositor does not offer what capturing needs.
    Unavailable(String),
}

pub(crate) enum Command {
    Want(Vec<Wanted>, Duration),
    Quit,
}

/// The capture worker. Dropping it ends the thread and every session.
pub struct ThumbnailService {
    commands: mpsc::Sender<Command>,
    wake: UnixStream,
}

impl ThumbnailService {
    /// Start the worker on the session's Wayland display.
    ///
    /// `on_event` runs on the worker thread. `locked` answers whether the
    /// session is locked (`None`: unknown, treated as locked).
    pub fn start(
        on_event: impl Fn(Event) + Send + Sync + 'static,
        locked: impl FnMut() -> Option<bool> + Send + 'static,
    ) -> io::Result<Self> {
        let (commands, receive) = mpsc::channel();
        let (wake, wake_worker) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wake_worker.set_nonblocking(true)?;
        std::thread::Builder::new()
            .name("gnos-thumbnails".to_owned())
            .spawn(move || {
                let on_event: worker::OnEvent = std::sync::Arc::new(on_event);
                if let Err(e) = worker::run(receive, wake_worker, on_event.clone(), locked) {
                    on_event(Event::Unavailable(e));
                }
            })?;
        Ok(Self { commands, wake })
    }

    /// The windows wanted now, and how often each may be captured again.
    /// An empty list ends every capture.
    pub fn want(&self, wanted: Vec<Wanted>, refresh: Duration) {
        if self.commands.send(Command::Want(wanted, refresh)).is_ok() {
            let _ = (&self.wake).write(&[1]);
        }
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        if self.commands.send(Command::Quit).is_ok() {
            let _ = (&self.wake).write(&[1]);
        }
    }
}
