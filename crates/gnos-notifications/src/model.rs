//! Typed notification vocabulary (Desktop Notifications Specification 1.2).
//!
//! Everything a client sends is validated into these types before it reaches
//! the store. Malformed optional facts are dropped individually; they never
//! reject the notification and are never guessed.

use std::sync::Arc;

/// Server-assigned identity. Never zero: zero means "no replacement" on the
/// wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NotificationId(u32);

impl NotificationId {
    pub fn new(raw: u32) -> Option<Self> {
        (raw != 0).then_some(Self(raw))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    /// Spec byte values 0/1/2; anything else is not an urgency.
    pub fn from_wire(raw: i64) -> Option<Self> {
        match raw {
            0 => Some(Self::Low),
            1 => Some(Self::Normal),
            2 => Some(Self::Critical),
            _ => None,
        }
    }
}

/// `NotificationClosed` reason codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    Closed = 3,
    Undefined = 4,
}

impl CloseReason {
    pub fn wire(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Actions arrive as a flat `[key, label, key, label, ...]` list. A trailing
/// unpaired key and duplicate keys (first wins) are dropped.
pub fn pair_actions(flat: &[String]) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    for pair in flat.chunks_exact(2) {
        if pair[0].is_empty() || actions.iter().any(|a| a.key == pair[0]) {
            continue;
        }
        actions.push(Action {
            key: pair[0].clone(),
            label: pair[1].clone(),
        });
    }
    actions
}

/// Maximum accepted `image-data` payload; larger images are dropped.
pub const IMAGE_DATA_LIMIT: usize = 4 * 1024 * 1024;

/// Validated `image-data` (`(iiibiiay)`): 8-bit RGB or RGBA rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rowstride: u32,
    pub has_alpha: bool,
    pub data: Arc<[u8]>,
}

impl RawImage {
    pub fn new(
        width: i32,
        height: i32,
        rowstride: i32,
        has_alpha: bool,
        bits_per_sample: i32,
        channels: i32,
        data: Vec<u8>,
    ) -> Option<Self> {
        let expected_channels = if has_alpha { 4 } else { 3 };
        if bits_per_sample != 8 || channels != expected_channels {
            return None;
        }
        let (width, height, rowstride) = (
            u32::try_from(width).ok()?,
            u32::try_from(height).ok()?,
            u32::try_from(rowstride).ok()?,
        );
        if width == 0 || height == 0 || rowstride < width.checked_mul(channels as u32)? {
            return None;
        }
        // The last row needs only its pixels, not a full stride.
        let needed = (rowstride as usize)
            .checked_mul(height as usize - 1)?
            .checked_add(width as usize * channels as usize)?;
        if data.len() < needed || data.len() > IMAGE_DATA_LIMIT {
            return None;
        }
        Some(Self {
            width,
            height,
            rowstride,
            has_alpha,
            data: data.into(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    /// `image-path` hint: an icon name, absolute path or `file://` URI.
    Path(String),
    Data(RawImage),
}

/// The hints this server understands. Unknown hints are ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    pub urgency: Option<Urgency>,
    pub category: Option<String>,
    pub desktop_entry: Option<String>,
    pub resident: bool,
    pub transient: bool,
    pub image: Option<ImageSource>,
    /// `value` hint, 0..=100 (progress).
    pub progress: Option<u8>,
}

impl Hints {
    pub fn progress_from_wire(raw: i64) -> Option<u8> {
        u8::try_from(raw).ok().filter(|value| *value <= 100)
    }
}

/// `expire_timeout` as sent: -1 server default, 0 never, otherwise ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpireTimeout {
    ServerDefault,
    Never,
    Millis(u32),
}

impl ExpireTimeout {
    pub fn from_wire(raw: i32) -> Self {
        match raw {
            0 => Self::Never,
            ms if ms > 0 => Self::Millis(ms as u32),
            _ => Self::ServerDefault,
        }
    }
}

/// One validated `Notify` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyRequest {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub hints: Hints,
    pub expire_timeout: ExpireTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn actions_pair_drop_unpaired_and_duplicates() {
        let actions = pair_actions(&strings(&[
            "default", "Open", "reply", "Reply", "default", "X", "orphan",
        ]));
        assert_eq!(
            actions,
            vec![
                Action {
                    key: "default".into(),
                    label: "Open".into()
                },
                Action {
                    key: "reply".into(),
                    label: "Reply".into()
                },
            ]
        );
    }

    #[test]
    fn wire_values() {
        assert_eq!(NotificationId::new(0), None);
        assert_eq!(Urgency::from_wire(2), Some(Urgency::Critical));
        assert_eq!(Urgency::from_wire(3), None);
        assert_eq!(ExpireTimeout::from_wire(-1), ExpireTimeout::ServerDefault);
        assert_eq!(ExpireTimeout::from_wire(-7), ExpireTimeout::ServerDefault);
        assert_eq!(ExpireTimeout::from_wire(0), ExpireTimeout::Never);
        assert_eq!(ExpireTimeout::from_wire(1500), ExpireTimeout::Millis(1500));
        assert_eq!(Hints::progress_from_wire(100), Some(100));
        assert_eq!(Hints::progress_from_wire(101), None);
        assert_eq!(Hints::progress_from_wire(-1), None);
    }

    #[test]
    fn raw_image_is_validated() {
        let rgba = vec![0u8; 2 * 2 * 4];
        assert!(RawImage::new(2, 2, 8, true, 8, 4, rgba.clone()).is_some());
        // Short buffer, wrong channels, wrong depth, zero size, short stride.
        assert!(RawImage::new(2, 2, 8, true, 8, 4, vec![0; 15]).is_none());
        assert!(RawImage::new(2, 2, 8, true, 8, 3, rgba.clone()).is_none());
        assert!(RawImage::new(2, 2, 8, true, 16, 4, rgba.clone()).is_none());
        assert!(RawImage::new(0, 2, 8, true, 8, 4, rgba.clone()).is_none());
        assert!(RawImage::new(2, 2, 7, true, 8, 4, rgba).is_none());
        // A padded stride only needs the last row's pixels.
        assert!(RawImage::new(2, 2, 12, false, 8, 3, vec![0; 12 + 6]).is_some());
        assert!(RawImage::new(1, 1, 3, false, 8, 3, vec![0; IMAGE_DATA_LIMIT + 1]).is_none());
    }
}
