//! The capture worker: its own Wayland connection, its own thread.

use crate::{Command, Event, Wanted, scale};
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, mpsc};

/// Where the worker's reports go.
pub(crate) type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;
use std::time::{Duration, Instant};

use wayland_client::backend::ObjectId;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

#[allow(dead_code, non_camel_case_types, non_upper_case_globals, clippy::all)]
mod mapping {
    use wayland_client;
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::*;

    pub mod __interfaces {
        use wayland_protocols::ext::foreign_toplevel_list::v1::client::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-mapping-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-mapping-v1.xml");
}
use mapping::hyprland_toplevel_mapping_manager_v1::HyprlandToplevelMappingManagerV1;
use mapping::hyprland_toplevel_window_mapping_handle_v1::{
    self, HyprlandToplevelWindowMappingHandleV1,
};

/// How long a lock answer is trusted before asking again.
const LOCK_FRESH: Duration = Duration::from_millis(500);
/// The longest the worker sleeps with nothing due (it still wakes on any
/// compositor event or request from the shell).
const IDLE: Duration = Duration::from_secs(5);

struct Toplevel {
    handle: ExtForeignToplevelHandleV1,
    address: Option<u64>,
    _mapping: HyprlandToplevelWindowMappingHandleV1,
}

/// A shared-memory buffer the compositor copies a window into.
struct Shm {
    size: (u32, u32),
    format: wl_shm::Format,
    buffer: wl_buffer::WlBuffer,
    map: memmap2::MmapMut,
}

impl Drop for Shm {
    fn drop(&mut self) {
        self.buffer.destroy();
    }
}

struct Capture {
    max: (u32, u32),
    session: ExtImageCopyCaptureSessionV1,
    source: ExtImageCaptureSourceV1,
    /// The buffer constraints, once the session has said `done`.
    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
    constraints_done: bool,
    /// The next constraint event starts a new set (after a `done`): the
    /// protocol does not order `buffer_size` before `shm_format`.
    constraints_fresh: bool,
    shm: Option<Shm>,
    frame: Option<ExtImageCopyCaptureFrameV1>,
    next_at: Instant,
    /// The last thumbnail handed over: an identical one is not sent again
    /// (Hyprland reports the whole window damaged on every frame).
    last: Option<Vec<u8>>,
    /// Refused or stopped: nothing more is asked of it.
    over: bool,
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            frame.destroy();
        }
        self.session.destroy();
        self.source.destroy();
    }
}

struct State {
    on_event: OnEvent,
    shm: wl_shm::WlShm,
    mapping: HyprlandToplevelMappingManagerV1,
    sources: ExtForeignToplevelImageCaptureSourceManagerV1,
    copy: ExtImageCopyCaptureManagerV1,
    toplevels: HashMap<ObjectId, Toplevel>,
    captures: HashMap<u64, Capture>,
    refresh: Duration,
    locked_at: Option<(Instant, bool)>,
}

pub(crate) fn run(
    commands: mpsc::Receiver<Command>,
    mut wake: UnixStream,
    on_event: OnEvent,
    mut locked: impl FnMut() -> Option<bool>,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("wayland: {e}"))?;
    let (globals, mut queue): (_, EventQueue<State>) =
        registry_queue_init(&conn).map_err(|e| format!("registry: {e}"))?;
    let qh = queue.handle();
    let bind = |name: &str| format!("the compositor does not offer {name}");
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).map_err(|_| bind("wl_shm"))?;
    let list: ExtForeignToplevelListV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| bind("ext_foreign_toplevel_list_v1"))?;
    let mapping: HyprlandToplevelMappingManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| bind("hyprland_toplevel_mapping_manager_v1"))?;
    let sources: ExtForeignToplevelImageCaptureSourceManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| bind("ext_foreign_toplevel_image_capture_source_manager_v1"))?;
    let copy: ExtImageCopyCaptureManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| bind("ext_image_copy_capture_manager_v1"))?;
    let mut state = State {
        on_event,
        shm,
        mapping,
        sources,
        copy,
        toplevels: HashMap::new(),
        captures: HashMap::new(),
        refresh: Duration::from_secs(1),
        locked_at: None,
    };
    let mut wanted: Vec<Wanted> = Vec::new();
    // The toplevels as they stand now (and their addresses).
    queue
        .roundtrip(&mut state)
        .map_err(|e| format!("roundtrip: {e}"))?;
    queue
        .roundtrip(&mut state)
        .map_err(|e| format!("roundtrip: {e}"))?;

    loop {
        loop {
            match commands.try_recv() {
                Ok(Command::Want(w, refresh)) => {
                    wanted = w;
                    state.refresh = refresh;
                }
                Ok(Command::Quit) | Err(mpsc::TryRecvError::Disconnected) => {
                    state.captures.clear();
                    list.stop();
                    let _ = queue.flush();
                    return Ok(());
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        state.follow(&wanted, &qh, &mut locked);
        queue.flush().map_err(|e| format!("flush: {e}"))?;
        let timeout = state
            .captures
            .values()
            .filter(|c| !c.over && c.frame.is_none() && c.constraints_done)
            .map(|c| c.next_at.saturating_duration_since(Instant::now()))
            .min()
            .unwrap_or(IDLE)
            .min(IDLE);
        if let Some(guard) = queue.prepare_read() {
            let (wl_ready, wake_ready) = poll2(guard.connection_fd(), wake.as_fd(), timeout);
            if wl_ready {
                // Woken for nothing to read (EAGAIN) is not an error.
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(format!("read: {e}")),
                }
            } else {
                drop(guard);
            }
            if wake_ready {
                let mut sink = [0u8; 64];
                while matches!(wake.read(&mut sink), Ok(n) if n > 0) {}
            }
        }
        queue
            .dispatch_pending(&mut state)
            .map_err(|e| format!("dispatch: {e}"))?;
    }
}

/// Wait for either descriptor to be readable, up to `timeout`.
fn poll2(
    a: std::os::fd::BorrowedFd,
    b: std::os::fd::BorrowedFd,
    timeout: Duration,
) -> (bool, bool) {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    let mut fds = [
        PollFd::new(&a, PollFlags::IN),
        PollFd::new(&b, PollFlags::IN),
    ];
    let ts = Timespec {
        tv_sec: timeout.as_secs() as i64,
        tv_nsec: i64::from(timeout.subsec_nanos()),
    };
    if poll(&mut fds, Some(&ts)).is_err() {
        return (false, false);
    }
    (!fds[0].revents().is_empty(), !fds[1].revents().is_empty())
}

impl State {
    /// Whether the session is unlocked, asked at most every `LOCK_FRESH`.
    fn unlocked(&mut self, locked: &mut impl FnMut() -> Option<bool>) -> bool {
        let now = Instant::now();
        let fresh = self
            .locked_at
            .filter(|(at, _)| now.duration_since(*at) < LOCK_FRESH);
        let is_locked = match fresh {
            Some((_, l)) => l,
            None => {
                let l = locked().unwrap_or(true);
                if l && self.locked_at.is_none_or(|(_, was)| !was) {
                    (self.on_event)(Event::Locked);
                }
                self.locked_at = Some((now, l));
                l
            }
        };
        !is_locked
    }

    /// Open the sessions wanted, close the others, take the captures due.
    fn follow(
        &mut self,
        wanted: &[Wanted],
        qh: &QueueHandle<State>,
        locked: &mut impl FnMut() -> Option<bool>,
    ) {
        self.captures
            .retain(|address, _| wanted.iter().any(|w| w.address == *address));
        if wanted.is_empty() {
            return;
        }
        // Locked: no new session, no frame. What is open stays idle.
        if !self.unlocked(locked) {
            return;
        }
        for w in wanted {
            if let Some(c) = self.captures.get_mut(&w.address) {
                c.max = w.max;
                continue;
            }
            let Some(top) = self
                .toplevels
                .values()
                .find(|t| t.address == Some(w.address))
            else {
                continue;
            };
            let source = self.sources.create_source(&top.handle, qh, ());
            let session = self.copy.create_session(
                &source,
                ext_image_copy_capture_manager_v1::Options::empty(),
                qh,
                w.address,
            );
            self.captures.insert(
                w.address,
                Capture {
                    max: w.max,
                    session,
                    source,
                    size: None,
                    formats: Vec::new(),
                    constraints_done: false,
                    constraints_fresh: true,
                    shm: None,
                    frame: None,
                    next_at: Instant::now(),
                    last: None,
                    over: false,
                },
            );
        }
        let now = Instant::now();
        let shm = self.shm.clone();
        let mut refused = Vec::new();
        for (address, c) in &mut self.captures {
            if c.over || c.frame.is_some() || !c.constraints_done || now < c.next_at {
                continue;
            }
            let Some(size) = c.size else { continue };
            // Little-endian ARGB/XRGB: bytes B, G, R, A.
            let format = [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888]
                .into_iter()
                .find(|f| c.formats.contains(f));
            let Some(format) = format else {
                if std::env::var_os("GNOS_THUMBNAILS_TRACE").is_some() {
                    eprintln!(
                        "GNOS_THUMBNAILS_TRACE address={address:#x} size={size:?} shm_formats={:?}",
                        c.formats
                    );
                }
                c.over = true;
                refused.push((*address, "no_shm_format"));
                continue;
            };
            if c.shm
                .as_ref()
                .is_none_or(|s| s.size != size || s.format != format)
            {
                c.shm = None;
                match new_shm(&shm, size, format, qh) {
                    Some(s) => c.shm = Some(s),
                    None => {
                        c.over = true;
                        refused.push((*address, "shm"));
                        continue;
                    }
                }
            }
            let Some(buffer) = c.shm.as_ref().map(|s| s.buffer.clone()) else {
                continue;
            };
            let frame = c.session.create_frame(qh, *address);
            frame.attach_buffer(&buffer);
            frame.damage_buffer(0, 0, size.0 as i32, size.1 as i32);
            frame.capture();
            c.frame = Some(frame);
        }
        for (address, reason) in refused {
            (self.on_event)(Event::Refused { address, reason });
        }
    }

    /// A frame landed: bring it down to its miniature and hand it over.
    fn ready(&mut self, address: u64) {
        let Some(c) = self.captures.get_mut(&address) else {
            return;
        };
        if let Some(frame) = c.frame.take() {
            frame.destroy();
        }
        c.next_at = Instant::now() + self.refresh;
        let Some(shm) = c.shm.as_ref() else { return };
        let target = scale::fit(shm.size, c.max);
        let opaque = shm.format == wl_shm::Format::Xrgb8888;
        let stride = shm.size.0 as usize * 4;
        let started = Instant::now();
        let bgra = scale::downscale(&shm.map, shm.size, stride, target, opaque);
        if std::env::var_os("GNOS_THUMBNAILS_TRACE").is_some() {
            eprintln!(
                "GNOS_THUMBNAILS_TRACE address={address:#x} source={:?} target={target:?} downscale_us={} same={}",
                shm.size,
                started.elapsed().as_micros(),
                bgra.is_some() && bgra == c.last
            );
        }
        if let Some(bgra) = bgra
            && c.last.as_ref() != Some(&bgra)
        {
            c.last = Some(bgra.clone());
            (self.on_event)(Event::Ready {
                address,
                width: target.0,
                height: target.1,
                bgra,
            });
        }
    }
}

/// A buffer of `size` in `format`, in a sealed-off memory file.
fn new_shm(
    shm: &wl_shm::WlShm,
    (w, h): (u32, u32),
    format: wl_shm::Format,
    qh: &QueueHandle<State>,
) -> Option<Shm> {
    use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
    let stride = w.checked_mul(4)?;
    let len = stride.checked_mul(h)?;
    let fd = memfd_create("gnos-thumbnail", MemfdFlags::CLOEXEC).ok()?;
    ftruncate(&fd, u64::from(len)).ok()?;
    let file = std::fs::File::from(fd);
    // SAFETY: the memory file is this process's own, sized above and never
    // resized; the compositor only writes into it during a capture, and the
    // worker reads it only after that capture's `ready`.
    #[allow(unsafe_code)]
    let map = unsafe { memmap2::MmapMut::map_mut(&file) }.ok()?;
    let pool = shm.create_pool(file.as_fd(), i32::try_from(len).ok()?, qh, ());
    let buffer = pool.create_buffer(
        0,
        i32::try_from(w).ok()?,
        i32::try_from(h).ok()?,
        i32::try_from(stride).ok()?,
        format,
        qh,
        (),
    );
    pool.destroy();
    Some(Shm {
        size: (w, h),
        format,
        buffer,
        map,
    })
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! ignore {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore!(
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    HyprlandToplevelMappingManagerV1,
    ExtForeignToplevelImageCaptureSourceManagerV1,
    ExtImageCaptureSourceV1,
    ExtImageCopyCaptureManagerV1
);

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            let mapping = state
                .mapping
                .get_window_for_toplevel(&toplevel, qh, toplevel.id());
            state.toplevels.insert(
                toplevel.id(),
                Toplevel {
                    handle: toplevel,
                    address: None,
                    _mapping: mapping,
                },
            );
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_handle_v1::Event::Closed = event
            && let Some(top) = state.toplevels.remove(&handle.id())
        {
            if let Some(address) = top.address
                && state.captures.remove(&address).is_some()
            {
                (state.on_event)(Event::Refused {
                    address,
                    reason: "closed",
                });
            }
            top.handle.destroy();
        }
    }
}

impl Dispatch<HyprlandToplevelWindowMappingHandleV1, ObjectId> for State {
    fn event(
        state: &mut Self,
        handle: &HyprlandToplevelWindowMappingHandleV1,
        event: hyprland_toplevel_window_mapping_handle_v1::Event,
        toplevel: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let hyprland_toplevel_window_mapping_handle_v1::Event::WindowAddress {
            address_hi,
            address,
        } = event
            && let Some(top) = state.toplevels.get_mut(toplevel)
        {
            top.address = Some((u64::from(address_hi) << 32) | u64::from(address));
        }
        handle.destroy();
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, u64> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        address: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_session_v1::Event as E;
        let Some(c) = state.captures.get_mut(address) else {
            return;
        };
        let begin = |c: &mut Capture| {
            if c.constraints_fresh {
                c.constraints_fresh = false;
                c.constraints_done = false;
                c.formats.clear();
                c.size = None;
            }
        };
        match event {
            E::BufferSize { width, height } => {
                begin(c);
                c.size = Some((width, height));
            }
            E::ShmFormat {
                format: WEnum::Value(f),
            } => {
                begin(c);
                c.formats.push(f);
            }
            E::DmabufDevice { .. } | E::DmabufFormat { .. } => begin(c),
            E::Done => {
                c.constraints_done = true;
                c.constraints_fresh = true;
            }
            E::Stopped => {
                state.captures.remove(address);
                (state.on_event)(Event::Refused {
                    address: *address,
                    reason: "stopped",
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, u64> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        address: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_frame_v1::{Event as E, FailureReason};
        match event {
            E::Damage {
                x,
                y,
                width,
                height,
            } if std::env::var_os("GNOS_THUMBNAILS_TRACE").is_some() => {
                eprintln!(
                    "GNOS_THUMBNAILS_TRACE address={address:#x} damage=[{x},{y},{width},{height}]"
                );
            }
            E::Ready => state.ready(*address),
            E::Failed { reason } => {
                let Some(c) = state.captures.get_mut(address) else {
                    return;
                };
                if let Some(frame) = c.frame.take() {
                    frame.destroy();
                }
                c.next_at = Instant::now() + state.refresh;
                match reason {
                    // New constraints follow: the next `done` retries.
                    WEnum::Value(FailureReason::BufferConstraints) => {
                        c.constraints_done = false;
                        c.shm = None;
                    }
                    WEnum::Value(FailureReason::Stopped) => {
                        state.captures.remove(address);
                        (state.on_event)(Event::Refused {
                            address: *address,
                            reason: "stopped",
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
