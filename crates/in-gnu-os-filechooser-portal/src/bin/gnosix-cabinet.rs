//! Cabinet GFC-03C native GPUI picker, only launched by the private portal bridge.
//! Open/Save, typed choices and filters, native GPUI input/IME hooks. No GTK, Qt
//! or filesystem mutation. No host portal installation or public IPC authority.
#[path = "../cabinet_ime.rs"]
mod cabinet_ime;
use cabinet_ime::FilenameInput;
use gpui::{
    div, prelude::*, px, rgb, size, Animation, AnimationExt as _, App, AppContext, Bounds, Context,
    Entity, FocusHandle, Render, SpringAnimation, SpringConfig, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions,
};
use in_gnu_os_filechooser_portal::cabinet_choices::CabinetChoice;
use in_gnu_os_filechooser_portal::cabinet_fs::{scan_directory_filtered, CabinetEntry};
use in_gnu_os_filechooser_portal::cabinet_navigation::{
    query_matches, quick_locations, CabinetNavigation,
};
use in_gnu_os_filechooser_portal::render_wire::{RenderRequest, RenderResponse, MAX_WIRE_BYTES};
use in_gnu_os_filechooser_portal::PickerKind;
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const BG: u32 = 0x10151d;
const PANEL: u32 = 0x182331;
const BORDER: u32 = 0x344355;
const TEXT: u32 = 0xecf1f5;
const MUTED: u32 = 0xa5b2bf;
const ORANGE: u32 = 0xff8733;

struct PickerView {
    request: RenderRequest,
    navigation: CabinetNavigation,
    filename_input: Entity<FilenameInput>,
    _filename_observer: gpui::Subscription,
    search_input: Entity<FilenameInput>,
    _search_observer: gpui::Subscription,
    show_hidden: bool,
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
    file_list_focus: FocusHandle,
    outcome: Arc<Mutex<RenderResponse>>,
}

impl PickerView {
    fn new(
        request: RenderRequest,
        outcome: Arc<Mutex<RenderResponse>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let navigation = CabinetNavigation::new(request.initial_folder.clone());
        let filename_input = cx.new(|cx| FilenameInput::new(request.filename.clone(), cx));
        let filename_observer = cx.observe(&filename_input, |_view, _editor, cx| cx.notify());
        let search_input = cx.new(|cx| {
            FilenameInput::with_placeholder(
                String::new(),
                "Search this folder…   Ctrl+F".to_string(),
                cx,
            )
        });
        let search_observer = cx.observe(&search_input, |_view, _editor, cx| cx.notify());
        let mut view = Self {
            filename_input,
            _filename_observer: filename_observer,
            search_input,
            _search_observer: search_observer,
            show_hidden: false,
            selected_filter: request.selected_filter,
            choices: request.choices.clone(),
            request,
            navigation,
            entries: Vec::new(),
            scanning: false,
            truncated: false,
            scan_generation: 0,
            current_index: None,
            selected: BTreeSet::new(),
            error: None,
            focus: cx.focus_handle(),
            file_list_focus: cx.focus_handle(),
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
        let folder = self.navigation.current().to_path_buf();
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
        let visible = self.visible_indexes(cx);
        if visible.is_empty() {
            return;
        }
        let current = self
            .current_index
            .and_then(|index| visible.iter().position(|i| *i == index));
        let next = match (current, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(visible.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.current_index = Some(visible[next]);
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
            self.navigate_to(entry.path.clone(), cx);
        } else if self.request.kind == PickerKind::SaveFile {
            self.filename_input
                .update(cx, |editor, cx| editor.set_text(&entry.label, cx));
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

    fn navigate_to(&mut self, location: PathBuf, cx: &mut Context<Self>) {
        if self.navigation.navigate(location) {
            self.start_scan(cx);
        }
    }
    fn navigate_up(&mut self, cx: &mut Context<Self>) {
        if self.navigation.up() {
            self.start_scan(cx);
        }
    }
    fn navigate_back(&mut self, cx: &mut Context<Self>) {
        if self.navigation.back() {
            self.start_scan(cx);
        }
    }
    fn navigate_forward(&mut self, cx: &mut Context<Self>) {
        if self.navigation.forward() {
            self.start_scan(cx);
        }
    }
    fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.current_index = None;
        cx.notify();
    }
    fn visible_indexes(&self, cx: &Context<Self>) -> Vec<usize> {
        let query = self.search_input.read(cx).text().to_owned();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                query_matches(&item.label, &query, self.show_hidden).then_some(index)
            })
            .collect()
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
                let current = self.filename_input.read(cx).text().to_owned();
                let name = std::path::Path::new(&current);
                if !matches!(
                    name.components().next(),
                    Some(std::path::Component::Normal(_))
                ) || name.components().count() != 1
                {
                    self.error = Some("Enter a valid file name (no paths)".into());
                    cx.notify();
                    return;
                }
                vec![self.navigation.current().join(name)]
            }
            PickerKind::SaveFiles => vec![self.navigation.current().to_path_buf()],
            PickerKind::OpenFile if self.request.directory => {
                vec![self.navigation.current().to_path_buf()]
            }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.request.title.clone();
        let source = if self.request.app_id.is_empty() {
            "Unknown caller".to_string()
        } else {
            self.request.app_id.clone()
        };
        let folder = self.navigation.current().to_string_lossy().to_string();
        let compact = window.bounds().size.width < px(710.);
        let crumbs = self.navigation.crumbs();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let locations = home.as_deref().map(quick_locations).unwrap_or_default();
        let search_query = self.search_input.read(cx).text().to_owned();
        let visible = self.visible_indexes(cx);
        let visible_count = visible.len();
        let reveal_id = format!("cabinet-reveal-{}-{}", self.scan_generation, self.scanning);
        let chosen = self.selected.len();
        let directory = self.request.directory;
        let save_file = self.request.kind == PickerKind::SaveFile;
        let save_files = self.request.kind == PickerKind::SaveFiles;
        let filename = self.filename_input.read(cx).text().to_owned();
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
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(|style| style.bg(rgb(0x35495e)))
                    .p_2()
                    .rounded_sm()
                    .bg(rgb(PANEL))
                    .text_color(rgb(ORANGE))
                    .text_size(px(12.))
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.cycle_choice(index, cx)))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                            this.cycle_choice(index, cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        let mut list = div().id("gfc-file-rows").w_full().flex().flex_col().gap_1();

        if scanning {
            list = list.child(div().text_color(rgb(MUTED)).child("Loading directory…"));
        } else if visible.is_empty() {
            list = list.child(div().p_4().text_color(rgb(MUTED)).child(
                if search_query.is_empty() {
                    "No visible files or folders"
                } else {
                    "No matching files or folders"
                },
            ));
        }
        for index in visible.iter().copied() {
            let entry = self.entries[index].clone();
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
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(|style| style.bg(rgb(0x35495e)))
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
                    .hover(|style| style.bg(rgb(0x324255)))
                    .active(|style| style.opacity(0.86))
                    .text_color(rgb(if picked { ORANGE } else { TEXT }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .min_w_0()
                            .child(
                                div().h(px(22.)).bg(rgb(ORANGE)).rounded_sm().with_spring(
                                    format!("cabinet-selected-{}", entry.path.display()),
                                    SpringAnimation::new(SpringConfig::new(230., 22., 1.))
                                        .to(px(if picked || focused { 3. } else { 0. })),
                                    |element, width| element.w(width),
                                ),
                            )
                            .child(
                                div()
                                    .w(px(24.))
                                    .text_color(rgb(if entry.folder { ORANGE } else { MUTED }))
                                    .child(marker),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .child(if entry.folder { "FOLDER" } else { "FILE" }),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.current_index = Some(index);
                        this.click_entry(&entry, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                            this.current_index = Some(index);
                            this.activate_cursor(cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }

        let mut shortcuts = div()
            .id("cabinet-locations")
            .w(px(174.))
            .h_full()
            .p_3()
            .rounded_md()
            .bg(rgb(0x131c27))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("PLACES"),
            );
        for (index, (label, location)) in locations.into_iter().enumerate() {
            let destination = location.clone();
            let destination_key = location.clone();
            let active = self.navigation.current() == location;
            shortcuts = shortcuts.child(
                div()
                    .id(format!("cabinet-place-{index}"))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Go to {label}"))
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(|style| style.bg(rgb(0x35495e)))
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .bg(rgb(if active { 0x283a4c } else { 0x131c27 }))
                    .hover(|style| style.bg(rgb(0x324255)))
                    .text_color(rgb(if active { ORANGE } else { TEXT }))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate_to(destination.clone(), cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                            this.navigate_to(destination_key.clone(), cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        let mut breadcrumbs = div()
            .id("cabinet-breadcrumbs")
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap_1();
        let crumb_count = crumbs.len();
        for (index, (name, path)) in crumbs.into_iter().enumerate() {
            if compact && index + 3 < crumb_count {
                continue;
            }
            let selected = index + 1 == crumb_count;
            let click_path = path.clone();
            let key_path = path.clone();
            breadcrumbs = breadcrumbs.child(
                div()
                    .id(format!("cabinet-crumb-{index}"))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .role(gpui::Role::Button)
                    .aria_label(format!("Open {name}"))
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(|style| style.bg(rgb(0x35495e)))
                    .text_color(rgb(if selected { ORANGE } else { MUTED }))
                    .hover(|style| style.bg(rgb(0x283748)))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate_to(click_path.clone(), cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                            this.navigate_to(key_path.clone(), cx);
                            cx.stop_propagation();
                        }
                    })),
            );
            if index + 1 < crumb_count {
                breadcrumbs = breadcrumbs.child(div().text_color(rgb(MUTED)).child("›"));
            }
        }
        let search_focus = self.search_input.read(cx).focus.clone();

        div()
            .id("gnosix-filechooser")
            .role(gpui::Role::Application)
            .aria_label("Cabinet — Gnosix file selection")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let search_focused = this.search_input.read(cx).focus.is_focused(window);
                let filename_focused = this.filename_input.read(cx).focus.is_focused(window);
                if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                    && event.keystroke.key == "f" {
                    let focus = this.search_input.read(cx).focus.clone();
                    focus.focus(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if (event.keystroke.modifiers.alt || event.keystroke.modifiers.platform)
                    && event.keystroke.key == "left" {
                    this.navigate_back(cx);
                    cx.stop_propagation();
                    return;
                }
                if (event.keystroke.modifiers.alt || event.keystroke.modifiers.platform)
                    && event.keystroke.key == "right" {
                    this.navigate_forward(cx);
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "f3" {
                    this.file_list_focus.focus(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if search_focused && event.keystroke.key == "down" {
                    this.file_list_focus.focus(window, cx);
                    this.move_cursor(true, cx);
                    cx.stop_propagation();
                    return;
                }
                if filename_focused && this.request.kind == PickerKind::SaveFile {
                    this.filename_input.update(cx, |editor, cx| editor.on_key(event, window, cx));
                } else if search_focused {
                    this.search_input.update(cx, |editor, cx| editor.on_key(event, window, cx));
                }
                match event.keystroke.key.as_str() {
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    }
                    "f6" => this.cycle_filter(cx),
                    "f7" => this.toggle_hidden(cx),
                    "escape" => this.complete(RenderResponse::Cancelled, cx),
                    "backspace" if !filename_focused && !search_focused => {
                        this.navigate_up(cx)
                    }
                    "up" if !filename_focused && !search_focused => {
                        this.move_cursor(false, cx)
                    }
                    "down" if !filename_focused && !search_focused => {
                        this.move_cursor(true, cx)
                    }
                    "enter" if filename_focused && this.request.kind == PickerKind::SaveFile => this.confirm(cx),
                    "enter" if this.current_index.is_some() => this.activate_cursor(cx),
                    _ => {} // native filename text is handled only by the GPUI InputHandler
                }
            }))
            .size_full()
            .p_4()
            .gap_3()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family("Montreal")
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(ORANGE))
                            .child("CABINET  /  GNOSIX"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child("NATIVE  •  GPUI"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(19.))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(if scanning { "SCANNING…" } else { "READY" }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("cabinet-back")
                            .role(gpui::Role::Button)
                            .aria_label("Previous folder")
                            .focusable()
                            .tab_stop(true)
                            .p_2()
                            .rounded_sm()
                            .bg(rgb(PANEL))
                            .text_color(rgb(if self.navigation.can_back() { TEXT } else { MUTED }))
                            .hover(|style| style.bg(rgb(0x324255)))
                            .child("←")
                            .on_click(cx.listener(|this, _, _, cx| this.navigate_back(cx)))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.navigate_back(cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("cabinet-forward")
                            .role(gpui::Role::Button)
                            .aria_label("Next folder")
                            .focusable()
                            .tab_stop(true)
                            .p_2()
                            .rounded_sm()
                            .bg(rgb(PANEL))
                            .text_color(rgb(if self.navigation.can_forward() { TEXT } else { MUTED }))
                            .hover(|style| style.bg(rgb(0x324255)))
                            .child("→")
                            .on_click(cx.listener(|this, _, _, cx| this.navigate_forward(cx)))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.navigate_forward(cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("gfc-up")
                            .focusable()
                            .tab_stop(true)
                            .role(gpui::Role::Button)
                            .aria_label("Go to parent folder")
                            .px_3()
                            .py_2()
                            .bg(rgb(PANEL))
                            .hover(|style| style.bg(rgb(0x324255)))
                            .rounded_sm()
                            .child("↑")
                            .on_click(cx.listener(|this, _, _, cx| this.navigate_up(cx)))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.navigate_up(cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .child(breadcrumbs),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("cabinet-search")
                            .flex_1()
                            .min_w_0()
                            .px_3()
                            .py_2()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .role(gpui::Role::TextInput)
                            .aria_label("Filter files by name")
                            .track_focus(&search_focus)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let focus = this.search_input.read(cx).focus.clone();
                                focus.focus(window, cx);
                            }))
                            .child(cabinet_ime::editor_canvas(self.search_input.clone())),
                    )
                    .child(
                        div()
                            .id("cabinet-hidden")
                            .role(gpui::Role::Button)
                            .aria_label("Show or hide dotfiles")
                            .focusable()
                            .tab_stop(true)
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(if self.show_hidden { 0x284557 } else { PANEL }))
                            .text_color(rgb(if self.show_hidden { ORANGE } else { MUTED }))
                            .hover(|style| style.bg(rgb(0x324255)))
                            .child(if self.show_hidden { "Hidden ✓" } else { "Hidden" })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_hidden(cx)))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.toggle_hidden(cx);
                                    cx.stop_propagation();
                                }
                            })),
                    ),
            )
            .when_some(filter_text, |panel, label| {
                panel.child(
                    div()
                        .id("gfc-filter-cycle")
                        .focusable()
                        .tab_stop(true)
                        .role(gpui::Role::Button)
                        .aria_label("Switch file type filter")
                        .px_3()
                        .py_2()
                        .bg(rgb(PANEL))
                        .text_color(rgb(ORANGE))
                        .text_size(px(12.))
                        .child(label)
                        .on_click(cx.listener(|this, _, _, cx| this.cycle_filter(cx)))
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                this.cycle_filter(cx);
                                cx.stop_propagation();
                            }
                        })),
                )
            })
            .child(choice_panel)
            .child(
                div()
                    .id("cabinet-explorer-body")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .when(!compact, |body| body.child(shortcuts))
                    .child(
                        div()
                            .id("gfc-files")
                            .focusable()
                            .tab_stop(true)
                            .track_focus(&self.file_list_focus)
                            .role(gpui::Role::Group)
                            .aria_label("Available files and folders")
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_2()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PANEL))
                            .rounded_md()
                            .child(
                                list.with_animation(
                                    reveal_id,
                                    Animation::new(Duration::from_millis(175))
                                        .with_easing(gpui::ease_out_quint()),
                                    |panel, phase| panel.opacity(phase),
                                ),
                            ),
                    ),
            )
            .when(save_file, |panel| {
                panel.child(
                    div()
                        .id("cabinet-filename-field")
                        .role(gpui::Role::TextInput)
                        .aria_label("File name")
                        .w_full()
                        .p_2()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .rounded_sm()
                        .bg(rgb(PANEL))
                        .text_size(px(16.))
                        .text_color(rgb(TEXT))
                        .track_focus(&self.filename_input.read(cx).focus)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let handle = this.filename_input.read(cx).focus.clone();
                            handle.focus(window, cx);
                        }))
                        .child(cabinet_ime::editor_canvas(self.filename_input.clone())),
                )
            })
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(12.))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(if save_file {
                        format!("Filename: {filename}  ·  native input / IME · Enter to confirm")
                    } else if save_files {
                        format!(
                            "Choose folder for {files_count} file(s); existing names are rejected"
                        )
                    } else if directory {
                        "Choose this directory".to_string()
                    } else {
                        format!("{chosen} selected  ·  {visible_count} visible  ·  {folder}  ·  Request: {source}")
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
                            .focusable()
                            .tab_stop(true)
                            .role(gpui::Role::Button)
                            .aria_label("Cancel file selection")
                            .px_4()
                            .py_2()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .rounded_sm()
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete(RenderResponse::Cancelled, cx);
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.complete(RenderResponse::Cancelled, cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("gfc-confirm")
                            .focusable()
                            .tab_stop(true)
                            .role(gpui::Role::Button)
                            .aria_label("Confirm file selection")
                            .px_4()
                            .py_2()
                            .rounded_sm()
                            .bg(rgb(ORANGE))
                            .text_color(rgb(BG))
                            .child(accept_label)
                            .on_click(cx.listener(|this, _, _, cx| this.confirm(cx)))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                                    this.confirm(cx);
                                    cx.stop_propagation();
                                }
                            })),
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
        let bounds = Bounds::centered(None, size(px(1020.), px(740.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                app_id: Some("in.gnu.os.cabinet".into()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| PickerView::new(request, child_outcome, cx));
                view.update(cx, |view, cx| {
                    if view.request.kind == PickerKind::SaveFile {
                        let handle = view.filename_input.read(cx).focus.clone();
                        handle.focus(window, cx);
                    } else {
                        view.focus.focus(window, cx);
                    }
                });
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
