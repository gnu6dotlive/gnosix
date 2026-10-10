//! Cabinet: Gnosix native file selection and the bounded XDG portal backend.
//! GPUI UI is opt-in behind the native-gpui feature; host portal defaults are never installed.
pub mod cabinet_choices;
pub mod cabinet_filters;
pub mod cabinet_fs;
pub mod cabinet_navigation;
pub mod filename_text;
pub mod render_wire;
use crate::cabinet_filters::CabinetFilter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, watch};
use zbus::zvariant::OwnedValue;

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.gnosix";
pub const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
pub const MAX_REQUESTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickerKind {
    OpenFile,
    SaveFile,
    SaveFiles,
}

#[derive(Debug)]
pub struct PickerInvocation {
    pub handle: String,
    pub app_id: String,
    pub parent_window: String,
    pub title: String,
    pub kind: PickerKind,
    /// Portal options are validated and translated to a bounded renderer contract.
    pub options: HashMap<String, OwnedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Selected(Vec<PathBuf>),
    SelectedWithFilter {
        paths: Vec<PathBuf>,
        filter: CabinetFilter,
    },
    SelectedWithMetadata {
        paths: Vec<PathBuf>,
        filter: Option<CabinetFilter>,
        choices: Vec<(String, String)>,
    },
    Cancelled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalReply {
    /// 0 = success; 1 = cancelled; 2 = backend error.
    pub code: u32,
    pub uris: Vec<String>,
    pub current_filter: Option<CabinetFilter>,
    pub choices: Option<Vec<(String, String)>>,
}

impl PortalReply {
    fn cancelled() -> Self {
        Self {
            code: 1,
            uris: vec![],
            current_filter: None,
            choices: None,
        }
    }
    fn unavailable() -> Self {
        Self {
            code: 2,
            uris: vec![],
            current_filter: None,
            choices: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmitError {
    Capacity,
    Duplicate,
    InvalidOptions,
}

struct Pending {
    epoch: u64,
    kind: PickerKind,
    multiple: bool,
    reply: oneshot::Sender<PortalReply>,
    cancel: watch::Sender<bool>,
}

#[derive(Default)]
struct Inner {
    next_epoch: u64,
    requests: HashMap<String, Pending>,
}

/// Scoped lifecycle authority: every active handle owns exactly one completion.
#[derive(Clone)]
pub struct Registry {
    inner: Arc<Mutex<Inner>>,
    capacity: usize,
}

impl Registry {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            capacity,
        }
    }

    pub fn active(&self) -> usize {
        self.inner.lock().expect("registry poisoned").requests.len()
    }

    pub fn admit(&self, invocation: &PickerInvocation) -> Result<Ticket, AdmitError> {
        let multiple = match invocation.options.get("multiple") {
            Some(value) if invocation.kind == PickerKind::OpenFile => {
                bool::try_from(value).map_err(|_| AdmitError::InvalidOptions)?
            }
            Some(_) => return Err(AdmitError::InvalidOptions),
            None => false,
        };
        for flag in ["modal", "directory", "writable"] {
            if let Some(value) = invocation.options.get(flag) {
                let _: bool = bool::try_from(value).map_err(|_| AdmitError::InvalidOptions)?;
            }
        }
        let mut inner = self.inner.lock().expect("registry poisoned");
        if inner.requests.contains_key(&invocation.handle) {
            return Err(AdmitError::Duplicate);
        }
        if inner.requests.len() >= self.capacity {
            return Err(AdmitError::Capacity);
        }
        let (sender, receiver) = oneshot::channel();
        let (cancel_sender, cancel_receiver) = watch::channel(false);
        inner.next_epoch = inner.next_epoch.wrapping_add(1);
        let epoch = inner.next_epoch;
        inner.requests.insert(
            invocation.handle.clone(),
            Pending {
                epoch,
                kind: invocation.kind,
                multiple,
                reply: sender,
                cancel: cancel_sender,
            },
        );
        Ok(Ticket {
            handle: invocation.handle.clone(),
            epoch,
            registry: self.clone(),
            receiver,
            cancelled: cancel_receiver,
        })
    }

    pub fn complete(&self, handle: &str, resolution: Resolution) -> bool {
        let pending = self
            .inner
            .lock()
            .expect("registry poisoned")
            .requests
            .remove(handle);
        let Some(pending) = pending else {
            return false;
        };
        let result = resolution_to_reply(pending.kind, pending.multiple, resolution);
        pending.cancel.send_replace(true);
        let _ = pending.reply.send(result);
        true
    }

    pub fn complete_if_epoch(&self, handle: &str, epoch: u64, resolution: Resolution) -> bool {
        let mut inner = self.inner.lock().expect("registry poisoned");
        if inner.requests.get(handle).map(|p| p.epoch) != Some(epoch) {
            return false;
        }
        let pending = inner.requests.remove(handle).expect("checked pending");
        let result = resolution_to_reply(pending.kind, pending.multiple, resolution);
        pending.cancel.send_replace(true);
        let _ = pending.reply.send(result);
        true
    }

    pub fn cancel_if_epoch(&self, handle: &str, epoch: u64) -> bool {
        let mut inner = self.inner.lock().expect("registry poisoned");
        if inner.requests.get(handle).map(|p| p.epoch) != Some(epoch) {
            return false;
        }
        let pending = inner.requests.remove(handle).expect("checked pending");
        pending.cancel.send_replace(true);
        let _ = pending.reply.send(PortalReply::cancelled());
        true
    }
}

pub struct Ticket {
    handle: String,
    epoch: u64,
    registry: Registry,
    receiver: oneshot::Receiver<PortalReply>,
    cancelled: watch::Receiver<bool>,
}

impl Ticket {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn cancellation(&self) -> watch::Receiver<bool> {
        self.cancelled.clone()
    }
    pub async fn wait(&mut self) -> PortalReply {
        (&mut self.receiver)
            .await
            .unwrap_or_else(|_| PortalReply::unavailable())
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        // In-flight D-Bus method was dropped: no leaked request, even if UI hangs.
        self.registry.cancel_if_epoch(&self.handle, self.epoch);
    }
}

fn resolution_to_reply(kind: PickerKind, multiple: bool, resolution: Resolution) -> PortalReply {
    let (paths, current_filter, choices) = match resolution {
        Resolution::Cancelled => return PortalReply::cancelled(),
        Resolution::Unavailable => return PortalReply::unavailable(),
        Resolution::Selected(paths) => (paths, None, None),
        Resolution::SelectedWithFilter { paths, filter } => (paths, Some(filter), None),
        Resolution::SelectedWithMetadata {
            paths,
            filter,
            choices,
        } => (paths, filter, Some(choices)),
    };
    let count_valid = match kind {
        PickerKind::OpenFile => !paths.is_empty() && (multiple || paths.len() == 1),
        PickerKind::SaveFile => paths.len() == 1,
        PickerKind::SaveFiles => !paths.is_empty() && current_filter.is_none(),
    };
    if !count_valid {
        return PortalReply::unavailable();
    }
    let uris: Option<Vec<String>> = paths
        .into_iter()
        .map(|path| {
            if !path.is_absolute() {
                return None;
            }
            url::Url::from_file_path(path).ok().map(Into::into)
        })
        .collect();
    match uris {
        Some(uris) => PortalReply {
            code: 0,
            uris,
            current_filter,
            choices,
        },
        None => PortalReply::unavailable(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(id: &str, kind: PickerKind) -> PickerInvocation {
        PickerInvocation {
            handle: id.into(),
            app_id: "org.gnosix.test".into(),
            parent_window: "".into(),
            title: "Test".into(),
            kind,
            options: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn out_of_order_parallel_calls_are_isolated() {
        let registry = Registry::new(8);
        let mut one = registry
            .admit(&call("/request/a", PickerKind::OpenFile))
            .unwrap();
        let mut two = registry
            .admit(&call("/request/b", PickerKind::SaveFile))
            .unwrap();
        let mut three = registry
            .admit(&call("/request/c", PickerKind::OpenFile))
            .unwrap();
        assert_eq!(registry.active(), 3);
        assert!(registry.complete(
            "/request/b",
            Resolution::Selected(vec!["/tmp/b.txt".into()])
        ));
        assert!(registry.cancel_if_epoch("/request/a", one.epoch()));
        assert!(registry.complete(
            "/request/c",
            Resolution::Selected(vec!["/tmp/c d.txt".into()])
        ));
        assert_eq!(two.wait().await.uris, vec!["file:///tmp/b.txt"]);
        assert_eq!(one.wait().await.code, 1);
        assert_eq!(three.wait().await.uris, vec!["file:///tmp/c%20d.txt"]);
        assert_eq!(registry.active(), 0);
    }

    #[tokio::test]
    async fn duplicate_capacity_and_replay_are_fail_closed() {
        let registry = Registry::new(1);
        let ticket = registry
            .admit(&call("/request/a", PickerKind::OpenFile))
            .unwrap();
        assert!(matches!(
            registry.admit(&call("/request/a", PickerKind::OpenFile)),
            Err(AdmitError::Duplicate)
        ));
        assert!(matches!(
            registry.admit(&call("/request/b", PickerKind::OpenFile)),
            Err(AdmitError::Capacity)
        ));
        let stale_epoch = ticket.epoch();
        drop(ticket);
        let mut next = registry
            .admit(&call("/request/a", PickerKind::SaveFile))
            .unwrap();
        assert!(!registry.cancel_if_epoch("/request/a", stale_epoch));
        assert!(registry.complete("/request/a", Resolution::Cancelled));
        assert!(!registry.complete("/request/a", Resolution::Cancelled));
        assert_eq!(next.wait().await.code, 1);
    }

    #[tokio::test]
    async fn invalid_paths_and_multiplicity_never_return_success() {
        let registry = Registry::new(3);
        let mut open = registry
            .admit(&call("/request/a", PickerKind::OpenFile))
            .unwrap();
        assert!(registry.complete(
            "/request/a",
            Resolution::Selected(vec!["/tmp/a".into(), "/tmp/b".into()])
        ));
        assert_eq!(open.wait().await.code, 2);
        let mut save = registry
            .admit(&call("/request/b", PickerKind::SaveFile))
            .unwrap();
        assert!(registry.complete(
            "/request/b",
            Resolution::Selected(vec!["relative.txt".into()])
        ));
        assert_eq!(save.wait().await.code, 2);
    }

    #[tokio::test]
    async fn preserves_utf8_and_validates_boolean_options() {
        let registry = Registry::new(2);
        let mut request = call("/request/a", PickerKind::OpenFile);
        request
            .options
            .insert("multiple".into(), OwnedValue::from(true));
        let mut ticket = registry.admit(&request).unwrap();
        assert!(registry.complete(
            "/request/a",
            Resolution::Selected(vec!["/tmp/été.png".into(), "/tmp/c b.png".into(),])
        ));
        let reply = ticket.wait().await;
        assert_eq!(reply.code, 0);
        assert_eq!(
            reply.uris,
            ["file:///tmp/%C3%A9t%C3%A9.png", "file:///tmp/c%20b.png"]
        );
        let mut invalid = call("/request/b", PickerKind::OpenFile);
        invalid
            .options
            .insert("multiple".into(), OwnedValue::from(14_u32));
        assert!(matches!(
            registry.admit(&invalid),
            Err(AdmitError::InvalidOptions)
        ));
    }

    #[tokio::test]
    async fn cancelled_worker_cannot_complete_reused_request() {
        let registry = Registry::new(2);
        let first = registry
            .admit(&call("/request/1", PickerKind::OpenFile))
            .unwrap();
        let epoch = first.epoch();
        let cancelled = first.cancellation();
        assert!(!*cancelled.borrow());
        assert!(registry.cancel_if_epoch("/request/1", epoch));
        assert!(*cancelled.borrow());
        drop(first);
        let mut second = registry
            .admit(&call("/request/1", PickerKind::OpenFile))
            .unwrap();
        assert!(!registry.complete_if_epoch("/request/1", epoch, Resolution::Unavailable));
        assert!(registry.complete_if_epoch("/request/1", second.epoch(), Resolution::Cancelled));
        assert_eq!(second.wait().await.code, 1);
    }

    #[tokio::test]
    async fn dropped_ticket_cleans_inflight_state() {
        let registry = Registry::new(2);
        let ticket = registry
            .admit(&call("/request/a", PickerKind::OpenFile))
            .unwrap();
        assert_eq!(registry.active(), 1);
        drop(ticket);
        assert_eq!(registry.active(), 0);
    }
}
