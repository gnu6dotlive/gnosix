//! SURF-04A: the desktop layer.
//!
//! Each scene host opens one companion surface on its output's background
//! layer: under every window, over the wallpaper. Its input region is the
//! modules only (the whole output while one is dragged), so the desktop
//! beneath stays the compositor's. The scene owns what it shows through a
//! shared `DesktopModel`; a drop is emitted as `DesktopEvent::Placed` for the
//! scene to keep. When the scene host goes, it marks the model closed and the
//! surface removes itself.

use crate::desktop_modules::{DesktopModules, ModuleKind, ModulePlace};
use crate::notification_view::line;
use crate::reference_bar::{COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_PRIMARY};
use gpui::prelude::*;
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, Window, div, point, px, rgba, size,
};

pub const DESKTOP_NAMESPACE: &str = "gnosix-desktop";

pub enum DesktopEvent {
    Placed(ModuleKind, ModulePlace),
}

/// What the dictation element shows.
///
/// The state is published by an application; the element never reads the
/// microphone itself, and shows nothing when nobody publishes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DictationView {
    /// An application is recording or transcribing.
    pub active: bool,
    /// It is recording, as opposed to transcribing.
    pub recording: bool,
    /// Seconds spent in the current state.
    pub elapsed_seconds: u32,
    /// The text the model has committed to.
    pub text: String,
    /// The model's current hypothesis about the words that follow.
    pub tentative: String,
}

/// The mic patch's size (DICTATION-01).
pub const DICTATION_PATCH_SIZE: f32 = 32.0;
/// The dictation surface's width (DICTATION-01).
pub const DICTATION_SURFACE_WIDTH: f32 = 420.0;
/// The surface's inner padding, on each side (DICTATION-01).
pub const DICTATION_SURFACE_PADDING: f32 = 12.0;
/// The clear space between the mic patch and the surface above it.
pub const DICTATION_SURFACE_GAP: f32 = 8.0;
/// The surface's header: the state, and the time in it.
pub const DICTATION_HEADER_HEIGHT: f32 = 28.0;
/// The surface's padding and the gap under its header.
pub const DICTATION_SURFACE_CHROME: f32 = 20.0;
/// The width of a character of the body style, estimated.
pub const DICTATION_CHAR_WIDTH: f32 = 7.2;
/// The deck's fill: the dictation surface is the same material as the deck.
const DICTATION_FILL: u32 = 0x2638_44ff;

#[derive(Default)]
pub struct DesktopModel {
    pub modules: DesktopModules,
    pub time: Option<String>,
    pub date: Option<String>,
    /// The visualiser's bands, 0..=1; empty while silent.
    pub bands: Vec<f32>,
    /// The scene host is gone: the surface removes itself.
    pub closed: bool,
    /// SET-02A: the operator turned the desktop clock off.
    pub clock_hidden: bool,
    /// What the dictation element shows; `None` when nothing publishes.
    pub dictation: Option<DictationView>,
    /// WALL-01: the wallpaper, prepared at this output's size.
    pub wallpaper: Option<std::path::PathBuf>,
    /// The output's size in device pixels, as last rendered.
    pub device_size: (u32, u32),
    /// What the shell reserves along each edge (top, right, bottom, left):
    /// the surface spans the whole output so the wallpaper reaches under the
    /// Bar, and the modules keep to the rest.
    pub inset: [f32; 4],
    reported: Option<String>,
}

impl EventEmitter<DesktopEvent> for DesktopModel {}

impl DesktopModel {
    fn visualiser_shown(&self) -> bool {
        !self.bands.is_empty() || self.modules.dragging() == Some(ModuleKind::Visualiser)
    }

    /// `GNOS_UX_DESKTOP`: the modules and what they show, on change.
    ///
    /// The visualiser's peak changes on every frame while audio plays, so it
    /// is reported only when traced (`GNOS_UX_DESKTOP_TRACE=1`, the SURF-04
    /// harness); otherwise a deployed session would write a receipt per frame
    /// to the journal.
    fn report(&mut self) {
        static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let trace =
            *TRACE.get_or_init(|| std::env::var("GNOS_UX_DESKTOP_TRACE").as_deref() == Ok("1"));
        // In the surface's coordinates (the modules' own are inside the inset).
        let rect = |k| {
            let (x, y, w, h) = self.modules.rect(k);
            let (x, y) = (x + self.inset[3], y + self.inset[0]);
            format!("[{x:.1},{y:.1},{w:.1},{h:.1}]")
        };
        let peak = if trace {
            self.bands
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or("none".to_owned(), |(i, v)| format!("{i}:{v:.2}"))
        } else {
            "untraced".to_owned()
        };
        let line = format!(
            "GNOS_UX_DESKTOP viewport={:.0}x{:.0} clock={} time={} date={} visualiser={} shown={} bands={} peak={peak} dragging={} dictation={}",
            self.modules.viewport().0,
            self.modules.viewport().1,
            rect(ModuleKind::Clock),
            self.time.as_deref().unwrap_or("none").replace(' ', "_"),
            self.date.as_deref().unwrap_or("none").replace(' ', "_"),
            rect(ModuleKind::Visualiser),
            self.visualiser_shown(),
            self.bands.len(),
            self.modules.dragging().map_or("none", |k| k.slug()),
            rect(ModuleKind::Dictation),
        );
        if self.reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.reported = Some(line);
        }
    }
}

pub struct DesktopLayer {
    model: Entity<DesktopModel>,
    _observe: Subscription,
}

impl DesktopLayer {
    pub fn new(model: Entity<DesktopModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            _observe: observe,
        }
    }

    fn pointer(&mut self, x: f32, y: f32, phase: u8, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| {
            let (x, y) = (x - model.inset[3], y - model.inset[0]);
            let out = match phase {
                0 => model.modules.pointer_down(x, y),
                1 => model.modules.pointer_moved(x, y),
                _ => model.modules.pointer_up(x, y),
            };
            if let Some((kind, place)) = out.placed {
                println!(
                    "GNOS_UX_DESKTOP_DROP module={} place=[{:.4},{:.4}]",
                    kind.slug(),
                    place.x,
                    place.y
                );
                cx.emit(DesktopEvent::Placed(kind, place));
            }
            if out.changed {
                cx.notify();
            }
        });
    }
}

fn clock(time: &str, date: Option<&str>, rect: (f32, f32, f32, f32)) -> gpui::Div {
    let (x, y, w, h) = rect;
    let prefs = crate::surface_prefs::current();
    let scale = prefs.clock_scale_pct as f32 / 100.0;
    let show_date = prefs.clock_date;
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .flex()
        .flex_col()
        .justify_center()
        // SET-02D: the operator's size, and the date line or not.
        .child(line(time, COLOR_ON_SURFACE, 72.0 * scale, 84.0 * scale))
        .children(
            date.filter(|_| show_date)
                .map(|d| line(d, COLOR_ON_SURFACE_VARIANT, 18.0 * scale, 24.0 * scale)),
        )
}

fn visualiser(bands: &[f32], rect: (f32, f32, f32, f32)) -> gpui::Div {
    let (x, y, w, h) = rect;
    let n = bands.len().max(1) as f32;
    let gap = 4.0;
    let bar = ((w - gap * (n - 1.0)) / n).max(1.0);
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .children(bands.iter().enumerate().map(|(i, v)| {
            let height = (v.clamp(0.0, 1.0) * h).max(2.0);
            div()
                .absolute()
                .left(px(i as f32 * (bar + gap)))
                .top(px(h - height))
                .w(px(bar))
                .h(px(height))
                .rounded(px(bar.min(6.0) / 2.0))
                .bg(rgba(COLOR_PRIMARY))
        }))
}

/// The mic patch, and the surface it grows into.
///
/// The patch is the element's resting shape: the icon alone, with no
/// material. The material is the deck's, and it comes when the element has
/// something to say.
pub fn dictation(
    view: &DictationView,
    rect: (f32, f32, f32, f32),
    radius: f32,
    viewport_height: f32,
    motion: crate::product_indicator::DictationMotion,
    visible_text: String,
) -> gpui::Div {
    let (x, y, w, h) = rect;
    let patch = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius.min(w * 0.5)))
        .when(view.active, |patch| patch.bg(rgba(DICTATION_FILL)))
        .child(crate::icons::icon_element(crate::icons::IconSpec::new(
            "microphone",
            16.0,
            if view.active {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            },
        )));
    // The surface is the patch's extrusion: it comes out of the mic patch,
    // and it goes back in. Its height follows its content, so a new line
    // appears by the surface growing — never by a cut.
    let surface = {
        let openness = motion.extrusion.phase();
        // DICTATION-DOCK: the surface emerges from the Deck. The extrusion's
        // phase opens it; the follower follows the text; the rectangle, the
        // height and the opacity all come from that one place, so the box and
        // its movement cannot disagree.
        let height = motion.height.value.max(0.0) * openness.clamp(0.0, 1.0);
        let state_label = if view.recording {
            "En écoute"
        } else {
            "Transcription…"
        };
        let elapsed = format!(
            "{}:{:02}",
            view.elapsed_seconds / 60,
            view.elapsed_seconds % 60
        );
        div()
            .absolute()
            .left(px(x))
            // Anchored by its lower edge: the surface is as tall as the text
            // it holds, and it grows upwards from the patch.
            .bottom(px((viewport_height - y + DICTATION_SURFACE_GAP).max(0.0)))
            .w(px(DICTATION_SURFACE_WIDTH))
            .h(px(height))
            .rounded(px(radius))
            .bg(rgba(DICTATION_FILL))
            .overflow_hidden()
            .flex()
            .flex_col()
            .justify_end()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(8.0))
            .opacity(openness.clamp(0.0, 1.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgba(COLOR_PRIMARY)),
                    )
                    .child(crate::product_design::text(
                        state_label,
                        crate::product_design::ProductTextStyle::Label,
                        crate::product_design::ProductDesignContext::default(),
                    ))
                    .child(crate::product_design::text(
                        elapsed.clone(),
                        crate::product_design::ProductTextStyle::Caption,
                        crate::product_design::ProductDesignContext::default(),
                    )),
            )
            .when(!visible_text.is_empty(), |surface| {
                surface.child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(6.0))
                        .child(
                            div()
                                .min_w_0()
                                // The text wraps inside the panel: a wider
                                // maximum would lay the line out past the
                                // panel's edge instead of folding it.
                                .max_w(px(DICTATION_SURFACE_WIDTH - DICTATION_SURFACE_PADDING))
                                .child(crate::product_design::text(
                                    visible_text.clone(),
                                    crate::product_design::ProductTextStyle::Body,
                                    crate::product_design::ProductDesignContext::default(),
                                )),
                        )
                        // The live edge: the model is writing here.
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(18.0))
                                .flex_shrink_0()
                                .rounded(px(1.0))
                                .bg(rgba(COLOR_PRIMARY)),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgba(COLOR_PRIMARY)),
                    )
                    .child(crate::product_design::text(
                        state_label,
                        crate::product_design::ProductTextStyle::Label,
                        crate::product_design::ProductDesignContext::default(),
                    ))
                    .child(crate::product_design::text(
                        elapsed.clone(),
                        crate::product_design::ProductTextStyle::Caption,
                        crate::product_design::ProductDesignContext::default(),
                    )),
            )
    };
    // The wrapper fills the surface: a static wrapper would have no height
    // (its only child is absolute), and what a zero-height box holds is
    // clipped away.
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(patch)
        .child(surface)
}

impl Render for DesktopLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.model.read(cx).closed {
            window.remove_window();
            return div();
        }
        let viewport = window.viewport_size();
        let viewport = (f32::from(viewport.width), f32::from(viewport.height));
        let scale = window.scale_factor();
        let (rects, clock_el, vis_el, dict_el, wallpaper, inset) =
            self.model.update(cx, |model, _cx| {
                // The modules live in what the shell leaves free; the wallpaper
                // covers the whole output.
                let [top, right, bottom, left] = model.inset;
                model.modules.set_viewport((
                    (viewport.0 - left - right).max(0.0),
                    (viewport.1 - top - bottom).max(0.0),
                ));
                let shifted = |(x, y, w, h): (f32, f32, f32, f32)| (x + left, y + top, w, h);
                model.device_size = (
                    (viewport.0 * scale).round() as u32,
                    (viewport.1 * scale).round() as u32,
                );
                model.report();
                let mut rects = model.modules.input_rects();
                // MODS-01: a module the hull did not boot is not on the desktop.
                let clock_on = !model.clock_hidden && crate::modules::on_desktop(ModuleKind::Clock);
                let vis_on = crate::modules::on_desktop(ModuleKind::Visualiser);
                if !clock_on && model.modules.dragging() != Some(ModuleKind::Clock) {
                    let clock = model.modules.rect(ModuleKind::Clock);
                    rects.retain(|r| *r != clock);
                }
                if !(model.visualiser_shown() && vis_on) && model.modules.dragging().is_none() {
                    let vis = model.modules.rect(ModuleKind::Visualiser);
                    rects.retain(|r| *r != vis);
                }
                let clock_el = model.time.as_deref().filter(|_| clock_on).map(|t| {
                    clock(
                        t,
                        model.date.as_deref(),
                        shifted(model.modules.rect(ModuleKind::Clock)),
                    )
                });
                let vis_el = (vis_on && !model.bands.is_empty()).then(|| {
                    visualiser(
                        &model.bands,
                        shifted(model.modules.rect(ModuleKind::Visualiser)),
                    )
                });
                // DICTATION-01: the mic patch is a button, so it stays clickable
                // even when it is dark; the surface above it joins the input
                // region only while it is showing.
                let dictation_on = crate::modules::on_desktop(ModuleKind::Dictation);
                let dictation_rect = shifted(model.modules.rect(ModuleKind::Dictation));
                if dictation_on {
                    rects.push(dictation_rect);
                }
                // DICTATION-01: the element is painted by the scene, on the
                // surface that stands above the windows. The desktop layer
                // keeps its place in the input region, so the element stays
                // draggable.
                let dict_el: Option<gpui::Div> = None;
                (
                    rects,
                    clock_el,
                    vis_el,
                    dict_el,
                    model.wallpaper.clone(),
                    model.inset,
                )
            });
        let [top, _, _, left] = inset;
        let bounds: Vec<Bounds<gpui::Pixels>> = rects
            .iter()
            .map(|(x, y, w, h)| {
                Bounds::new(point(px(*x + left), px(*y + top)), size(px(*w), px(*h)))
            })
            .collect();
        window.set_input_region(Some(&bounds));
        let mut root = div()
            .id("gnosix-desktop")
            .size_full()
            .relative()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                    this.pointer(f32::from(e.position.x), f32::from(e.position.y), 0, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                this.pointer(f32::from(e.position.x), f32::from(e.position.y), 1, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseUpEvent, _, cx| {
                    this.pointer(f32::from(e.position.x), f32::from(e.position.y), 2, cx);
                }),
            );
        if let Some(c) = clock_el {
            root = root.child(c);
        }
        if let Some(v) = vis_el {
            root = root.child(v);
        }
        if let Some(d) = dict_el {
            root = root.child(d);
        }
        let base = div().size_full().relative();
        let base = match wallpaper {
            // WALL-01: under the modules; it takes no input (the input
            // region is the modules only). Cover, not fill: while the surface
            // takes its final size, a picture prepared for the previous one is
            // cropped a little rather than stretched.
            Some(path) => base.child({
                use gpui::StyledImage as _;
                gpui::img(path)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover)
            }),
            None => base,
        };
        base.child(root.absolute().top_0().left_0())
    }
}

/// Open the desktop surface for one output and return its model.
pub fn open_desktop_layer(
    cx: &mut App,
    display_id: Option<gpui::DisplayId>,
) -> Option<Entity<DesktopModel>> {
    let model = cx.new(|_| DesktopModel::default());
    let for_window = model.clone();
    cx.open_window(
        gpui::WindowOptions {
            titlebar: None,
            window_bounds: Some(gpui::WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(0.0), px(0.0)),
            })),
            display_id,
            app_id: Some(DESKTOP_NAMESPACE.to_owned()),
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            kind: gpui::WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
                namespace: DESKTOP_NAMESPACE.to_owned(),
                layer: gpui::layer_shell::Layer::Background,
                anchor: gpui::layer_shell::Anchor::TOP
                    | gpui::layer_shell::Anchor::RIGHT
                    | gpui::layer_shell::Anchor::BOTTOM
                    | gpui::layer_shell::Anchor::LEFT,
                // Under the shell's own reservation too: the wallpaper is the
                // whole output's, what a translucent Bar shows through.
                exclusive_zone: Some(px(-1.0)),
                keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::None,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| DesktopLayer::new(for_window, cx)),
    )
    .map_err(|e| eprintln!("GNOS_UX_DESKTOP open failed={e}"))
    .ok()?;
    println!("GNOS_UX_DESKTOP state=opened namespace={DESKTOP_NAMESPACE}");
    Some(model)
}
