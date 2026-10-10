//! Native GPUI text-field bridge for Cabinet SaveFile.
//! Text arrives from the platform through EntityInputHandler rather than key_char.
use gpui::{
    canvas, fill, point, prelude::*, px, rgb, size, Bounds, ClipboardItem, Context, CursorStyle,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, KeyDownEvent, Pixels, ShapedLine,
    SharedString, TextRun, UTF16Selection, Window,
};
use in_gnu_os_filechooser_portal::filename_text::FilenameBuffer;
use std::ops::Range;

const TEXT: u32 = 0xecf1f5;
const ORANGE: u32 = 0xff8733;

pub struct FilenameInput {
    pub focus: FocusHandle,
    buffer: FilenameBuffer,
    last_line: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl FilenameInput {
    pub fn new(filename: String, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            buffer: FilenameBuffer::new(filename),
            last_line: None,
            last_bounds: None,
        }
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    pub fn set_text(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.buffer.set_text(name) {
            cx.notify();
        }
    }

    pub fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        let shortcut = mods.control || mods.platform;
        if shortcut {
            match key {
                "a" => self.buffer.select_all(),
                "c" => {
                    let range = self.buffer.selection();
                    if !range.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(self.text()[range].into()));
                    }
                }
                "x" => {
                    let range = self.buffer.selection();
                    if !range.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(self.text()[range].into()));
                        self.buffer.replace(None, "", false);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.buffer.replace(None, &text, false);
                    }
                }
                _ => return,
            }
        } else {
            match key {
                "backspace" => {
                    self.buffer.backspace();
                }
                "delete" => {
                    self.buffer.delete_forward();
                }
                "left" => self.buffer.left(mods.shift),
                "right" => self.buffer.right(mods.shift),
                "home" => self.buffer.home(),
                "end" => self.buffer.end(),
                _ => return, // printable text is exclusively handled by GPUI's native input handler
            }
        }
        if !self.focus.is_focused(window) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
}

impl EntityInputHandler for FilenameInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let bytes = self.buffer.range_from_utf16(range);
        *adjusted = Some(self.buffer.utf16_at(bytes.start)..self.buffer.utf16_at(bytes.end));
        Some(self.text()[bytes].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.selected_utf16(),
            reversed: self.buffer.selection_reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .mark()
            .map(|range| self.buffer.utf16_at(range.start)..self.buffer.utf16_at(range.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.clear_mark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.buffer.replace(range, text, false) {
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.buffer.replace(range, text, true) {
            if let Some(range) = selected {
                self.buffer.set_mark_selection(range);
            }
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_line.as_ref()?;
        let bytes = self.buffer.range_from_utf16(range);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(bytes.start), bounds.top()),
            point(bounds.left() + line.x_for_index(bytes.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_line.as_ref()?;
        let byte = line.closest_index_for_x(point.x - bounds.left());
        Some(self.buffer.utf16_at(byte))
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.set_selection_utf16(range);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.buffer.len_utf16())
    }

    fn text_input_editable_range(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        Some(0..self.buffer.len_utf16())
    }
}

pub fn editor_canvas(input: Entity<FilenameInput>) -> impl IntoElement {
    let pre = input.clone();
    let paint = input.clone();
    canvas(
        move |_, window, cx| {
            let editor = pre.read(cx);
            let text = editor.text();
            let placeholder = text.is_empty();
            let display: SharedString = if placeholder {
                "Enter filename…".into()
            } else {
                text.to_string().into()
            };
            let style = window.text_style();
            let run = TextRun {
                len: display.len(),
                font: style.font(),
                color: if placeholder {
                    gpui::hsla(0., 0., 0.65, 1.)
                } else {
                    style.color
                },
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                display,
                style.font_size.to_pixels(window.rem_size()),
                &[run],
                None,
            );
            let selection = editor.buffer.selection();
            let mark = editor.buffer.mark();
            let cursor = selection.end;
            (line, selection, mark, cursor)
        },
        move |bounds, (line, selection, mark, cursor), window, cx| {
            let focus = paint.read(cx).focus.clone();
            window.handle_input(&focus, ElementInputHandler::new(bounds, paint.clone()), cx);
            if let Some(range) = mark {
                if !range.is_empty() {
                    window.paint_quad(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + line.x_for_index(range.start),
                                bounds.bottom() - px(2.),
                            ),
                            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
                        ),
                        rgb(ORANGE),
                    ));
                }
            }
            if !selection.is_empty() {
                window.paint_quad(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selection.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selection.end),
                            bounds.bottom(),
                        ),
                    ),
                    gpui::rgba(0x3a81c07f),
                ));
            }
            line.paint(
                bounds.origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .expect("GPUI native filename text paint");
            if focus.is_focused(window) {
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.left() + line.x_for_index(cursor), bounds.top()),
                        size(px(2.), bounds.size.height),
                    ),
                    rgb(ORANGE),
                ));
            }
            paint.update(cx, |editor, _| {
                editor.last_line = Some(line);
                editor.last_bounds = Some(bounds);
            });
        },
    )
    .w_full()
    .h(px(26.))
    .text_color(rgb(TEXT))
    .cursor(CursorStyle::IBeam)
}
