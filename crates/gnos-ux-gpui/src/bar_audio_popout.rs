//! UXE-01A: Bar-attached audio popout, headless.
//!
//! Content follows the pinned reference audio popout (output devices, input
//! devices, volume label, slider with wheel, settings entry). Two Gnosix
//! additions use authority that already exists: output and input mute toggles.
//! The interactive surface is a `blobin-ui-core` tree: the router owns hit
//! testing and activation, the gesture coordinator owns slider drags.
//!
//! Effects are typed. Volume and mute become `AudioIntent`s for the existing
//! AUDIO-01 loop (revalidated against the observed default endpoint). Choosing
//! another default device and opening settings have no authority yet and are
//! reported as `Pending`, never approximated. Content geometry is provisional;
//! pixel parity belongs to the visual phase.

use crate::bar_pointer_routing::StepDirection;
use crate::product_audio_action::{AudioIntent, ExactAudioEndpointRef, ProductVolume, VolumeDelta};
use blobin_ui_core::gestures::{CancelReason, GestureAction, GestureCoordinator};
use blobin_ui_core::input::{
    EventKind, InputEvent, InputReceipt, InputRouter, Key, Point, ScrollDelta, ScrollSource, Units,
};
use blobin_ui_core::layout::{
    self, Axis, Insets, LayoutSnapshot, LayoutStyles, Length, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};
use gnos_audio::{AudioEndpointId, AudioEndpointKind, AudioProviderState, AudioSnapshot};
use gnos_i18n::t;
use std::collections::BTreeMap;

/// Reference token `sizes.bar.audioWidth`.
pub const AUDIO_POPOUT_WIDTH: f32 = 320.0;
/// Reference `services.audioIncrement`.
pub const VOLUME_STEP: f32 = 0.1;
const PADDING: f32 = 12.0;
const GAP: f32 = 12.0;
const HEADER_HEIGHT: f32 = 20.0;
const ROW_HEIGHT: f32 = 32.0;
/// Reference slider row: `padding.medium * 3`.
const SLIDER_ROW_HEIGHT: f32 = 36.0;
const MUTE_SIZE: f32 = 36.0;
const POINTER: u64 = 1;
const PRIMARY: u8 = 1;

/// Vertical placement of a Bar popout: centred on its anchor, then kept inside
/// the panel area (reference `ClipWrapper.y`). `anchor_y` is in screen space;
/// the result is relative to the panel area, which starts after `border`.
pub fn popout_top(anchor_y: f32, height: f32, area_height: f32, border: f32) -> f32 {
    let offset = anchor_y - border - height / 2.0;
    let overflow = area_height - (offset + height).floor();
    if overflow < 0.0 {
        offset + overflow
    } else {
        offset.max(0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRow {
    pub id: AudioEndpointId,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndpointControl {
    pub target: ExactAudioEndpointRef,
    /// Cubic user level, as the mixer presents it.
    pub level: Option<f32>,
    pub muted: bool,
}

/// What the popout presents, derived only from an observed snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioPopoutModel {
    pub available: bool,
    pub outputs: Vec<DeviceRow>,
    pub inputs: Vec<DeviceRow>,
    pub output: Option<EndpointControl>,
    pub input: Option<EndpointControl>,
}

impl AudioPopoutModel {
    pub fn from_snapshot(snapshot: &AudioSnapshot) -> Self {
        let available = snapshot.provider == AudioProviderState::Available;
        let rows = |kind: AudioEndpointKind, default: Option<AudioEndpointId>| {
            snapshot
                .endpoints
                .values()
                .filter(|endpoint| endpoint.kind == kind)
                .map(|endpoint| DeviceRow {
                    id: endpoint.id,
                    label: endpoint.display_name.clone(),
                    selected: default == Some(endpoint.id),
                })
                .collect::<Vec<_>>()
        };
        let control = |kind: AudioEndpointKind, default: Option<AudioEndpointId>| {
            let endpoint = snapshot.endpoints.get(&default?)?;
            (endpoint.kind == kind).then(|| EndpointControl {
                target: ExactAudioEndpointRef {
                    id: endpoint.id,
                    kind,
                },
                level: endpoint.volume.cubic_user_level(),
                muted: endpoint.muted,
            })
        };
        let out_default = snapshot.defaults.output.effective.resolved;
        let in_default = snapshot.defaults.input.effective.resolved;
        if !available {
            return Self {
                available,
                outputs: Vec::new(),
                inputs: Vec::new(),
                output: None,
                input: None,
            };
        }
        Self {
            available,
            outputs: rows(AudioEndpointKind::Output, out_default),
            inputs: rows(AudioEndpointKind::Input, in_default),
            output: control(AudioEndpointKind::Output, out_default),
            input: control(AudioEndpointKind::Input, in_default),
        }
    }

    /// Reference label: `Volume (N%)`, or `Volume (muted)`.
    pub fn volume_label(&self) -> String {
        match self.output {
            None => t("Audio unavailable").to_owned(),
            Some(EndpointControl { muted: true, .. }) => t("Volume (muted)").to_owned(),
            Some(EndpointControl {
                level: Some(level), ..
            }) => gnos_i18n::tf(
                "Volume ({percent}%)",
                &[("percent", &((level * 100.0).round() as i64).to_string())],
            ),
            Some(_) => t("Volume").to_owned(),
        }
    }

    /// Setting the volume unmutes, as the reference `setVolume` does.
    fn set_volume(&self, level: f32) -> Vec<AudioPopoutEffect> {
        let Some(output) = self.output else {
            return Vec::new();
        };
        let Ok(level) = ProductVolume::new(level.clamp(0.0, 1.0)) else {
            return Vec::new();
        };
        let mut effects = vec![AudioPopoutEffect::Intent(AudioIntent::SetOutputVolume {
            target: output.target,
            level,
        })];
        if output.muted {
            effects.push(AudioPopoutEffect::Intent(AudioIntent::ToggleOutputMute {
                target: output.target,
            }));
        }
        effects
    }

    /// One reference step on the default output, unmuting like the reference.
    pub fn step(&self, direction: StepDirection) -> Vec<AudioIntent> {
        let Some(output) = self.output else {
            return Vec::new();
        };
        let signed = match direction {
            StepDirection::Increase => VOLUME_STEP,
            StepDirection::Decrease => -VOLUME_STEP,
        };
        let Ok(delta) = VolumeDelta::new(signed) else {
            return Vec::new();
        };
        let mut intents = vec![AudioIntent::AdjustOutputVolume {
            target: output.target,
            delta,
        }];
        if output.muted {
            intents.push(AudioIntent::ToggleOutputMute {
                target: output.target,
            });
        }
        intents
    }
}

/// A capability the reference offers that Gnosix has no authority for yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingCapability {
    OpenAudioSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioPopoutEffect {
    /// Presentation only: the slider shows this level while dragging.
    PreviewLevel(f32),
    ClearPreview,
    Intent(AudioIntent),
    Pending(PendingCapability),
}

/// One painted row, in popout-local logical pixels. Rendering reads these
/// rectangles from the same layout the router hit-tests.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioRow {
    pub kind: AudioRowKind,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioRowKind {
    Header,
    OutputDevice { selected: bool },
    InputDevice { selected: bool },
    VolumeLabel,
    Slider { level: Option<f32>, muted: bool },
    MuteOutput { muted: bool },
    MuteInput { muted: bool },
    OpenSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    OutputDevice(usize),
    InputDevice(usize),
    Slider,
    MuteOutput,
    MuteInput,
    OpenSettings,
}

#[derive(Debug)]
pub enum AudioPopoutError {
    Build(String),
    Input(String),
}

/// The interactive popout body. Coordinates are popout-local logical pixels.
pub struct AudioPopoutSurface {
    model: AudioPopoutModel,
    tree: UiTree,
    styles: LayoutStyles,
    layout: LayoutSnapshot,
    viewport: Viewport,
    router: InputRouter,
    gestures: GestureCoordinator,
    controls: BTreeMap<NodeId, Control>,
    painted: Vec<(NodeId, AudioRowKind)>,
    height: f32,
    serial: u64,
    drag_x: Option<f32>,
}

impl AudioPopoutSurface {
    pub fn new(model: AudioPopoutModel) -> Result<Self, AudioPopoutError> {
        let build = |e: &dyn std::fmt::Debug| AudioPopoutError::Build(format!("{e:?}"));
        let surface = SurfaceId(2);
        let mut tree = UiTree::new();
        let mut placed: Vec<(NodeId, Style)> = Vec::new();
        let mut controls = BTreeMap::new();
        let mut painted: Vec<(NodeId, AudioRowKind)> = Vec::new();
        let output_muted = model.output.is_some_and(|o| o.muted);
        let input_muted = model.input.is_some_and(|i| i.muted);
        let fixed = |height: f32| Style {
            height: Length::Px(height),
            ..Style::default()
        };
        let row = |height: f32| Style {
            height: Length::Px(height),
            axis: Axis::Row,
            ..Style::default()
        };
        let grow = Style {
            flex_grow: 1.0,
            ..Style::default()
        };
        let square = |size: f32| Style {
            width: Length::Px(size),
            height: Length::Px(size),
            ..Style::default()
        };
        let mut height = 2.0 * PADDING;
        let mut rows = 0usize;
        let mut add_height = |h: f32, rows: &mut usize| {
            height += h;
            *rows += 1;
        };
        tree.transact(|tx| {
            tx.open_surface(surface)?;
            let root = tx.append_root(surface, NodeProperties::new(Role::Container, "audio"))?;
            placed.push((
                root,
                Style {
                    padding: Insets::uniform(PADDING),
                    gap: GAP,
                    ..Style::fill()
                },
            ));
            let text = |tx: &mut blobin_ui_core::UiTransaction<'_>, parent, label: &str| {
                tx.append_child(parent, NodeProperties::new(Role::Text, label))
            };
            let header = text(tx, root, t("Output device"))?;
            placed.push((header, fixed(HEADER_HEIGHT)));
            painted.push((header, AudioRowKind::Header));
            add_height(HEADER_HEIGHT, &mut rows);
            for (index, device) in model.outputs.iter().enumerate() {
                let id = tx.append_child(
                    root,
                    NodeProperties::new(Role::Button, device.label.clone()),
                )?;
                placed.push((id, fixed(ROW_HEIGHT)));
                controls.insert(id, Control::OutputDevice(index));
                painted.push((
                    id,
                    AudioRowKind::OutputDevice {
                        selected: device.selected,
                    },
                ));
                add_height(ROW_HEIGHT, &mut rows);
            }
            let input_header =
                tx.append_child(root, NodeProperties::new(Role::Container, "input-header"))?;
            placed.push((input_header, row(HEADER_HEIGHT)));
            add_height(HEADER_HEIGHT, &mut rows);
            let input_label = text(tx, input_header, t("Input device"))?;
            placed.push((input_label, grow.clone()));
            painted.push((input_label, AudioRowKind::Header));
            if model.input.is_some() {
                let mic = tx.append_child(
                    input_header,
                    NodeProperties::new(Role::Button, t("Mute input")),
                )?;
                placed.push((mic, square(HEADER_HEIGHT)));
                controls.insert(mic, Control::MuteInput);
                painted.push((mic, AudioRowKind::MuteInput { muted: input_muted }));
            }
            for (index, device) in model.inputs.iter().enumerate() {
                let id = tx.append_child(
                    root,
                    NodeProperties::new(Role::Button, device.label.clone()),
                )?;
                placed.push((id, fixed(ROW_HEIGHT)));
                controls.insert(id, Control::InputDevice(index));
                painted.push((
                    id,
                    AudioRowKind::InputDevice {
                        selected: device.selected,
                    },
                ));
                add_height(ROW_HEIGHT, &mut rows);
            }
            let label = text(tx, root, &model.volume_label())?;
            placed.push((label, fixed(HEADER_HEIGHT)));
            painted.push((label, AudioRowKind::VolumeLabel));
            add_height(HEADER_HEIGHT, &mut rows);
            let slider_row =
                tx.append_child(root, NodeProperties::new(Role::Container, "volume"))?;
            placed.push((slider_row, row(SLIDER_ROW_HEIGHT)));
            add_height(SLIDER_ROW_HEIGHT, &mut rows);
            let slider = tx.append_child(
                slider_row,
                NodeProperties::new(Role::Custom("slider".into()), t("Output volume")),
            )?;
            placed.push((slider, grow.clone()));
            controls.insert(slider, Control::Slider);
            painted.push((
                slider,
                AudioRowKind::Slider {
                    level: model.output.and_then(|o| o.level),
                    muted: output_muted,
                },
            ));
            let mute = tx.append_child(
                slider_row,
                NodeProperties::new(Role::Button, t("Mute output")),
            )?;
            placed.push((mute, square(MUTE_SIZE)));
            controls.insert(mute, Control::MuteOutput);
            painted.push((
                mute,
                AudioRowKind::MuteOutput {
                    muted: output_muted,
                },
            ));
            let settings =
                tx.append_child(root, NodeProperties::new(Role::Button, t("Open settings")))?;
            placed.push((settings, fixed(ROW_HEIGHT)));
            controls.insert(settings, Control::OpenSettings);
            painted.push((settings, AudioRowKind::OpenSettings));
            add_height(ROW_HEIGHT, &mut rows);
            Ok(())
        })
        .map_err(|e| build(&e))?;
        let height = height + GAP * rows.saturating_sub(1) as f32;
        let mut styles = LayoutStyles::new();
        let snapshot = tree.snapshot();
        for (id, style) in placed {
            styles.set(&snapshot, id, style).map_err(|e| build(&e))?;
        }
        let viewport = Viewport::new(Size::new(AUDIO_POPOUT_WIDTH, height), 1.0, 1);
        let layout =
            layout::compute(&snapshot, &styles, surface, viewport).map_err(|e| build(&e))?;
        Ok(Self {
            model,
            tree,
            styles,
            layout,
            viewport,
            router: InputRouter::new(),
            gestures: GestureCoordinator::new(),
            controls,
            painted,
            height,
            serial: 0,
            drag_x: None,
        })
    }

    pub fn model(&self) -> &AudioPopoutModel {
        &self.model
    }

    pub fn rows(&self) -> Vec<AudioRow> {
        let snapshot = self.tree.snapshot();
        self.painted
            .iter()
            .filter_map(|(id, kind)| {
                let rect = self.layout.get(*id)?.rect;
                Some(AudioRow {
                    kind: *kind,
                    label: snapshot.get(*id)?.properties().label.clone(),
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                })
            })
            .collect()
    }

    /// Natural content height; the popout sizes itself to it.
    pub fn height(&self) -> f32 {
        self.height
    }

    fn rect_of(&self, control: Control) -> Option<blobin_ui_core::layout::Rect> {
        let (id, _) = self.controls.iter().find(|(_, c)| **c == control)?;
        Some(self.layout.get(*id)?.rect)
    }

    fn level_at(&self, x: f32) -> Option<f32> {
        let rect = self.rect_of(Control::Slider)?;
        (rect.width > 0.0).then(|| ((x - rect.x) / rect.width).clamp(0.0, 1.0))
    }

    fn dispatch(
        &mut self,
        timestamp_micros: u64,
        kind: EventKind,
    ) -> Result<(InputEvent, InputReceipt), AudioPopoutError> {
        self.serial += 1;
        let event = InputEvent::for_layout(&self.layout, self.serial, timestamp_micros, kind);
        let receipt = self
            .router
            .dispatch(
                &self.tree.snapshot(),
                &self.styles,
                &self.layout,
                self.viewport,
                event.clone(),
            )
            .map_err(|e| AudioPopoutError::Input(format!("{e:?}")))?;
        Ok((event, receipt))
    }

    fn pointer(
        &mut self,
        timestamp_micros: u64,
        kind: EventKind,
    ) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        let (event, receipt) = self.dispatch(timestamp_micros, kind)?;
        let actions = self
            .gestures
            .observe(
                &self.tree.snapshot(),
                &self.styles,
                &self.layout,
                self.viewport,
                &event,
                &receipt,
            )
            .map_err(|e| AudioPopoutError::Input(format!("{e:?}")))?;
        let mut effects = Vec::new();
        for action in actions {
            if self.controls.get(&action.target()) != Some(&Control::Slider) {
                continue;
            }
            effects.extend(self.slider_action(action));
        }
        if let Some(activated) = receipt.activated {
            effects.extend(self.activate(activated));
        }
        Ok(effects)
    }

    fn slider_action(&self, action: GestureAction) -> Vec<AudioPopoutEffect> {
        match action {
            GestureAction::Begin { origin, .. } => self
                .level_at(origin.x)
                .map(AudioPopoutEffect::PreviewLevel)
                .into_iter()
                .collect(),
            GestureAction::Preview { delta, .. } => self
                .drag_origin()
                .and_then(|x| self.level_at(x + delta.x))
                .map(AudioPopoutEffect::PreviewLevel)
                .into_iter()
                .collect(),
            GestureAction::Commit { delta, .. } => {
                self.commit_at(self.drag_origin().map(|x| x + delta.x))
            }
            // A press without travel is a click: the slider jumps there.
            GestureAction::Cancel {
                reason: CancelReason::NoTravel,
                ..
            } => self.commit_at(self.drag_origin()),
            GestureAction::Cancel { .. } => vec![AudioPopoutEffect::ClearPreview],
        }
    }

    fn commit_at(&self, x: Option<f32>) -> Vec<AudioPopoutEffect> {
        let mut effects = vec![AudioPopoutEffect::ClearPreview];
        if let Some(level) = x.and_then(|x| self.level_at(x)) {
            effects.extend(self.model.set_volume(level));
        }
        effects
    }

    fn activate(&self, node: NodeId) -> Vec<AudioPopoutEffect> {
        let intent = |intent| vec![AudioPopoutEffect::Intent(intent)];
        match self.controls.get(&node) {
            Some(Control::MuteOutput) => self.model.output.map_or_else(Vec::new, |o| {
                intent(AudioIntent::ToggleOutputMute { target: o.target })
            }),
            Some(Control::MuteInput) => self.model.input.map_or_else(Vec::new, |i| {
                intent(AudioIntent::ToggleInputMute { target: i.target })
            }),
            Some(Control::OutputDevice(index)) => self
                .model
                .outputs
                .get(*index)
                .filter(|row| !row.selected)
                .map(|row| {
                    AudioPopoutEffect::Intent(AudioIntent::SelectDefault {
                        target: ExactAudioEndpointRef::output(row.id),
                    })
                })
                .into_iter()
                .collect(),
            Some(Control::InputDevice(index)) => self
                .model
                .inputs
                .get(*index)
                .filter(|row| !row.selected)
                .map(|row| {
                    AudioPopoutEffect::Intent(AudioIntent::SelectDefault {
                        target: ExactAudioEndpointRef::input(row.id),
                    })
                })
                .into_iter()
                .collect(),
            Some(Control::OpenSettings) => {
                vec![AudioPopoutEffect::Pending(
                    PendingCapability::OpenAudioSettings,
                )]
            }
            Some(Control::Slider) | None => Vec::new(),
        }
    }

    /// The x where the active slider drag started, kept by the host because
    /// gesture deltas are relative to it.
    fn drag_origin(&self) -> Option<f32> {
        self.drag_x
    }

    pub fn pointer_down(
        &mut self,
        x: f32,
        y: f32,
        t: u64,
    ) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        self.drag_x = Some(x);
        self.pointer(
            t,
            EventKind::PointerDown {
                pointer: POINTER,
                button: PRIMARY,
                position: Point::new(x, y),
                units: Units::Logical,
            },
        )
    }

    pub fn pointer_move(
        &mut self,
        x: f32,
        y: f32,
        t: u64,
    ) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        self.pointer(
            t,
            EventKind::PointerMove {
                pointer: POINTER,
                position: Point::new(x, y),
                units: Units::Logical,
            },
        )
    }

    pub fn pointer_up(
        &mut self,
        x: f32,
        y: f32,
        t: u64,
    ) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        let effects = self.pointer(
            t,
            EventKind::PointerUp {
                pointer: POINTER,
                button: PRIMARY,
                position: Point::new(x, y),
                units: Units::Logical,
            },
        );
        self.drag_x = None;
        effects
    }

    /// Escape abandons an in-progress drag without changing the volume.
    pub fn escape(&mut self, t: u64) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        let effects = self.pointer(
            t,
            EventKind::KeyDown {
                key: Key::Escape,
                shift: false,
                repeat: false,
            },
        );
        self.drag_x = None;
        effects
    }

    /// Wheel over the slider row steps the default output (Wayland axis sign).
    pub fn wheel(
        &mut self,
        x: f32,
        y: f32,
        delta_y: f32,
        t: u64,
    ) -> Result<Vec<AudioPopoutEffect>, AudioPopoutError> {
        let (_, receipt) = self.dispatch(
            t,
            EventKind::Scroll {
                pointer: POINTER,
                position: Point::new(x, y),
                units: Units::Logical,
                delta: ScrollDelta { x: 0.0, y: delta_y },
                source: ScrollSource::Wheel,
            },
        )?;
        let over_slider_row = receipt
            .route
            .as_ref()
            .and_then(|route| self.controls.get(&route.target))
            .is_some_and(|c| matches!(c, Control::Slider | Control::MuteOutput));
        if !over_slider_row || delta_y == 0.0 {
            return Ok(Vec::new());
        }
        let direction = if delta_y < 0.0 {
            StepDirection::Increase
        } else {
            StepDirection::Decrease
        };
        Ok(self
            .model
            .step(direction)
            .into_iter()
            .map(AudioPopoutEffect::Intent)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnos_audio::{
        AudioDefaultRoles, AudioEndpointSnapshot, AudioVolume, NativeVolume, RoleSelection,
    };

    const COOKIE: u32 = 7;

    fn endpoint(
        serial: u64,
        kind: AudioEndpointKind,
        name: &str,
        linear: f32,
        muted: bool,
    ) -> AudioEndpointSnapshot {
        AudioEndpointSnapshot::new(
            AudioEndpointId::new(COOKIE, serial),
            kind,
            format!("node.{serial}"),
            name.into(),
            AudioVolume::from_parts(
                None,
                vec![NativeVolume::new(linear).unwrap(); 2],
                Vec::new(),
            ),
            muted,
        )
    }

    /// Two outputs (default = 1), one input (default = 3). Output at user
    /// level 0.5 (linear 0.125).
    fn snapshot(muted: bool) -> AudioSnapshot {
        let mut endpoints = BTreeMap::new();
        for e in [
            endpoint(1, AudioEndpointKind::Output, "Speakers", 0.125, muted),
            endpoint(2, AudioEndpointKind::Output, "Headphones", 1.0, false),
            endpoint(3, AudioEndpointKind::Input, "Microphone", 1.0, false),
        ] {
            endpoints.insert(e.id, e);
        }
        let mut defaults = AudioDefaultRoles::default();
        let role = |serial: u64| RoleSelection {
            name: Some(format!("node.{serial}")),
            resolved: Some(AudioEndpointId::new(COOKIE, serial)),
            metadata_error: None,
        };
        defaults.output.effective = role(1);
        defaults.input.effective = role(3);
        AudioSnapshot {
            provider: AudioProviderState::Available,
            generation: 1,
            endpoints,
            defaults,
        }
    }

    fn surface(muted: bool) -> AudioPopoutSurface {
        AudioPopoutSurface::new(AudioPopoutModel::from_snapshot(&snapshot(muted))).unwrap()
    }

    fn centre(s: &AudioPopoutSurface, c: Control) -> (f32, f32) {
        let r = s.rect_of(c).unwrap();
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    fn set_level(effects: &[AudioPopoutEffect]) -> Option<f32> {
        effects.iter().find_map(|e| match e {
            AudioPopoutEffect::Intent(AudioIntent::SetOutputVolume { level, .. }) => {
                Some(level.get())
            }
            _ => None,
        })
    }

    fn output() -> ExactAudioEndpointRef {
        ExactAudioEndpointRef::output(AudioEndpointId::new(COOKIE, 1))
    }

    #[test]
    fn model_reflects_observed_defaults_and_reference_label() {
        let model = AudioPopoutModel::from_snapshot(&snapshot(false));
        assert_eq!(model.outputs.len(), 2);
        assert!(model.outputs[0].selected && !model.outputs[1].selected);
        assert_eq!(model.inputs.len(), 1);
        assert_eq!(model.volume_label(), "Volume (50%)");
        assert_eq!(
            AudioPopoutModel::from_snapshot(&snapshot(true)).volume_label(),
            "Volume (muted)"
        );
        let mut gone = snapshot(false);
        gone.provider = AudioProviderState::Unavailable;
        let model = AudioPopoutModel::from_snapshot(&gone);
        assert!(model.outputs.is_empty() && model.output.is_none());
        assert_eq!(model.volume_label(), "Audio unavailable");
    }

    #[test]
    fn slider_drag_previews_then_commits_one_volume() {
        let mut s = surface(false);
        let r = s.rect_of(Control::Slider).unwrap();
        let y = r.y + r.height / 2.0;
        let start = r.x + r.width * 0.25;
        let down = s.pointer_down(start, y, 1).unwrap();
        assert_eq!(down, vec![AudioPopoutEffect::PreviewLevel(0.25)]);
        let moved = s.pointer_move(r.x + r.width * 0.75, y, 2).unwrap();
        assert_eq!(moved, vec![AudioPopoutEffect::PreviewLevel(0.75)]);
        let up = s.pointer_up(r.x + r.width * 0.75, y, 3).unwrap();
        assert_eq!(up[0], AudioPopoutEffect::ClearPreview);
        assert_eq!(set_level(&up), Some(0.75));
        assert_eq!(up.len(), 2, "one volume intent, no unmute when not muted");
    }

    #[test]
    fn slider_click_sets_level_at_pointer_and_drag_clamps() {
        let mut s = surface(false);
        let r = s.rect_of(Control::Slider).unwrap();
        let y = r.y + r.height / 2.0;
        s.pointer_down(r.x + r.width * 0.4, y, 1).unwrap();
        assert_eq!(
            set_level(&s.pointer_up(r.x + r.width * 0.4, y, 2).unwrap()),
            Some(0.4)
        );
        s.pointer_down(r.x + r.width * 0.9, y, 3).unwrap();
        // Captured drag far beyond the track clamps to full volume.
        s.pointer_move(r.x + r.width * 3.0, y, 4).unwrap();
        assert_eq!(
            set_level(&s.pointer_up(r.x + r.width * 3.0, y, 5).unwrap()),
            Some(1.0)
        );
    }

    #[test]
    fn escape_abandons_a_drag_without_intent() {
        let mut s = surface(false);
        let r = s.rect_of(Control::Slider).unwrap();
        let y = r.y + r.height / 2.0;
        s.pointer_down(r.x + 10.0, y, 1).unwrap();
        s.pointer_move(r.x + 60.0, y, 2).unwrap();
        assert_eq!(s.escape(3).unwrap(), vec![AudioPopoutEffect::ClearPreview]);
        assert!(set_level(&s.pointer_up(r.x + 60.0, y, 4).unwrap()).is_none());
    }

    #[test]
    fn setting_volume_while_muted_also_unmutes() {
        let mut s = surface(true);
        let (x, y) = centre(&s, Control::Slider);
        s.pointer_down(x, y, 1).unwrap();
        let effects = s.pointer_up(x, y, 2).unwrap();
        assert!(set_level(&effects).is_some());
        assert_eq!(
            effects.last(),
            Some(&AudioPopoutEffect::Intent(AudioIntent::ToggleOutputMute {
                target: output()
            }))
        );
    }

    #[test]
    fn wheel_on_slider_row_steps_and_elsewhere_does_nothing() {
        let mut s = surface(false);
        let (x, y) = centre(&s, Control::Slider);
        let up = s.wheel(x, y, -15.0, 1).unwrap();
        assert!(matches!(
            up.as_slice(),
            [AudioPopoutEffect::Intent(AudioIntent::AdjustOutputVolume { delta, .. })] if delta.get() == VOLUME_STEP
        ));
        let down = s.wheel(x, y, 15.0, 2).unwrap();
        assert!(matches!(
            down.as_slice(),
            [AudioPopoutEffect::Intent(AudioIntent::AdjustOutputVolume { delta, .. })] if delta.get() == -VOLUME_STEP
        ));
        let (hx, hy) = centre(&s, Control::OpenSettings);
        assert!(s.wheel(hx, hy, -15.0, 3).unwrap().is_empty());
        assert!(s.wheel(x, y, 0.0, 4).unwrap().is_empty());
    }

    #[test]
    fn mute_buttons_use_existing_toggles() {
        let mut s = surface(false);
        let (x, y) = centre(&s, Control::MuteOutput);
        s.pointer_down(x, y, 1).unwrap();
        assert_eq!(
            s.pointer_up(x, y, 2).unwrap(),
            vec![AudioPopoutEffect::Intent(AudioIntent::ToggleOutputMute {
                target: output()
            })]
        );
        let (x, y) = centre(&s, Control::MuteInput);
        s.pointer_down(x, y, 3).unwrap();
        assert_eq!(
            s.pointer_up(x, y, 4).unwrap(),
            vec![AudioPopoutEffect::Intent(AudioIntent::ToggleInputMute {
                target: ExactAudioEndpointRef::input(AudioEndpointId::new(COOKIE, 3))
            })]
        );
    }

    #[test]
    fn a_device_row_selects_that_device_as_the_default() {
        let mut s = surface(false);
        let (x, y) = centre(&s, Control::OutputDevice(1));
        s.pointer_down(x, y, 1).unwrap();
        assert_eq!(
            s.pointer_up(x, y, 2).unwrap(),
            vec![AudioPopoutEffect::Intent(AudioIntent::SelectDefault {
                target: ExactAudioEndpointRef::output(AudioEndpointId::new(COOKIE, 2))
            })]
        );
        let (x, y) = centre(&s, Control::OutputDevice(0));
        s.pointer_down(x, y, 3).unwrap();
        assert!(
            s.pointer_up(x, y, 4).unwrap().is_empty(),
            "current default is a no-op"
        );
        let (x, y) = centre(&s, Control::OpenSettings);
        s.pointer_down(x, y, 5).unwrap();
        assert_eq!(
            s.pointer_up(x, y, 6).unwrap(),
            vec![AudioPopoutEffect::Pending(
                PendingCapability::OpenAudioSettings
            )]
        );
    }

    #[test]
    fn bar_wheel_step_unmutes_like_the_reference() {
        let muted = AudioPopoutModel::from_snapshot(&snapshot(true));
        let intents = muted.step(StepDirection::Decrease);
        assert_eq!(intents.len(), 2);
        assert!(
            matches!(intents[0], AudioIntent::AdjustOutputVolume { delta, .. } if delta.get() == -VOLUME_STEP)
        );
        assert_eq!(
            intents[1],
            AudioIntent::ToggleOutputMute { target: output() }
        );
        assert_eq!(
            AudioPopoutModel::from_snapshot(&snapshot(false))
                .step(StepDirection::Increase)
                .len(),
            1
        );
    }

    #[test]
    fn placement_centres_on_anchor_and_stays_inside_the_area() {
        assert_eq!(popout_top(500.0, 200.0, 1420.0, 10.0), 390.0);
        assert_eq!(popout_top(40.0, 200.0, 1420.0, 10.0), 0.0);
        assert_eq!(popout_top(1430.0, 200.0, 1420.0, 10.0), 1220.0);
    }

    #[test]
    fn painted_rows_share_the_hit_test_geometry() {
        let s = surface(true);
        let rows = s.rows();
        let slider = rows
            .iter()
            .find(|r| matches!(r.kind, AudioRowKind::Slider { .. }))
            .unwrap();
        let hit = s.rect_of(Control::Slider).unwrap();
        assert_eq!(
            (slider.x, slider.y, slider.width, slider.height),
            (hit.x, hit.y, hit.width, hit.height)
        );
        assert!(
            matches!(slider.kind, AudioRowKind::Slider { level: Some(l), muted: true } if (l - 0.5).abs() < 1e-6)
        );
        assert_eq!(
            rows.iter()
                .filter(|r| matches!(r.kind, AudioRowKind::OutputDevice { .. }))
                .count(),
            2
        );
        assert!(
            rows.iter()
                .any(|r| r.kind == AudioRowKind::VolumeLabel && r.label == "Volume (muted)")
        );
    }

    #[test]
    fn content_height_matches_laid_out_rows() {
        let s = surface(false);
        let (_, settings_y) = centre(&s, Control::OpenSettings);
        assert!((settings_y + ROW_HEIGHT / 2.0 + PADDING - s.height()).abs() < 0.01);
    }
}
