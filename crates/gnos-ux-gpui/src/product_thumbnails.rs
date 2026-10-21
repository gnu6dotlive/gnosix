//! WS-04: the live thumbnails, shared by every output's scene.
//!
//! One capture worker (`gnos_thumbnails`) for the whole process, started
//! when a scene first wishes for a thumbnail and ended when none does: no
//! Wayland connection, no buffer and no image is held while every
//! workspaces panel is in.
//!
//! - Each scene wishes for the windows its panel shows, with the size of
//!   their miniature in device pixels; the hub asks the worker for their
//!   union.
//! - A window is named by its `SurfaceId`; the worker names it by its
//!   Hyprland address. The hub keeps the correspondence, read with one
//!   fixed `j/clients` query (`gnosix_hyprland`), off the UI thread, again
//!   when a wished window has none.
//! - A thumbnail replaces the previous one; the previous one leaves the
//!   GPU atlas at once. Locked, closed, refused or no longer wished for, a
//!   window's image is dropped the same way.
//!
//! Receipt: `GNOS_UX_THUMBNAILS`, on change.

use futures::{StreamExt, channel::mpsc};
use gnos_thumbnails::{Event, ThumbnailService, Wanted};
use gnosix_desktop_model::SurfaceId;
use gpui::{AppContext, Context, Entity, RenderImage};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One scene's wish: its windows and the size of their miniature, in
/// device pixels.
pub type Wish = Vec<(SurfaceId, (u32, u32))>;

/// How soon the addresses may be read again for a window that has none.
const ADDRESSES_AGAIN: Duration = Duration::from_millis(500);

pub struct ThumbnailHub {
    service: Option<ThumbnailService>,
    /// Bumped at each start: events from an ended worker are ignored.
    epoch: u64,
    failed: Option<String>,
    /// Each scene's wish: its windows and the size of their miniature.
    wishes: BTreeMap<u64, Wish>,
    refresh: Duration,
    addresses: HashMap<SurfaceId, u64>,
    addresses_read: Option<Instant>,
    reading: bool,
    images: HashMap<u64, Arc<RenderImage>>,
    locked: bool,
    sent: Vec<Wanted>,
    reported: Option<String>,
}

#[derive(Clone)]
struct HubGlobal(Entity<ThumbnailHub>);
impl gpui::Global for HubGlobal {}

impl ThumbnailHub {
    pub fn shared(cx: &mut gpui::App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<HubGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            service: None,
            epoch: 0,
            failed: None,
            wishes: BTreeMap::new(),
            refresh: Duration::from_secs(1),
            addresses: HashMap::new(),
            addresses_read: None,
            reading: false,
            images: HashMap::new(),
            locked: false,
            sent: Vec::new(),
            reported: None,
        });
        cx.set_global(HubGlobal(entity.clone()));
        entity
    }

    /// The thumbnail of a window, if one has been taken.
    pub fn image(&self, surface: SurfaceId) -> Option<Arc<RenderImage>> {
        let address = self.addresses.get(&surface)?;
        self.images.get(address).cloned()
    }

    /// One scene's wish (empty: its panel is in, or thumbnails are off).
    pub fn wish(&mut self, host: u64, windows: Wish, refresh: Duration, cx: &mut Context<Self>) {
        let changed = if windows.is_empty() {
            self.wishes.remove(&host).is_some()
        } else {
            self.wishes.insert(host, windows.clone()) != Some(windows)
        };
        if changed || refresh != self.refresh {
            self.refresh = refresh;
            self.follow(cx);
        }
    }

    /// Ask the worker for the union of the wishes; start or end it.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let mut wanted: Vec<Wanted> = Vec::new();
        let mut unknown = false;
        for (surface, max) in self.wishes.values().flatten() {
            match self.addresses.get(surface) {
                Some(&address) => match wanted.iter_mut().find(|w| w.address == address) {
                    Some(w) => w.max = (w.max.0.max(max.0), w.max.1.max(max.1)),
                    None => wanted.push(Wanted { address, max: *max }),
                },
                None => unknown = true,
            }
        }
        if unknown {
            self.read_addresses(cx);
        }
        if self.wishes.is_empty() {
            // Every panel is in: end the worker and forget every image.
            self.service = None;
            self.sent.clear();
            self.clear_images(cx);
            self.report();
            return;
        }
        if self.service.is_none() && self.failed.is_none() {
            self.start(cx);
        }
        // Images of windows no longer wished for go.
        let gone: Vec<u64> = self
            .images
            .keys()
            .filter(|a| !wanted.iter().any(|w| w.address == **a))
            .copied()
            .collect();
        for address in gone {
            self.drop_image(address, cx);
        }
        wanted.sort_by_key(|w| w.address);
        if wanted != self.sent {
            if let Some(service) = &self.service {
                service.want(wanted.clone(), self.refresh);
            }
            self.sent = wanted;
        } else if let Some(service) = &self.service {
            // The refresh may have changed alone.
            service.want(wanted, self.refresh);
        }
        self.report();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        let (sender, mut receiver) = mpsc::unbounded::<Event>();
        let mut bus: Option<zbus::blocking::Connection> = None;
        let locked = move || {
            if bus.is_none() {
                bus = zbus::blocking::Connection::system().ok();
            }
            session_locked(bus.as_ref()?)
        };
        match ThumbnailService::start(move |e| drop(sender.unbounded_send(e)), locked) {
            Ok(service) => {
                self.service = Some(service);
                self.sent.clear();
                cx.spawn(async move |this, cx| {
                    while let Some(event) = receiver.next().await {
                        let alive = this
                            .update(cx, |hub, cx| hub.receive(epoch, event, cx))
                            .is_ok();
                        if !alive {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(e) => {
                self.failed = Some(e.to_string());
                self.report();
            }
        }
    }

    fn receive(&mut self, epoch: u64, event: Event, cx: &mut Context<Self>) {
        if epoch != self.epoch || self.service.is_none() {
            return;
        }
        match event {
            Event::Ready {
                address,
                width,
                height,
                bgra,
            } => {
                if !self.sent.iter().any(|w| w.address == address) {
                    return;
                }
                let Some(buffer) = image::RgbaImage::from_raw(width, height, bgra) else {
                    return;
                };
                self.locked = false;
                let fresh = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
                if let Some(old) = self.images.insert(address, fresh) {
                    cx.drop_image(old, None);
                }
                cx.notify();
            }
            Event::Refused { address, reason } => {
                println!("GNOS_UX_THUMBNAIL address={address:#x} outcome={reason}");
                self.drop_image(address, cx);
            }
            Event::Locked => {
                self.locked = true;
                self.clear_images(cx);
            }
            Event::Unavailable(reason) => {
                eprintln!("GNOS_UX_THUMBNAILS state=unavailable reason={reason}");
                self.failed = Some(reason);
                self.service = None;
                self.clear_images(cx);
            }
        }
        self.report();
    }

    fn read_addresses(&mut self, cx: &mut Context<Self>) {
        if self.reading
            || self
                .addresses_read
                .is_some_and(|at| at.elapsed() < ADDRESSES_AGAIN)
        {
            return;
        }
        let Ok(instance) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") else {
            return;
        };
        self.reading = true;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { gnosix_hyprland::semantic::client_window_addresses(&instance) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.reading = false;
                hub.addresses_read = Some(Instant::now());
                if let Ok(pairs) = read {
                    let fresh: HashMap<SurfaceId, u64> = pairs.into_iter().collect();
                    if fresh != hub.addresses {
                        hub.addresses = fresh;
                        hub.follow(cx);
                    }
                }
            });
        })
        .detach();
    }

    fn drop_image(&mut self, address: u64, cx: &mut Context<Self>) {
        if let Some(old) = self.images.remove(&address) {
            cx.drop_image(old, None);
            cx.notify();
        }
    }

    fn clear_images(&mut self, cx: &mut Context<Self>) {
        let addresses: Vec<u64> = self.images.keys().copied().collect();
        for address in addresses {
            self.drop_image(address, cx);
        }
    }

    fn report(&mut self) {
        let wished: usize = self.wishes.values().map(Vec::len).sum();
        let line = format!(
            "GNOS_UX_THUMBNAILS running={} wished={wished} capturing={} shown={} refresh_ms={} locked={} failed={}",
            self.service.is_some(),
            self.sent.len(),
            self.images.len(),
            self.refresh.as_millis(),
            self.locked,
            self.failed.as_deref().unwrap_or("none").replace(' ', "_"),
        );
        if self.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.reported = Some(line);
        }
    }
}

/// Whether logind reports the session locked (`LockedHint`); `None` when it
/// cannot be read (the worker then captures nothing).
fn session_locked(bus: &zbus::blocking::Connection) -> Option<bool> {
    let reply = bus
        .call_method(
            Some("org.freedesktop.login1"),
            gnos_utilities::OWN_SESSION_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.login1.Session", "LockedHint"),
        )
        .ok()?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
    bool::try_from(value).ok()
}
