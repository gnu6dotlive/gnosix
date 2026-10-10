//! GFC-02A native GPUI renderer; invoked ONLY by its parent via private pipes.
//! The first opt-in slice implements local OpenFile/OpenDirectory without filters.
//! There is intentionally no GTK, Qt, filesystem mutation, or D-Bus authority.
use gpui::{
    div, prelude::*, px, rgb, size, App, AppContext, Bounds, Context, FocusHandle, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use in_gnu_os_filechooser_portal::cabinet_choices::CabinetChoice;
use in_gnu_os_filechooser_portal::cabinet_fs::{scan_directory_filtered, CabinetEntry};
use in_gnu_os_filechooser_portal::render_wire::{RenderRequest, RenderResponse, MAX_WIRE_BYTES};
use in_gnu_os_filechooser_portal::PickerKind;
use std::{
    collections::BTreeSet,
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

struct PickerView {
    request: RenderRequest,
    folder: PathBuf,
    filename: String,
    entries: Vec<CabinetEntry>,
    scanning: bool,
    truncated: bool,
    scan_generation: u64,
    current_index: Option<usize>,
    selected_filter: Option<usize>,
    choices: Vec<CabinetChoice>,
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
            filename: request.filename.clone(),
            selected_filter: request.selected_filter,
            choices: request.choices.clone(),
            request,
            folder,
            entries: Vec::new(),
            scanning: false,
            truncated: false,
            scan_generation: 0,
            current_index: None,
            selected: BTreeSet::new(),
            error: None,
            focus: cx.focus_handle(),
            outcome,
        };
        view.start_scan(cx);
        view
    }

    /// File metadata scanning is never performed on the GPUI UI thread.
    /// Navigating again invalidates late results through an epoch check.
    fn start_scan(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.selected.clear();
        self.current_index = None;
        self.scanning = true;
        self.truncated = false;
        self.error = None;
        self.scan_generation = self.scan_generation.wrapping_add(1);
        let scan_generation = self.scan_generation;
        let folder = self.folder.clone();
        let filter = self
            .selected_filter
            .and_then(|index| self.request.filters.get(index))
            .cloned();
        let work = cx
            .background_executor()
            .spawn(async move { scan_directory_filtered(&folder, filter.as_ref()) });
        cx.spawn(async move |this, cx| {
            let listing = work.await;
            let _ = this.update(cx, |view, cx| {
                if view.scan_generation != scan_generation {
                    return;
                }
                view.scanning = false;
                match listing {
                    Ok(listing) => {
                        view.entries = listing.entries;
                        view.truncated = listing.truncated;
                    }
                    Err(error) => view.error = Some(format!("Directory unavailable: {error}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn cycle_filter(&mut self, cx: &mut Context<Self>) {
        if self.request.filters.is_empty() {
            return;
        }
        let next = self
            .selected_filter
            .map_or(0, |i| (i + 1) % self.request.filters.len());
        self.selected_filter = Some(next);
        self.start_scan(cx);
    }

    fn cycle_choice(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(choice) = self.choices.get_mut(index) {
            choice.cycle();
            cx.notify();
        }
    }

    fn move_cursor(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.entries.is_empty() || self.scanning {
            return;
        }
        let last = self.entries.len() - 1;
        self.current_index = Some(match (self.current_index, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(last),
            (Some(i), false) => i.saturating_sub(1),
        });
        cx.notify();
    }

    fn activate_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.current_index else {
            return;
        };
        if let Some(entry) = self.entries.get(index).cloned() {
            if !entry.folder && !self.request.multiple && self.selected.contains(&entry.path) {
                self.confirm(cx);
            } else {
                self.click_entry(&entry, cx);
            }
        }
    }

    fn click_entry(&mut self, entry: &CabinetEntry, cx: &mut Context<Self>) {
        if entry.folder {
            self.folder = entry.path.clone();
            self.start_scan(cx);
        } else if self.request.kind == PickerKind::SaveFile {
            self.filename = entry.label.clone();
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
            self.start_scan(cx);
        }
    }

    fn complete(&mut self, response: RenderResponse, cx: &mut Context<Self>) {
        if let Ok(mut result) = self.outcome.lock() {
            *result = response;
        }
        cx.quit();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let paths = match self.request.kind {
            PickerKind::SaveFile => {
                let name = std::path::Path::new(&self.filename);
                if !matches!(
                    name.components().next(),
                    Some(std::path::Component::Normal(_))
                ) || name.components().count() != 1
                {
                    self.error = Some("Enter a valid file name (no paths)".into());
                    cx.notify();
                    return;
                }
                vec![self.folder.join(name)]
            }
            PickerKind::SaveFiles => vec![self.folder.clone()],
            PickerKind::OpenFile if self.request.directory => vec![self.folder.clone()],
            PickerKind::OpenFile => self.selected.iter().cloned().collect(),
        };
        if self.request.validate_selection(&paths) {
            let response = if !self.choices.is_empty() {
                RenderResponse::SelectedWithMetadata {
                    paths,
                    filter_index: self.selected_filter,
                    choices: self
                        .choices
                        .iter()
                        .map(|item| (item.id.clone(), item.selected.clone()))
                        .collect(),
                }
            } else {
                match self.selected_filter {
                    Some(filter_index) => RenderResponse::SelectedWithFilter {
                        paths,
                        filter_index,
                    },
                    None => RenderResponse::Selected(paths),
                }
            };
            self.complete(response, cx);
        } else {
            self.error =
                Some("Invalid selection or existing destination; overwrite not supported".into());
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
        let save_file = self.request.kind == PickerKind::SaveFile;
        let save_files = self.request.kind == PickerKind::SaveFiles;
        let filename = self.filename.clone();
        let files_count = self.request.requested_files.len();
        let accept_label = if !self.request.accept_label.is_empty() {
            self.request.accept_label.replace('_', "")
        } else if save_file {
            "Save".to_string()
        } else if directory {
            "Choose folder".to_string()
        } else {
            "Open".to_string()
        };
        let scanning = self.scanning;
        let truncated = self.truncated;
        let current_index = self.current_index;
        let filter_text = self
            .selected_filter
            .and_then(|index| self.request.filters.get(index))
            .map(|filter| format!("Filter: {}  ·  click / F6 to switch", filter.name));
        let mut choice_panel = div()
            .id("cabinet-choices")
            .w_full()
            .flex()
            .flex_col()
            .gap_1();
        for (index, item) in self.choices.iter().enumerate() {
            let label = format!(
                "{}: {}  ·  click to change",
                item.label,
                item.selection_label()
            );
            choice_panel = choice_panel.child(
                div()
                    .id(format!("cabinet-choice-{index}"))
                    .p_2()
                    .rounded_sm()
                    .bg(rgb(PANEL))
                    .text_color(rgb(ORANGE))
                    .text_size(px(12.))
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.cycle_choice(index, cx))),
            );
        }
        let mut list = div().id("gfc-file-rows").w_full().flex().flex_col().gap_1();

        if scanning {
            list = list.child(div().text_color(rgb(MUTED)).child("Loading directory…"));
        } else if self.entries.is_empty() {
            list = list.child(
                div()
                    .text_color(rgb(MUTED))
                    .child("No files or folders found"),
            );
        }
        for (index, entry) in self.entries.iter().cloned().enumerate() {
            let label = entry.label.clone();
            let marker = if entry.folder { "▸" } else { "•" };
            let picked = self.selected.contains(&entry.path);
            let focused = current_index == Some(index);
            let bg = if picked {
                0x25445a
            } else if focused {
                0x283748
            } else {
                PANEL
            };
            list = list.child(
                div()
                    .id(format!("gfc-file-{index}"))
                    .w_full()
                    .p_2()
                    .rounded_sm()
                    .role(gpui::Role::Button)
                    .aria_label(if entry.folder {
                        format!("Open folder {label}")
                    } else {
                        format!("Select file {label}")
                    })
                    .bg(rgb(bg))
                    .text_color(rgb(if picked { ORANGE } else { TEXT }))
                    .child(format!("{marker}   {label}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.current_index = Some(index);
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
                match event.keystroke.key.as_str() {
                    "f6" => this.cycle_filter(cx),
                    "escape" => this.complete(RenderResponse::Cancelled, cx),
                    "backspace" if this.request.kind == PickerKind::SaveFile => {
                        this.filename.pop();
                        this.error = None;
                        cx.notify();
                    }
                    "backspace" => this.navigate_up(cx),
                    "up" => this.move_cursor(false, cx),
                    "down" => this.move_cursor(true, cx),
                    "enter" if this.request.kind == PickerKind::SaveFile => this.confirm(cx),
                    "enter" if this.current_index.is_some() => this.activate_cursor(cx),
                    _ if this.request.kind == PickerKind::SaveFile
                        && !event.keystroke.modifiers.control
                        && !event.keystroke.modifiers.alt
                        && !event.keystroke.modifiers.platform =>
                    {
                        // Provisional keyboard path only; full text/IME composition
                        // must be qualified before exposing to real applications.
                        if let Some(value) = &event.keystroke.key_char {
                            if !value.contains('/') && !value.contains(' ')
                                && value.chars().all(|ch| !ch.is_control())
                                && this.filename.len() + value.len() <= 240
                            {
                                this.filename.push_str(value);
                                this.error = None;
                                cx.notify();
                            }
                        }
                    }
                    _ => {}
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
            .when_some(filter_text, |panel, label| {
                panel.child(
                    div()
                        .id("gfc-filter-cycle")
                        .px_3()
                        .py_2()
                        .bg(rgb(PANEL))
                        .text_color(rgb(ORANGE))
                        .text_size(px(12.))
                        .child(label)
                        .on_click(cx.listener(|this, _, _, cx| this.cycle_filter(cx))),
                )
            })
            .child(choice_panel)
            .child(
                div()
                    .id("gfc-files")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
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
                    .child(if save_file {
                        format!("Save name: {filename}  (type to edit · Backspace to delete · Enter to confirm)")
                    } else if save_files {
                        format!("Choose folder for {files_count} file(s); existing names are rejected")
                    } else if directory {
                        "Choose this directory".to_string()
                    } else {
                        format!("{chosen} selected")
                    }),
            )
            .when(truncated, |panel| {
                panel.child(
                    div()
                        .text_color(rgb(ORANGE))
                        .text_size(px(12.))
                        .child("Only the first 4096 entries are listed"),
                )
            })
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
                            .child(accept_label)
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
