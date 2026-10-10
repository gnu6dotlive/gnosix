//! GFC-02A native GPUI renderer; invoked ONLY by its parent via private pipes.
//! The first opt-in slice implements local OpenFile/OpenDirectory without filters.
//! There is intentionally no GTK, Qt, filesystem mutation, or D-Bus authority.
use gpui::{
    div, prelude::*, px, rgb, size, App, AppContext, Bounds, Context, FocusHandle, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use in_gnu_os_filechooser_portal::render_wire::{RenderRequest, RenderResponse, MAX_WIRE_BYTES};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

const BG: u32 = 0x10151d;
const PANEL: u32 = 0x182331;
const BORDER: u32 = 0x344355;
const TEXT: u32 = 0xecf1f5;
const MUTED: u32 = 0xa5b2bf;
const ORANGE: u32 = 0xff8733;

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    label: String,
    folder: bool,
}

struct PickerView {
    request: RenderRequest,
    folder: PathBuf,
    entries: Vec<Entry>,
    selected: BTreeSet<PathBuf>,
    error: Option<String>,
    focus: FocusHandle,
    outcome: Arc<Mutex<RenderResponse>>,
}

impl PickerView {
    fn new(
        request: RenderRequest,
        outcome: Arc<Mutex<RenderResponse>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let folder = request.initial_folder.clone();
        let mut view = Self {
            request,
            folder,
            entries: Vec::new(),
            selected: BTreeSet::new(),
            error: None,
            focus: cx.focus_handle(),
            outcome,
        };
        view.reload();
        view
    }

    fn reload(&mut self) {
        self.entries.clear();
        self.selected.clear();
        self.error = None;
        let dirs = match fs::read_dir(&self.folder) {
            Ok(entries) => entries,
            Err(error) => {
                self.error = Some(format!("Directory unavailable: {error}"));
                return;
            }
        };
        for candidate in dirs.take(4097) {
            let Ok(candidate) = candidate else { continue };
            let path = candidate.path();
            let Ok(info) = fs::metadata(&path) else {
                continue;
            };
            if !info.is_file() && !info.is_dir() {
                continue;
            }
            self.entries.push(Entry {
                label: candidate.file_name().to_string_lossy().into_owned(),
                path,
                folder: info.is_dir(),
            });
        }
        self.entries.sort_by(|a, b| {
            b.folder
                .cmp(&a.folder)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });
        if self.entries.len() > 4096 {
            self.entries.truncate(4096);
            self.error = Some("Showing the first 4096 entries only".into());
        }
    }

    fn click_entry(&mut self, entry: &Entry, cx: &mut Context<Self>) {
        if entry.folder {
            self.folder = entry.path.clone();
            self.reload();
        } else if !self.request.directory {
            if self.request.multiple {
                if !self.selected.insert(entry.path.clone()) {
                    self.selected.remove(&entry.path);
                }
            } else {
                self.selected.clear();
                self.selected.insert(entry.path.clone());
            }
        }
        cx.notify();
    }

    fn navigate_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.folder.parent() {
            self.folder = parent.to_path_buf();
            self.reload();
            cx.notify();
        }
    }

    fn complete(&mut self, response: RenderResponse, cx: &mut Context<Self>) {
        if let Ok(mut result) = self.outcome.lock() {
            *result = response;
        }
        cx.quit();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let paths = if self.request.directory {
            vec![self.folder.clone()]
        } else {
            self.selected.iter().cloned().collect()
        };
        if self.request.validate_selection(&paths) {
            self.complete(RenderResponse::Selected(paths), cx);
        } else {
            self.error = Some("Select a valid item first".into());
            cx.notify();
        }
    }
}

impl Render for PickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.request.title.clone();
        let source = if self.request.app_id.is_empty() {
            "Unknown caller".to_string()
        } else {
            self.request.app_id.clone()
        };
        let folder = self.folder.to_string_lossy().to_string();
        let chosen = self.selected.len();
        let directory = self.request.directory;
        let mut list = div()
            .id("gfc-files")
            .flex_1()
            .w_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();

        for (index, entry) in self.entries.iter().cloned().enumerate() {
            let label = entry.label.clone();
            let marker = if entry.folder { "▸" } else { "•" };
            let picked = self.selected.contains(&entry.path);
            let bg = if picked { 0x25445a } else { PANEL };
            list = list.child(
                div()
                    .id(format!("gfc-file-{index}"))
                    .w_full()
                    .p_2()
                    .rounded_sm()
                    .bg(rgb(bg))
                    .text_color(rgb(if picked { ORANGE } else { TEXT }))
                    .child(format!("{marker}   {label}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.click_entry(&entry, cx);
                    })),
            );
        }

        div()
            .id("gnosix-filechooser")
            .role(gpui::Role::Application)
            .aria_label("Cabinet — Gnosix file selection")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    this.complete(RenderResponse::Cancelled, cx);
                }
            }))
            .size_full()
            .p_4()
            .gap_3()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(ORANGE))
                    .child("CABINET  /  GNOSIX"),
            )
            .child(div().text_size(px(22.)).child(title))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(format!("Caller: {source}")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("gfc-up")
                            .px_3()
                            .py_2()
                            .bg(rgb(PANEL))
                            .rounded_sm()
                            .child("↑  Up")
                            .on_click(cx.listener(|this, _, _, cx| this.navigate_up(cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(folder),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PANEL))
                    .rounded_md()
                    .child(list),
            )
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(12.))
                    .child(if directory {
                        "Choose this directory".to_string()
                    } else {
                        format!("{chosen} selected")
                    }),
            )
            .when_some(self.error.clone(), |panel, error| {
                panel.child(div().text_color(rgb(ORANGE)).child(error))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_3()
                    .child(
                        div()
                            .id("gfc-cancel")
                            .px_4()
                            .py_2()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .rounded_sm()
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete(RenderResponse::Cancelled, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("gfc-confirm")
                            .px_4()
                            .py_2()
                            .rounded_sm()
                            .bg(rgb(ORANGE))
                            .text_color(rgb(BG))
                            .child(if directory { "Choose folder" } else { "Open" })
                            .on_click(cx.listener(|this, _, _, cx| this.confirm(cx))),
                    ),
            )
    }
}

fn read_request() -> Result<RenderRequest, Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    io::stdin()
        .take((MAX_WIRE_BYTES + 1) as u64)
        .read_to_end(&mut input)?;
    if input.len() > MAX_WIRE_BYTES {
        return Err("request exceeds the size limit".into());
    }
    Ok(serde_json::from_slice(&input)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = read_request()?;
    if !request.initial_folder.is_dir() || !request.initial_folder.is_absolute() {
        return Err("invalid initial directory".into());
    }
    let outcome = Arc::new(Mutex::new(RenderResponse::Cancelled));
    let child_outcome = outcome.clone();

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(830.), px(620.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                app_id: Some("in.gnu.os.cabinet".into()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| PickerView::new(request, child_outcome, cx));
                view.update(cx, |view, cx| view.focus.focus(window, cx));
                view
            },
        )
        .expect("native chooser window");
    });

    let response = outcome
        .lock()
        .map_err(|_| "renderer response lock poisoned")?;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &*response)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}
