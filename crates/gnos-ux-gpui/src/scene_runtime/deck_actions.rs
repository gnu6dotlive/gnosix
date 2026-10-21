//! DECK-01D: the scene side of appcard actions (D04, D10, D11).
//!
//! The pure models live in `deck::sheet` (the action sheet and its dwell)
//! and `deck::drag` (drop targets, gaps and the pins a drop leaves). This
//! module routes the pointer to them and carries their outcomes out through
//! the typed capabilities: a window's focus request, a launch naming a
//! desktop action, and the pins store.

use super::*;
use crate::deck::drag::{
    DRAG_SLOP, DeckDrag, DragSource, DropTarget, drop_target, pins_after_drop, unpin_target,
};
use crate::deck::sheet::{SheetItem, row_at, sheet_height, sheet_items, sheet_rows};
use crate::deck::{CARD, DeckSpacing};
use blobin_ui_core::layout::Rect;

/// A primary press on an appcard, waiting to become a click or a drag.
#[derive(Clone, Debug)]
pub(super) struct DeckPress {
    key: String,
    x: f32,
    y: f32,
    /// The pointer within the card, and the card's painted size.
    grab: (f32, f32),
    size: f32,
}

impl SceneHost {
    /// The row's spacing while a drag reshapes it.
    pub(super) fn deck_spacing(&self) -> DeckSpacing {
        self.deck_drag
            .as_ref()
            .map(|drag| drag.spacing(&self.deck_cards))
            .unwrap_or_default()
    }

    /// The sheet's rows where they stand this frame, and its clip.
    pub(super) fn deck_sheet_rows(
        &self,
    ) -> Option<(Vec<crate::deck::sheet::SheetRow>, Vec<Rect>, Rect)> {
        let shape = self.deck_shape.as_ref()?;
        let tab = shape.tab.as_ref()?;
        if self.deck_sheet_items.is_empty() || self.deck_wave.sheet_shown().is_none() {
            return None;
        }
        let (rows, lines) = sheet_rows(&self.deck_sheet_items, tab.body, shape.grows_up);
        Some((rows, lines, tab.clip))
    }

    /// The pointer moved while a sheet is out: above the Deck's row it holds
    /// the wave and hovers the sheet's rows. Returns whether it held.
    pub(super) fn deck_sheet_hold(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if !self.deck_sheet.is_open() {
            return false;
        }
        let Some(shape) = &self.deck_shape else {
            return false;
        };
        if !shape.bounds.contains(x, y) || shape.base.contains(x, y) {
            return false;
        }
        let row = self
            .deck_sheet_rows()
            .and_then(|(rows, _, clip)| row_at(&rows, clip, x, y));
        if self.deck_sheet.set_hovered(row) {
            cx.notify();
        }
        true
    }

    /// The card under the pointer changed: a new card closes any sheet and
    /// starts its dwell.
    pub(super) fn deck_sheet_hover(&mut self, key: Option<&str>, cx: &mut Context<Self>) {
        let was_open = self.deck_sheet.is_open();
        let token = self.deck_sheet.hover(key);
        if was_open && !self.deck_sheet.is_open() {
            self.deck_wave.set_sheet(None);
            self.report_deck_sheet_closed("card");
            self.deck_sheet_moved(cx);
        }
        let idle = self.deck_drag.is_none() && self.deck_press.is_none();
        if let Some(token) = token.filter(|_| idle) {
            cx.spawn(async move |this, cx| {
                let dwell = crate::surface_prefs::advanced_ms("sheet_dwell_ms");
                cx.background_executor().timer(dwell).await;
                let _ = this.update(cx, |view, cx| view.deck_dwell_elapsed(token, cx));
            })
            .detach();
        }
    }

    fn deck_dwell_elapsed(&mut self, token: u64, cx: &mut Context<Self>) {
        let busy = self.deck_drag.is_some()
            || self.deck_press.is_some()
            || self.settings.is_open()
            || self.stack.is_visible();
        if busy || !self.deck_sheet.dwell_elapsed(token) {
            return;
        }
        self.open_deck_sheet("dwell", cx);
    }

    /// Grow the sheet's card tab into its sheet.
    fn open_deck_sheet(&mut self, trigger: &str, cx: &mut Context<Self>) {
        let Some(key) = self.deck_sheet.card().map(str::to_owned) else {
            return;
        };
        let Some(card) = self.deck_cards.iter().find(|c| c.key == key).cloned() else {
            self.deck_sheet.close();
            return;
        };
        let actions = card
            .entry
            .as_ref()
            .and_then(|(entry, generation)| {
                self.product_runtime
                    .installed_applications()
                    .into_iter()
                    .find(|c| {
                        c.reference.entry() == entry && c.reference.generation() == generation
                    })
            })
            .map(|content| content.actions)
            .unwrap_or_default();
        let runtime = &self.product_runtime;
        let items = sheet_items(&card, &actions, |w| runtime.application_is_focused(w));
        if items.is_empty() {
            self.deck_sheet.close();
            return;
        }
        self.deck_wave.set_sheet(Some(sheet_height(&items)));
        println!(
            "GNOS_UX_DECK_SHEET state=open trigger={trigger} key={key} items=[{}]",
            items
                .iter()
                .map(SheetItem::receipt)
                .collect::<Vec<_>>()
                .join(";")
        );
        self.deck_sheet_items = items;
        self.deck_sheet_reported = None;
        self.deck_sheet_moved(cx);
    }

    pub(super) fn close_deck_sheet(&mut self, reason: &str, cx: &mut Context<Self>) -> bool {
        if !self.deck_sheet.close() {
            return false;
        }
        self.deck_wave.set_sheet(None);
        self.report_deck_sheet_closed(reason);
        self.deck_sheet_moved(cx);
        true
    }

    /// The sheet's target changed: the wave retracts or extrudes it.
    fn deck_sheet_moved(&mut self, cx: &mut Context<Self>) {
        if self.effective_reduced_motion() {
            self.tick_deck();
        }
        self.start_motion_driver(cx);
        cx.notify();
    }

    fn report_deck_sheet_closed(&mut self, reason: &str) {
        println!("GNOS_UX_DECK_SHEET state=closed reason={reason}");
        self.deck_sheet_reported = None;
    }

    /// `GNOS_UX_DECK_SHEET state=settled` once an open sheet stands still:
    /// its extent and every row, for the harness to aim at.
    pub(super) fn report_deck_sheet(&mut self) {
        if !self.deck_sheet.is_open() {
            return;
        }
        let Some((rows, _, clip)) = self.deck_sheet_rows() else {
            return;
        };
        let r = |r: &Rect| format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height);
        let rows = rows
            .iter()
            .map(|row| {
                format!(
                    "{{{} rect={}}}",
                    self.deck_sheet_items[row.item].receipt(),
                    r(&row.rect)
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let line = format!(
            "key={} sheet={} rows=[{rows}]",
            self.deck_sheet.card().unwrap_or("None"),
            r(&clip)
        );
        if self.deck_sheet_reported.as_deref() != Some(line.as_str()) {
            println!("GNOS_UX_DECK_SHEET state=settled {line}");
            self.deck_sheet_reported = Some(line);
        }
    }

    /// `GNOS_UX_DECK_DRAG state=settled` once a drag's gaps stand still: its
    /// target, the open gap and the cards that stand, as painted.
    pub(super) fn report_deck_drag(&mut self) {
        let Some(drag) = &self.deck_drag else {
            self.deck_drag_reported = None;
            return;
        };
        let Some(shape) = &self.deck_shape else {
            return;
        };
        if drag.needs_frame() || self.deck_wave.needs_frame() {
            return;
        }
        let r = |r: &Rect| format!("[{:.1},{:.1},{:.1},{:.1}]", r.x, r.y, r.width, r.height);
        let cards = self
            .deck_cards
            .iter()
            .zip(&shape.cards)
            .filter(|(card, _)| card.key != drag.key)
            .map(|(card, rect)| format!("{{{} rect={}}}", card.key, r(rect)))
            .collect::<Vec<_>>()
            .join(";");
        let gaps = shape.gaps.iter().map(r).collect::<Vec<_>>().join(";");
        let line = format!(
            "key={} target={} base={} gaps=[{gaps}] cards=[{cards}]",
            drag.key,
            drag.target().receipt(),
            r(&shape.base)
        );
        if self.deck_drag_reported.as_deref() != Some(line.as_str()) {
            println!("GNOS_UX_DECK_DRAG state=settled {line}");
            self.deck_drag_reported = Some(line);
        }
    }

    /// A press that has travelled past the slop becomes a drag of its card,
    /// when the card is a catalog entry.
    pub(super) fn deck_press_moved(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        let Some(press) = self.deck_press.clone() else {
            return;
        };
        if (x - press.x).hypot(y - press.y) <= DRAG_SLOP {
            return;
        }
        let Some(card) = self.deck_cards.iter().find(|c| c.key == press.key).cloned() else {
            return;
        };
        if card.entry.is_none() {
            return;
        }
        self.deck_press = None;
        let icon = card.icon.as_deref().and_then(|name| self.app_icon(name));
        let drag = DeckDrag::new(
            DragSource::Card {
                pinned: card.pinned,
            },
            card.key.clone(),
            card.name.clone(),
            icon,
            (x, y),
            press.grab,
            press.size,
        );
        self.begin_deck_drag(drag, "card", cx);
    }

    /// A tile pulled out of the AppStack's grid.
    pub(super) fn deck_tile_drag(
        &mut self,
        row: crate::deck::stack::AppRow,
        cx: &mut Context<Self>,
    ) {
        let pointer = self.deck_last_pointer.unwrap_or((0.0, 0.0));
        let drag = DeckDrag::new(
            DragSource::Tile,
            row.entry.as_str().to_owned(),
            row.name.clone(),
            row.icon.clone(),
            pointer,
            (CARD / 2.0, CARD / 2.0),
            CARD,
        );
        self.begin_deck_drag(drag, "tile", cx);
    }

    fn begin_deck_drag(&mut self, drag: DeckDrag, source: &str, cx: &mut Context<Self>) {
        self.close_deck_sheet("drag", cx);
        self.deck_wave.hover(None);
        println!(
            "GNOS_UX_DECK_DRAG state=start source={source} key={}",
            drag.key
        );
        self.deck_drag = Some(drag);
        self.deck_drag_moved(None, cx);
    }

    /// The carried app follows the pointer; the Deck aims its gap.
    pub(super) fn deck_drag_moved(&mut self, pointer: Option<(f32, f32)>, cx: &mut Context<Self>) {
        let Some(mut drag) = self.deck_drag.take() else {
            return;
        };
        if let Some(pointer) = pointer {
            drag.pointer = pointer;
        }
        if let Some(shape) = &self.deck_shape {
            let target = drop_target(shape, &self.deck_cards, &drag, drag.pointer);
            if drag.retarget(target) {
                println!(
                    "GNOS_UX_DECK_DRAG state=target key={} target={} pointer={:.1},{:.1}",
                    drag.key,
                    target.receipt(),
                    drag.pointer.0,
                    drag.pointer.1
                );
            }
        }
        let snap = self.effective_reduced_motion();
        self.deck_drag = Some(drag);
        if snap {
            self.tick_deck();
        }
        self.start_motion_driver(cx);
        cx.notify();
    }

    /// The release of a drag: pin, move, unpin or nothing.
    fn deck_drop(&mut self, pointer: (f32, f32), cx: &mut Context<Self>) {
        self.deck_drag_moved(Some(pointer), cx);
        let Some(drag) = self.deck_drag.take() else {
            return;
        };
        let target = drag.target();
        println!(
            "GNOS_UX_DECK_DRAG state=drop key={} target={}",
            drag.key,
            target.receipt()
        );
        let op = match (target, drag.source) {
            (DropTarget::Cancel, _) => None,
            (DropTarget::Launch, _) => {
                self.launch_entry_key(&drag.key, cx);
                None
            }
            (DropTarget::Remove, _) => Some("unpin"),
            (DropTarget::Insert(_), DragSource::Card { pinned: true }) => Some("move"),
            (DropTarget::Insert(_), _) => Some("pin"),
        };
        if let Some(op) = op {
            self.drop_on_pins(&drag.key, target, op, cx);
        }
        // The wave picks the pointer up again where the drop left it.
        self.deck_wave_reported = None;
        cx.notify();
    }

    /// DECK-04: launch a new instance of the application `key` names (an
    /// entry id), as the AppStack does.
    fn launch_entry_key(&mut self, key: &str, cx: &mut Context<Self>) {
        self.refresh_product_runtime();
        let Some(selected) = self
            .product_runtime
            .installed_applications()
            .into_iter()
            .find(|c| c.reference.entry().as_str() == key)
        else {
            println!("GNOS_UX_DECK_LAUNCH entry={key} outcome=STALE detail=not_installed");
            return;
        };
        let entry = selected.reference.entry().clone();
        let record =
            crate::product_launch::launch_application(&mut self.product_runtime, &selected);
        // CURSOR-01B: the pointer says it is starting until its window is
        // there.
        #[cfg(feature = "component-lab")]
        self.note_launch(&record, cx);
        let launched = record.terminal.as_ref().is_some_and(|t| t.is_launched());
        println!(
            "GNOS_UX_DECK_LAUNCH entry={} via=drop outcome={} detail={}",
            record.entry.as_str(),
            record.terminal.as_ref().map_or("NONE", |t| t.code()),
            record.detail
        );
        if launched {
            self.stack.record_launch(&entry);
        }
        cx.notify();
    }

    /// Apply a drop to the pins store.
    fn drop_on_pins(&mut self, key: &str, target: DropTarget, op: &str, cx: &mut Context<Self>) {
        let visible: Vec<String> = self
            .deck_cards
            .iter()
            .filter(|c| c.pinned)
            .map(|c| c.key.clone())
            .collect();
        let pins = pins_after_drop(&self.deck_pins.pinned, &visible, key, target);
        self.save_deck_pins(pins, op, key, cx);
    }

    /// Persist new pins. With a writable durable profile (R12, DECK-01F)
    /// they are a profile edit; otherwise the pins store (DeckPins,
    /// `gnosix.deck.v1`). A store that was refused at start is never
    /// rewritten; a failed write keeps the pins as they were.
    fn save_deck_pins(&mut self, pins: Vec<String>, op: &str, key: &str, cx: &mut Context<Self>) {
        #[cfg(feature = "r12-production-client")]
        if pins != self.deck_pins.pinned && self.deck_pins_profiled(cx) {
            let outcome = self.write_deck_pins(&pins, cx);
            if outcome == "SENT" {
                self.deck_pins_pending = Some((pins.clone(), Instant::now()));
                self.deck_pins.pinned = pins;
            }
            println!(
                "GNOS_UX_DECK_PIN op={op} key={key} outcome={outcome} store=profile pins=[{}]",
                self.deck_pins.pinned.join(";")
            );
            return;
        }
        #[cfg(not(feature = "r12-production-client"))]
        let _ = &cx;
        let outcome = if pins == self.deck_pins.pinned {
            "UNCHANGED".to_owned()
        } else if !self.deck_pins_writable {
            "REFUSED detail=store_unreadable".to_owned()
        } else if let Some(path) = crate::deck::DeckPins::default_path() {
            let next = crate::deck::DeckPins { pinned: pins };
            match next.save(&path) {
                Ok(()) => {
                    self.deck_pins = next;
                    "SAVED".to_owned()
                }
                Err(error) => format!("FAILED detail={error}"),
            }
        } else {
            "REFUSED detail=no_state_dir".to_owned()
        };
        println!(
            "GNOS_UX_DECK_PIN op={op} key={key} outcome={outcome} pins=[{}]",
            self.deck_pins.pinned.join(";")
        );
    }

    /// Primary press. Returns whether the Deck took it.
    pub(super) fn deck_pointer_down(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if self.settings.is_open() || self.stack.is_visible() || self.bar_panel_open() {
            return false;
        }
        if self.deck_sheet.is_open() {
            let in_sheet = self
                .deck_shape
                .as_ref()
                .and_then(|s| s.tab.as_ref())
                .is_some_and(|tab| tab.clip.contains(x, y));
            if in_sheet {
                let row = self
                    .deck_sheet_rows()
                    .and_then(|(rows, _, clip)| row_at(&rows, clip, x, y));
                self.deck_sheet.press(row);
                return true;
            }
        }
        let Some(shape) = &self.deck_shape else {
            return false;
        };
        let Some(index) = shape.card_at(x, y) else {
            self.close_deck_sheet("press", cx);
            return false;
        };
        let (Some(card), Some(rect)) = (self.deck_cards.get(index), shape.cards.get(index)) else {
            return false;
        };
        self.deck_press = Some(DeckPress {
            key: card.key.clone(),
            x,
            y,
            grab: (x - rect.x, y - rect.y),
            size: rect.width.max(CARD),
        });
        self.close_deck_sheet("press", cx);
        true
    }

    /// Primary release. Returns whether the Deck took it.
    pub(super) fn deck_pointer_up(
        &mut self,
        x: f32,
        y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.deck_drag.is_some() {
            self.deck_drop((x, y), cx);
            return true;
        }
        if self.deck_sheet.pressing() {
            let row = self
                .deck_sheet_rows()
                .and_then(|(rows, _, clip)| row_at(&rows, clip, x, y));
            if let Some(index) = self.deck_sheet.release(row) {
                self.run_sheet_item(index, window, cx);
            }
            return true;
        }
        let Some(press) = self.deck_press.take() else {
            return false;
        };
        let same = self
            .deck_shape
            .as_ref()
            .and_then(|shape| shape.card_at(x, y))
            .and_then(|i| self.deck_cards.get(i))
            .is_some_and(|card| card.key == press.key);
        if same {
            self.activate_deck_card(&press.key, window, cx);
        }
        true
    }

    /// Secondary press: a card's sheet opens at once, or closes.
    pub(super) fn deck_secondary(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if self.settings.is_open()
            || self.stack.is_visible()
            || self.bar_panel_open()
            || self.deck_drag.is_some()
        {
            return false;
        }
        let Some(key) = self
            .deck_shape
            .as_ref()
            .and_then(|shape| shape.card_at(x, y))
            .and_then(|i| self.deck_cards.get(i))
            .map(|card| card.key.clone())
        else {
            return false;
        };
        if self.deck_sheet.toggle(&key) {
            self.open_deck_sheet("secondary", cx);
        } else {
            self.deck_wave.set_sheet(None);
            self.report_deck_sheet_closed("secondary");
            self.deck_sheet_moved(cx);
        }
        true
    }

    /// Escape: cancel a drag, else close the sheet. Returns whether it did.
    pub(super) fn deck_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(drag) = self.deck_drag.take() {
            println!(
                "GNOS_UX_DECK_DRAG state=drop key={} target=cancel reason=escape",
                drag.key
            );
            cx.notify();
            return true;
        }
        if self.close_deck_sheet("escape", cx) {
            return true;
        }
        false
    }

    /// Carry out a sheet row; the sheet closes.
    fn run_sheet_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let _ = &window;
        let Some(item) = self.deck_sheet_items.get(index).cloned() else {
            return;
        };
        let Some(key) = self.deck_sheet.card().map(str::to_owned) else {
            return;
        };
        self.close_deck_sheet("item", cx);
        self.refresh_product_runtime();
        let Some(card) = self.deck_cards.iter().find(|c| c.key == key).cloned() else {
            return;
        };
        let receipt = item.receipt();
        match item {
            SheetItem::Window { index, .. } => {
                let Some(target) = card.windows.get(index).cloned() else {
                    println!(
                        "GNOS_UX_DECK_SHEET_ACTION key={key} item={receipt} outcome=STALE detail=window_gone"
                    );
                    return;
                };
                #[cfg(feature = "r13-production-client")]
                {
                    let record = self
                        .product_controller
                        .focus_application(&mut self.product_runtime, target.clone());
                    println!(
                        "GNOS_UX_DECK_SHEET_ACTION key={key} item={receipt} action=focus pid={} outcome={}",
                        target.process_id,
                        record.terminal.map_or("NONE", |t| t.as_str())
                    );
                }
                #[cfg(not(feature = "r13-production-client"))]
                println!(
                    "GNOS_UX_DECK_SHEET_ACTION key={key} item={receipt} action=focus pid={} outcome=NO_CLIENT",
                    target.process_id
                );
            }
            SheetItem::Action { id, .. } => {
                let Some((entry, generation)) = card.entry else {
                    return;
                };
                let selected = self
                    .product_runtime
                    .installed_applications()
                    .into_iter()
                    .find(|c| {
                        c.reference.entry() == &entry && c.reference.generation() == &generation
                    });
                let Some(selected) = selected else {
                    println!(
                        "GNOS_UX_DECK_SHEET_ACTION key={key} item={receipt} outcome=STALE detail=catalog_changed"
                    );
                    return;
                };
                let record = crate::product_launch::launch_application_action(
                    &mut self.product_runtime,
                    &selected,
                    &id,
                );
                // CURSOR-01B: the pointer says it is starting.
                self.note_launch(&record, cx);
                let launched = record.terminal.as_ref().is_some_and(|t| t.is_launched());
                println!(
                    "GNOS_UX_DECK_SHEET_ACTION key={key} item={receipt} outcome={} detail={}",
                    record.terminal.as_ref().map_or("NONE", |t| t.code()),
                    record.detail
                );
                if launched {
                    self.stack.record_launch(&entry);
                }
            }
            SheetItem::Pin => {
                let shown = self.deck_cards.iter().filter(|c| c.pinned).count();
                self.drop_on_pins(&key, DropTarget::Insert(shown), "pin", cx);
            }
            SheetItem::Unpin => self.drop_on_pins(&key, DropTarget::Remove, "unpin", cx),
        }
        cx.notify();
    }

    /// The sheet's rows and dividers, placed relative to `origin` (the
    /// tab's clip). A hovered row takes the primary tint; the focused
    /// window carries a primary dot. Nothing fades. Painted inside the
    /// product Dock, which the R13 client builds.
    #[cfg(feature = "r13-production-client")]
    pub(super) fn deck_sheet_children(
        &self,
        rows: Vec<crate::deck::sheet::SheetRow>,
        lines: Vec<Rect>,
        origin: (f32, f32),
    ) -> Vec<gpui::AnyElement> {
        use crate::deck::sheet::{ITEM_INSET, ITEM_PX};
        use crate::reference_bar::{COLOR_ON_SURFACE, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY};
        let place = |r: Rect| {
            div()
                .absolute()
                .left(px(r.x - origin.0))
                .top(px(r.y - origin.1))
                .w(px(r.width))
                .h(px(r.height))
        };
        let mut out: Vec<gpui::AnyElement> = lines
            .into_iter()
            .map(|line| {
                place(line)
                    .bg(rgba(COLOR_OUTLINE_VARIANT))
                    .into_any_element()
            })
            .collect();
        for row in rows {
            let Some(item) = self.deck_sheet_items.get(row.item) else {
                continue;
            };
            let hovered = self.deck_sheet.hovered() == Some(row.item);
            let focused = matches!(item, SheetItem::Window { focused: true, .. });
            let mut element = place(row.rect)
                .rounded(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(ITEM_INSET))
                .overflow_hidden()
                .text_size(px(ITEM_PX))
                .text_color(rgba(COLOR_ON_SURFACE))
                .whitespace_nowrap();
            if hovered {
                element = element.bg(crate::notification_view::with_alpha(COLOR_PRIMARY, 0x29));
            }
            if focused {
                element = element.child(
                    div()
                        .flex_none()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(rgba(COLOR_PRIMARY)),
                );
            }
            out.push(
                element
                    .child(
                        div()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(item.text().to_owned()),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    /// The ghost a drag carries: the app's icon on a card, ringed in primary
    /// over a landing place, in error with a minus badge where it would be
    /// unpinned.
    pub(super) fn deck_ghost_layer(&self) -> Option<gpui::Div> {
        use crate::reference_bar::{
            COLOR_ERROR, COLOR_ON_SURFACE, COLOR_PRIMARY, COLOR_SURFACE_CONTAINER,
            COLOR_SURFACE_CONTAINER_HIGHEST,
        };
        let drag = self.deck_drag.as_ref()?;
        let ghost = drag.ghost();
        let icon = ghost.width * 0.75;
        let glyph = match &drag.icon {
            Some(path) => gpui::img(path.clone()).size(px(icon)).into_any_element(),
            None => div()
                .size(px(icon))
                .rounded(px(icon * 0.28))
                .bg(rgba(COLOR_SURFACE_CONTAINER_HIGHEST))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(icon * 0.45))
                .text_color(rgba(COLOR_ON_SURFACE))
                .child(
                    drag.name
                        .chars()
                        .find(|c| c.is_alphanumeric())
                        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect()),
                )
                .into_any_element(),
        };
        let mut card = div()
            .absolute()
            .left(px(ghost.x))
            .top(px(ghost.y))
            .size(px(ghost.width))
            .rounded(px(ghost.width * 0.3))
            .bg(rgba(COLOR_SURFACE_CONTAINER))
            .flex()
            .items_center()
            .justify_center()
            .child(glyph);
        match drag.target() {
            DropTarget::Insert(_) => card = card.border_2().border_color(rgba(COLOR_PRIMARY)),
            DropTarget::Remove => {
                card = card.border_2().border_color(rgba(COLOR_ERROR)).child(
                    div()
                        .absolute()
                        .top(px(-6.0))
                        .right(px(-6.0))
                        .size(px(18.0))
                        .rounded_full()
                        .bg(rgba(COLOR_ERROR))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(14.0))
                        .text_color(rgba(0xffffffff))
                        .child("\u{2212}"),
                );
            }
            DropTarget::Launch => card = card.border_2().border_color(rgba(COLOR_PRIMARY)),
            DropTarget::Cancel => {}
        }
        // DECK-04: while a pinned card is carried, the unpin target stands
        // beyond the Deck; it fills in error when the drop would land on it.
        let unpin = (drag.source == DragSource::Card { pinned: true })
            .then(|| self.deck_shape.as_ref().map(unpin_target))
            .flatten()
            .map(|r| {
                let over = drag.target() == DropTarget::Remove;
                let (fill, ink) = if over {
                    (COLOR_ERROR, 0xffffffff)
                } else {
                    (COLOR_SURFACE_CONTAINER, COLOR_ERROR)
                };
                div()
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y))
                    .size(px(r.width))
                    .rounded_full()
                    .bg(rgba(fill))
                    .border_2()
                    .border_color(rgba(COLOR_ERROR))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(26.0))
                    .text_color(rgba(ink))
                    .child("\u{2212}")
                    .child(
                        div()
                            .absolute()
                            .top(px(r.height + 6.0))
                            .left(px(-60.0))
                            .w(px(r.width + 120.0))
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(3.0))
                                    .rounded_full()
                                    .bg(rgba(COLOR_SURFACE_CONTAINER))
                                    .text_size(px(12.0))
                                    .text_color(rgba(COLOR_ON_SURFACE))
                                    .child(gnos_i18n::t("Unpin from the Deck")),
                            ),
                    )
            });
        // In scene coordinates, as the other layers.
        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .child(card)
                .children(unpin),
        )
    }

    /// DECK-01F (B10): whether a writable durable profile owns the pins.
    #[cfg(feature = "r12-production-client")]
    fn deck_pins_profiled(&self, cx: &mut Context<Self>) -> bool {
        self.review_native_rail
            && crate::product_profile::ProfileSession::shared(cx)
                .read(cx)
                .snapshot
                .is_some()
    }

    /// Send pins to the profile authority (the `pins` override, written
    /// over the profile's own overrides).
    #[cfg(feature = "r12-production-client")]
    fn write_deck_pins(&mut self, pins: &[String], cx: &mut Context<Self>) -> &'static str {
        let Ok(list) = in_gnu_os_settings_read_model::DeckPinList::from_ids(pins.iter().cloned())
        else {
            return "INVALID";
        };
        let session = crate::product_profile::ProfileSession::shared(cx);
        let Some(mut overrides) = session
            .read(cx)
            .snapshot
            .as_ref()
            .map(|s| s.preferences.clone())
        else {
            return "NO_PROFILE";
        };
        overrides.pins = Some(list);
        session.update(cx, |session, cx| session.select_preferences(overrides, cx));
        "SENT"
    }

    /// DECK-01F (B10): the profile's pins are the Deck's. The first time a
    /// profile without pins is seen, the pins of `deck.json` are offered to
    /// it (the valid, distinct ones, at most 32); `deck.json` itself is
    /// never rewritten or read again while a profile owns the pins.
    #[cfg(feature = "r12-production-client")]
    pub(super) fn sync_deck_pins(&mut self, cx: &mut Context<Self>) {
        const PENDING: Duration = Duration::from_secs(3);
        if !self.deck_pins_profiled(cx) {
            return;
        }
        let profile: Option<Vec<String>> = crate::product_profile::ProfileSession::shared(cx)
            .read(cx)
            .snapshot
            .as_ref()
            .and_then(|s| s.preferences.pins.as_ref())
            .map(|list| list.ids().iter().map(|id| id.as_str().to_owned()).collect());
        if let Some((pending, since)) = &self.deck_pins_pending {
            if profile.as_ref() == Some(pending) {
                self.deck_pins_pending = None;
            } else if since.elapsed() > PENDING {
                println!("GNOS_UX_DECK_PINS status=reverted reason=profile_unchanged");
                self.deck_pins_pending = None;
            } else {
                return;
            }
        }
        match profile {
            Some(ids) => {
                self.deck_pins_imported = true;
                if ids != self.deck_pins.pinned {
                    println!(
                        "GNOS_UX_DECK_PINS status=profile count={} pins=[{}]",
                        ids.len(),
                        ids.join(";")
                    );
                    self.deck_pins.pinned = ids;
                }
            }
            None if !self.deck_pins_imported => {
                self.deck_pins_imported = true;
                let mut ids: Vec<String> = Vec::new();
                for id in &self.deck_pins.pinned {
                    let valid =
                        in_gnu_os_settings_read_model::DeckPinId::try_from(id.clone()).is_ok();
                    if valid
                        && !ids.contains(id)
                        && ids.len() < in_gnu_os_settings_read_model::DeckPinList::MAX
                    {
                        ids.push(id.clone());
                    }
                }
                let dropped = self.deck_pins.pinned.len() - ids.len();
                if ids.is_empty() {
                    return;
                }
                let outcome = self.write_deck_pins(&ids, cx);
                if outcome == "SENT" {
                    self.deck_pins_pending = Some((ids.clone(), Instant::now()));
                    self.deck_pins.pinned = ids.clone();
                }
                println!(
                    "GNOS_UX_DECK_PINS status=importing source=deck.json count={} dropped={dropped} outcome={outcome}",
                    ids.len()
                );
            }
            None => {}
        }
    }
}
