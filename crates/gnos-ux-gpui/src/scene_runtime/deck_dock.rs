//! DECK-01: the Deck in the scene: its pins, its cards' measure, its
//! motion, pointer, wave and receipts.

use super::*;

/// DECK-01B: the label tab's width: the shaped name plus its padding, never
/// narrower than a card.
#[cfg(feature = "component-lab")]
pub(super) fn deck_label_width(window: &Window, label: &str) -> f32 {
    use crate::deck::wave::{LABEL_PAD, LABEL_PX};
    (deck_text_width(window, label, LABEL_PX) + 2.0 * LABEL_PAD).max(crate::deck::CARD)
}

/// The shaped width of one line of text at `size`.
#[cfg(feature = "component-lab")]
pub(super) fn deck_text_width(window: &Window, text: &str, size: f32) -> f32 {
    let run = gpui::TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: gpui::Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .layout_line(text, px(size), &[run], None);
    f32::from(line.width)
}

/// BAR-01J: how long an interrupted leg slows down before the Bar turns, as
/// a share of a leg (60–200 ms).
pub(super) const DECELERATION_SHARE: f32 = 0.12;

/// DECK-01A: the pins store, read once at start on the native rail. A
/// malformed file is reported and treated as no pins; it is never rewritten
/// (DECK-01D: the second value says whether the store may be written).
#[cfg(feature = "component-lab")]
pub(super) fn load_deck_pins(native: bool) -> (crate::deck::DeckPins, bool) {
    if !native {
        return (crate::deck::DeckPins::default(), false);
    }
    let Some(path) = crate::deck::DeckPins::default_path() else {
        println!("GNOS_UX_DECK_PINS status=no_state_dir count=0");
        return (crate::deck::DeckPins::default(), false);
    };
    match crate::deck::DeckPins::load(&path) {
        Ok(pins) => {
            println!(
                "GNOS_UX_DECK_PINS status=loaded count={}",
                pins.pinned.len()
            );
            (pins, true)
        }
        Err(error) => {
            println!("GNOS_UX_DECK_PINS status=refused count=0 detail={error}");
            (crate::deck::DeckPins::default(), false)
        }
    }
}

impl SceneHost {
    /// DECK-01B: advances the magnification wave (DECK-01D: and a drag's
    /// gaps); true while either moves.
    pub(super) fn tick_deck(&mut self) -> bool {
        #[cfg(feature = "component-lab")]
        if self.review_native_rail {
            if self.deck_wave.needs_frame() {
                self.deck_wave_frames += 1;
            }
            let now = Instant::now();
            let reduced = self.effective_reduced_motion();
            let wave = self.deck_wave.tick(now, reduced);
            let drag = self
                .deck_drag
                .as_mut()
                .is_some_and(|drag| drag.tick(now, reduced));
            return wave || drag;
        }
        false
    }

    /// DECK-01A/B: the Deck's content: appcards at their wave size,
    /// separators, the AppStack control's slot and the label tab's text,
    /// placed from this frame's `DeckShape` (relative to the Deck's base).
    /// `origin` is where the Dock's own element stands; `control` fills the
    /// control slot (the AppStack button, or the search field while the
    /// AppStack is out, DECK-01C).
    // Called only from the product Dock, which the R13 client builds.
    #[cfg(all(feature = "component-lab", feature = "r13-production-client"))]
    pub(super) fn deck_dock(
        &mut self,
        dock: gpui::Div,
        origin: (f32, f32),
        affordance: Option<gpui::AnyElement>,
    ) -> gpui::Div {
        use crate::deck::{ICON, MAX_DOTS};
        use crate::reference_bar::{
            COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
            COLOR_SURFACE_CONTAINER_HIGHEST,
        };
        let Some(shape) = self.deck_shape.clone() else {
            return dock;
        };
        let cards = self.deck_cards.clone();
        // DECK-01D: a card being carried is the ghost, not a card.
        let carried = self
            .deck_drag
            .as_ref()
            .filter(|drag| matches!(drag.source, crate::deck::drag::DragSource::Card { .. }))
            .map(|drag| drag.key.clone());
        let mut dock = dock;
        for (card, (rect, scale)) in cards.iter().zip(shape.cards.iter().zip(&shape.scales)) {
            if carried.as_deref() == Some(card.key.as_str()) {
                continue;
            }
            let icon = ICON * scale;
            let glyph = match card.icon.as_deref().and_then(|name| self.app_icon(name)) {
                Some(path) => gpui::img(path).w(px(icon)).h(px(icon)).into_any_element(),
                None => div()
                    .w(px(icon))
                    .h(px(icon))
                    .rounded(px(10.0 * scale))
                    .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(16.0 * scale))
                    .text_color(rgba(COLOR_ON_SURFACE))
                    .child(card.badge())
                    .into_any_element(),
            };
            let dot = if card.focused {
                COLOR_PRIMARY
            } else {
                COLOR_ON_SURFACE_VARIANT
            };
            let dots = (0..card.windows.len().min(MAX_DOTS))
                .map(move |_| div().w(px(4.0)).h(px(4.0)).rounded_full().bg(rgba(dot)));
            // The running dots sit on the card's border side.
            let dots = div()
                .absolute()
                .left(px(0.0))
                .w(px(rect.width))
                .flex()
                .justify_center()
                .gap(px(3.0))
                .children(dots);
            let dots = if shape.grows_up {
                dots.bottom(px(1.0))
            } else {
                dots.top(px(1.0))
            };
            // Presses on cards are routed by the scene (DECK-01D), so a
            // press can become a click or a drag.
            dock = dock.child(
                div()
                    .absolute()
                    .left(px(rect.x - origin.0))
                    .top(px(rect.y - origin.1))
                    .w(px(rect.width))
                    .h(px(rect.height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph)
                    .child(dots),
            );
        }
        for rect in &shape.separators {
            dock = dock.child(
                div()
                    .absolute()
                    .left(px(rect.x - origin.0))
                    .top(px(rect.y - origin.1))
                    .w(px(rect.width))
                    .h(px(rect.height))
                    .bg(rgba(COLOR_OUTLINE_VARIANT)),
            );
        }
        // The label rides the tab's outer edge and shows only where the tab
        // has extruded.
        if let (Some(tab), Some(label)) = (&shape.tab, self.deck_wave.label()) {
            let clip = tab.clip;
            let text = tab.text_box;
            // DECK-01D: a sheet's rows are uncovered under the label as the
            // tab extrudes; they ride its crest with it.
            let rows = self
                .deck_sheet_rows()
                .map(|(rows, lines, _)| self.deck_sheet_children(rows, lines, (clip.x, clip.y)))
                .unwrap_or_default();
            dock = dock.child(
                div()
                    .absolute()
                    .left(px(clip.x - origin.0))
                    .top(px(clip.y - origin.1))
                    .w(px(clip.width))
                    .h(px(clip.height))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(text.x - clip.x))
                            .top(px(text.y - clip.y))
                            .w(px(text.width))
                            .h(px(text.height))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(crate::deck::wave::LABEL_PX))
                            .text_color(rgba(COLOR_ON_SURFACE))
                            .whitespace_nowrap()
                            .child(label.to_owned()),
                    )
                    .children(rows),
            );
        }
        let control = shape.control;
        let affordance = if self.stack.is_visible() {
            let search: gpui::AnyView = self.appstack_search.clone().into();
            Some(
                div()
                    .size_full()
                    .rounded_full()
                    .bg(rgba(crate::reference_bar::COLOR_SURFACE_CONTAINER))
                    .flex()
                    .items_center()
                    .px(px(16.0))
                    .overflow_hidden()
                    .child(search)
                    .into_any_element(),
            )
        } else {
            affordance
        };
        dock.when_some(affordance, |dock, affordance| {
            dock.child(
                div()
                    .absolute()
                    .left(px(control.x - origin.0))
                    .top(px(control.y - origin.1))
                    .w(px(control.width))
                    .h(px(control.height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(affordance),
            )
        })
    }

    /// DECK-01B: the pointer moved (`Some`) or left the surface (`None`).
    /// Over the Deck's shape it drives the wave; the card under it gets the
    /// label tab. Settings and the open AppStack take the pointer first.
    #[cfg(feature = "component-lab")]
    pub(super) fn deck_pointer(&mut self, pointer: Option<(f32, f32)>, cx: &mut Context<Self>) {
        // DECK-01D: a drag carries its ghost; a press may become one; an
        // open sheet holds the wave while the pointer is above the row.
        if let Some((x, y)) = pointer {
            self.deck_last_pointer = Some((x, y));
            if self.deck_drag.is_some() {
                self.deck_drag_moved(Some((x, y)), cx);
            } else {
                self.deck_press_moved(x, y, cx);
            }
            if self.deck_sheet_hold(x, y, cx) {
                return;
            }
        }
        let covered = self.settings.is_open() || self.stack.is_visible() || self.bar_panel_open();
        let dragging = self.deck_drag.is_some();
        let over = pointer.filter(|_| !covered).and_then(|(x, y)| {
            let shape = self.deck_shape.as_ref()?;
            shape.bounds.contains(x, y).then(|| {
                let card = shape
                    .card_at(x, y)
                    .filter(|_| !dragging)
                    .and_then(|i| self.deck_cards.get(i).map(|c| (i, c.name.clone())));
                (x - self.deck_rest_left, card)
            })
        });
        let (rest_x, card) = match over {
            Some((rest_x, card)) => (Some(rest_x), card),
            None => (None, None),
        };
        let key = card
            .as_ref()
            .and_then(|(i, _)| self.deck_cards.get(*i).map(|c| c.key.clone()));
        let moved = self.deck_wave.pointer(rest_x);
        let hovered = self.deck_wave.hover(card);
        if hovered {
            println!(
                "GNOS_UX_DECK_HOVER card={} pointer={}",
                key.as_deref().unwrap_or("None"),
                pointer.map_or("exit".to_owned(), |(x, y)| format!("{x:.1},{y:.1}"))
            );
            if !dragging {
                self.deck_sheet_hover(key.as_deref(), cx);
            }
        }
        if moved || hovered {
            if self.effective_reduced_motion() {
                self.tick_deck();
            }
            self.start_motion_driver(cx);
            cx.notify();
        }
    }

    /// DECK-01B: `GNOS_UX_DECK_WAVE` when the shape a settled wave paints
    /// changes (a settle, or a pointer that moves without motion), with the
    /// frames stepped since the previous receipt.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_deck_wave(&mut self) {
        let Some(shape) = &self.deck_shape else {
            return;
        };
        let r = |r: &blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let input = self.deck_wave.input();
        let cards = self
            .deck_cards
            .iter()
            .zip(shape.cards.iter().zip(&shape.scales))
            .map(|(card, (rect, scale))| {
                format!("{{{} scale={scale:.3} rect={}}}", card.key, r(rect))
            })
            .collect::<Vec<_>>()
            .join(";");
        let hovered = self
            .deck_wave
            .hovered()
            .and_then(|i| self.deck_cards.get(i))
            .map_or("None", |c| c.key.as_str());
        let line = format!(
            "amount={:.3} hovered={hovered} label={:?} grows_up={} base={} bounds={} tab={} cards=[{cards}]",
            input.amount,
            self.deck_wave.label().unwrap_or(""),
            shape.grows_up,
            r(&shape.base),
            r(&shape.bounds),
            shape.tab.as_ref().map_or("None".to_owned(), |t| r(&t.clip)),
        );
        if self.deck_wave_reported.as_deref() == Some(line.as_str()) {
            return;
        }
        let (head, tail) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        println!(
            "GNOS_UX_DECK_WAVE state=settled {head} frames={} {tail}",
            self.deck_wave_frames
        );
        self.deck_wave_frames = 0;
        self.deck_wave_reported = Some(line);
    }

    /// DECK-01A: activate an appcard. A running application focuses its
    /// first window that does not have focus (else its first window); a
    /// pinned application that is not running launches.
    #[cfg(feature = "component-lab")]
    pub(super) fn activate_deck_card(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = &window;
        self.refresh_product_runtime();
        let Some(card) = self.deck_cards.iter().find(|c| c.key == key).cloned() else {
            return;
        };
        let target = card
            .windows
            .iter()
            .find(|w| !self.product_runtime.application_is_focused(w))
            .or(card.windows.first())
            .cloned();
        if let Some(target) = target {
            #[cfg(feature = "r13-production-client")]
            {
                let record = self
                    .product_controller
                    .focus_application(&mut self.product_runtime, target.clone());
                println!(
                    "GNOS_UX_DECK_ACTIVATE key={key} action=focus pid={} outcome={}",
                    target.process_id,
                    record.terminal.map_or("NONE", |t| t.as_str())
                );
            }
            #[cfg(not(feature = "r13-production-client"))]
            println!(
                "GNOS_UX_DECK_ACTIVATE key={key} action=focus pid={} outcome=NO_CLIENT",
                target.process_id
            );
            cx.notify();
            return;
        }
        let Some((entry, generation)) = card.entry else {
            return;
        };
        let selected = self
            .product_runtime
            .installed_applications()
            .into_iter()
            .find(|c| c.reference.entry() == &entry && c.reference.generation() == &generation);
        let Some(selected) = selected else {
            println!(
                "GNOS_UX_DECK_LAUNCH entry={} outcome=STALE detail=catalog_changed",
                entry.as_str()
            );
            return;
        };
        let record =
            crate::product_launch::launch_application(&mut self.product_runtime, &selected);
        // CURSOR-01B: the pointer says it is starting until its window is
        // there.
        #[cfg(feature = "component-lab")]
        self.note_launch(&record, cx);
        let launched = record.terminal.as_ref().is_some_and(|t| t.is_launched());
        println!(
            "GNOS_UX_DECK_LAUNCH entry={} outcome={} detail={}",
            record.entry.as_str(),
            record.terminal.as_ref().map_or("NONE", |t| t.code()),
            record.detail
        );
        if launched {
            self.stack.record_launch(&entry);
        }
        cx.notify();
    }

    /// DECK-01A: `GNOS_UX_DECK` when the Deck's geometry or appcards change.
    #[cfg(feature = "component-lab")]
    pub(super) fn report_deck(&mut self, deck: GeometryRect) {
        let layout = crate::deck::deck_layout(&self.deck_cards, deck.left(), deck.top());
        let r = |r: &blobin_ui_core::layout::Rect| {
            format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height)
        };
        let cards = self
            .deck_cards
            .iter()
            .zip(&layout.cards)
            .map(|(card, rect)| {
                format!(
                    "{{{} pinned={} windows={} focused={} rect={}}}",
                    card.key,
                    card.pinned,
                    card.windows.len(),
                    card.focused,
                    r(rect)
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let line = format!(
            "GNOS_UX_DECK deck={} control={} separators={} cards=[{cards}]",
            r(&layout.deck),
            r(&layout.control),
            layout.separators.len()
        );
        if self.deck_reported.as_deref() != Some(line.as_str()) {
            println!("{line}");
            self.deck_reported = Some(line);
        }
    }
}
