//! One-line IME-aware search field for AppStack.
//!
//! This is intentionally not a generic text editor. It owns only the editing
//! mechanics required to produce a search query; application execution remains
//! outside this component.

use std::ops::Range;

use gpui::{
    App, Bounds, Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, InteractiveElement, LayoutId, MouseButton,
    MouseDownEvent, PaintQuad, Pixels, ShapedLine, SharedString, Style, Styled, TextRun,
    UTF16Selection, Window, div, fill, point, prelude::*, px, size,
};

use crate::product_design::{
    ProductDesignContext, ProductPalette, ProductRadius, ProductSpacing, ProductStroke,
};

pub struct AppStackSearchInput {
    focus_handle: FocusHandle,
    /// The query being edited (`gnos-text`: graphemes, UTF-16, IME marks).
    text: gnos_text::TextBuffer,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    design: ProductDesignContext,
    /// No border or fill: the host draws the field (the Deck's slot).
    bare: bool,
    /// Left and Right belong to the host's grid (DECK-01C).
    grid_arrows: bool,
}

impl AppStackSearchInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: gnos_text::TextBuffer::new(gnos_text::Admission::LINE, ""),
            last_layout: None,
            last_bounds: None,
            design: ProductDesignContext::default(),
            bare: false,
            grid_arrows: false,
        }
    }

    pub fn query(&self) -> &str {
        self.text.text()
    }

    pub fn set_focus_handle(&mut self, handle: FocusHandle) {
        self.focus_handle = handle;
    }

    pub fn set_design(&mut self, design: ProductDesignContext) {
        self.design = design;
    }

    pub fn set_bare(&mut self, bare: bool) {
        self.bare = bare;
    }

    /// DECK-01C: in a grid, Left and Right move between tiles, so the field
    /// lets them through instead of moving its caret (Home and End still
    /// do).
    pub fn set_grid_arrows(&mut self, grid_arrows: bool) {
        self.grid_arrows = grid_arrows;
    }

    /// DECK-02 (A03): a chip rewrote the query's mode prefix; the caret
    /// goes to the end.
    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.text.text() == query {
            return;
        }
        self.text.set_text(&one_line(&query));
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.text.text().is_empty() {
            return;
        }
        self.text.set_text("");
        cx.notify();
    }

    fn on_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "backspace" => {
                if self.text.caret() > 0 && self.text.backspace() {
                    cx.notify();
                } else {
                    window.play_system_bell();
                }
                cx.stop_propagation();
            }
            "delete" => {
                if self.text.caret() < self.text.text().len() && self.text.delete_forward() {
                    cx.notify();
                } else {
                    window.play_system_bell();
                }
                cx.stop_propagation();
            }

            "left" | "right" if self.grid_arrows => {}
            "left" => {
                self.text.clear_mark();
                self.text.left(false);
                cx.notify();
                cx.stop_propagation();
            }
            "right" => {
                self.text.clear_mark();
                self.text.right(false);
                cx.notify();
                cx.stop_propagation();
            }
            "home" => {
                self.text.clear_mark();
                self.text.home();
                cx.notify();
                cx.stop_propagation();
            }
            "end" => {
                self.text.clear_mark();
                self.text.end();
                cx.notify();
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.text.clear_mark();
        self.text.end();
        cx.notify();
        cx.stop_propagation();
    }
}

/// A paste or a composition never breaks the one-line query: line breaks
/// become spaces, as before.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

impl Focusable for AppStackSearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for AppStackSearchInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.text.range_from_utf16(range_utf16);
        actual_range.replace(self.text.utf16_at(range.start)..self.text.utf16_at(range.end));
        Some(self.text.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.text.selected_utf16(),
            reversed: self.text.selection_reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.text
            .mark()
            .map(|range| self.text.utf16_at(range.start)..self.text.utf16_at(range.end))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.text.clear_mark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text.replace(range_utf16, &one_line(new_text), false) {
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text.replace(range_utf16, &one_line(new_text), true) {
            if let Some(selection) = new_selected_range_utf16 {
                self.text.set_mark_selection(selection);
            }
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.text.range_from_utf16(range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let local = bounds.localize(&point)?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(local.x)?;
        Some(self.text.utf16_at(index))
    }
}

struct SearchTextElement {
    input: Entity<AppStackSearchInput>,
}

struct SearchPrepaint {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
}

impl gpui::IntoElement for SearchTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SearchTextElement {
    type RequestLayoutState = ();
    type PrepaintState = SearchPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let display: SharedString = if input.text.text().is_empty() {
            gnos_i18n::t("Search").into()
        } else {
            input.text.text().to_owned().into()
        };
        let color = if input.text.text().is_empty() {
            ProductPalette::text_muted()
        } else {
            ProductPalette::text_primary()
        };

        let run = TextRun {
            len: display.len(),
            font: window.text_style().font(),
            color: color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display, font_size, &[run], None);
        let cursor = if input.text.text().is_empty() {
            0
        } else {
            input.text.caret()
        };
        let cursor = input.focus_handle.is_focused(window).then(|| {
            let x = line.x_for_index(cursor);
            fill(
                Bounds::new(
                    point(bounds.left() + x, bounds.top()),
                    size(px(1.5), bounds.bottom() - bounds.top()),
                ),
                ProductPalette::accent(),
            )
        });
        SearchPrepaint { line, cursor }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        prepaint
            .line
            .paint(
                bounds.origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .expect("AppStack search text must paint");
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        let line = prepaint.line.clone();
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl gpui::Render for AppStackSearchInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let design = self.design;
        let field = div()
            .id("product-appstack-search")
            .role(gpui::Role::TextInput)
            .aria_label("Rechercher une application")
            .track_focus(&self.focus_handle(cx))
            .tab_stop(true)
            .w_full()
            .flex()
            .items_center();
        let field = if self.bare {
            field.h_full()
        } else {
            field
                .h(px(design.scaled_px(36.0)))
                .px(px(ProductSpacing::SM))
                .rounded(px(ProductRadius::CONTROL))
                .border(px(ProductStroke::STANDARD))
                .border_color(ProductPalette::border())
                .bg(ProductPalette::control())
        };
        field
            .text_size(px(design.scaled_px(14.0)))
            .line_height(px(design.scaled_px(20.0)))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            // CURSOR-01: text is typed here.
            .cursor(gpui::CursorStyle::IBeam)
            .child(SearchTextElement { input: cx.entity() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> gnos_text::TextBuffer {
        gnos_text::TextBuffer::new(gnos_text::Admission::LINE, text)
    }

    #[test]
    fn utf16_and_utf8_offsets_round_trip_unicode_boundaries() {
        let text = query("é🙂z");
        let boundaries = [
            (0, 0),
            ("é".len(), 1),
            ("é🙂".len(), 3),
            (text.text().len(), 4),
        ];
        for (utf8, utf16) in boundaries {
            assert_eq!(text.utf16_at(utf8), utf16);
            assert_eq!(text.byte_at_utf16(utf16), utf8);
        }
    }

    #[test]
    fn a_composition_replaces_its_marked_text() {
        let mut text = query("fire");
        assert!(text.replace(None, "é", true));
        assert!(text.replace(None, "é🙂", true));
        assert_eq!(text.text(), "fireé🙂");
        assert_eq!(text.mark(), Some("fire".len().."fireé🙂".len()));
    }

    #[test]
    fn newline_normalization_keeps_search_single_line() {
        assert_eq!(one_line("fire\nfox\rnightly"), "fire fox nightly");
        let mut text = query("");
        assert!(text.replace(None, &one_line("fire\nfox"), false));
        assert_eq!(text.text(), "fire fox");
    }

    #[test]
    fn backspace_takes_a_whole_grapheme() {
        let mut text = query("ok👨‍👩‍👧");
        assert!(text.backspace());
        assert_eq!(text.text(), "ok");
    }
}
