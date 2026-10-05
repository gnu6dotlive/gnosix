//! Throwaway native comparison view, feature-gated out of the production scene.
//! Scripted fixture/state selection is lab configuration, never desktop input injection.
use crate::{
    lab_topology_compare::*,
    product_appstack::{appstack_rows, launch_feedback},
    product_design::{
        ProductControlFocus, ProductControlState, ProductDesignContext, ProductPalette,
        ProductSpacing, button, installed_application_item,
    },
    product_focus::{
        ProductFocusState, installed_application_control_identity, panel_control_identity,
        traversable_focus_handle,
    },
    product_runtime::ProductRuntime,
};
use gnos_ux_surface::grammar::reactive_envelope::ReactiveEnvelopeSolver;
use gnos_ux_surface::grammar::{GeometryRect, SurfaceViewport};
use gpui::{
    AnyElement, Bounds, Context, FocusHandle, PathBuilder, Pixels, Render, ScrollHandle, Window,
    canvas, div, point, prelude::*, px,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::Write,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

pub struct ComparisonContent {
    pub runtime: ProductRuntime,
    pub design: ProductDesignContext,
    pub name: String,
    scratch: Option<PathBuf>,
}
impl Drop for ComparisonContent {
    fn drop(&mut self) {
        if let Some(path) = &self.scratch {
            let _ = fs::remove_dir_all(path);
        }
    }
}
pub fn content_fixture(name: &str) -> Result<ComparisonContent, String> {
    let canonical = if name == "many-rows" {
        "appstack-basic"
    } else {
        name
    };
    let mut fixture = crate::product_lab::fixture(canonical)
        .ok_or_else(|| format!("unknown comparison fixture {name}"))?;
    let mut runtime = fixture.outputs.remove(0).runtime;
    let scratch = if name == "many-rows" {
        use gnosix_application_catalog::{
            ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
        };
        let root =
            std::env::temp_dir().join(format!("gnosix-topology-catalog-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        for i in 0..40 {
            fs::write(root.join(format!("probe-{i:02}.desktop")),format!("[Desktop Entry]\nType=Application\nName=Application {i:02}\nExec=/usr/bin/true\nTerminal=false\n")).map_err(|e|e.to_string())?;
        }
        runtime.set_application_catalog(std::sync::Arc::new(ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::from_names(["GNOSIX".to_string()]),
        )));
        Some(root)
    } else {
        None
    };
    Ok(ComparisonContent {
        runtime,
        design: fixture.design,
        name: name.to_string(),
        scratch,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct LabCommand {
    pub serial: u64,
    pub fixture: String,
    pub progress: f32,
    pub debug: bool,
    pub magnification: Option<usize>,
    pub reorder: bool,
    pub animate: bool,
}
impl Default for LabCommand {
    fn default() -> Self {
        Self {
            serial: 0,
            fixture: "appstack-joint".into(),
            progress: 1.0,
            debug: false,
            magnification: None,
            reorder: false,
            animate: false,
        }
    }
}
impl LabCommand {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let mut c = Self {
            serial: value["serial"]
                .as_u64()
                .ok_or("serial must be an integer")?,
            ..Self::default()
        };
        if let Some(s) = value["fixture"].as_str() {
            c.fixture = s.into();
        }
        if let Some(p) = value["progress"].as_f64() {
            c.progress = p as f32;
        }
        if !c.progress.is_finite() || !(0.0..=1.0).contains(&c.progress) {
            return Err("invalid progress".into());
        }
        c.debug = value["debug"].as_bool().unwrap_or(false);
        c.animate = value["animate"].as_bool().unwrap_or(false);
        c.reorder = value["reorder"].as_bool().unwrap_or(false);
        c.magnification = match value["magnification"].as_str() {
            None | Some("none") => None,
            Some("center") => Some(2),
            Some("edge") => Some(0),
            _ => return Err("unknown magnification sample".into()),
        };
        Ok(c)
    }
}

pub struct TopologyLabView {
    contract: ComparisonContract,
    command: LabCommand,
    control_path: Option<PathBuf>,
    ack_path: Option<PathBuf>,
    trace: Option<Rc<RefCell<fs::File>>>,
    prepaint: Rc<RefCell<[PanelPrepaint; 3]>>,
    activations: Rc<Cell<u64>>,
    probe_hit: Rc<Cell<Option<&'static str>>>,
    content: Rc<RefCell<ComparisonContent>>,
    frames: Vec<ComparisonFrame>,
    items: Vec<ItemProjection>,
    focus: [ProductFocusState; 3],
    scroll: [ScrollHandle; 3],
    sink: FocusHandle,
    solver: Option<ReactiveEnvelopeSolver>,
    shared: SharedProgress,
    production_c: crate::product_shared_envelope::SharedMotion,
    scales: [f32; 5],
    clock: Instant,
    previous: Instant,
    epoch: u64,
    closing: bool,
    motion_done: bool,
    intent_clock: Instant,
    durations: [Option<f64>; 3],
    open_durations: [Option<f64>; 3],
}

#[derive(Default)]
struct PanelPrepaint {
    cards: Vec<[f32; 4]>,
    rows: Vec<[f32; 4]>,
    line_height: f32,
}
impl TopologyLabView {
    pub fn new(
        control_path: Option<PathBuf>,
        ack_path: Option<PathBuf>,
        trace_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let contract =
            ComparisonContract::current(SurfaceViewport::try_new(1280.0, 800.0).unwrap())
                .map_err(|e| e.to_string())?;
        let command = LabCommand::default();
        let content = Rc::new(RefCell::new(content_fixture(&command.fixture)?));
        let (dock, items) =
            item_projection(contract, [1.0; 5], false).map_err(|e| e.to_string())?;
        let mut solver = None;
        let frames = vec![
            current_frame(contract, dock, &mut solver, true, true).map_err(|e| e.to_string())?,
            shared_frame(contract, dock, 1.0, 0.0, Topology::B).map_err(|e| e.to_string())?,
            shared_frame(contract, dock, 1.0, 0.0, Topology::C).map_err(|e| e.to_string())?,
        ];
        let trace = trace_path
            .map(|path| fs::File::create(path).map_err(|e| e.to_string()))
            .transpose()?;
        let now = Instant::now();
        let mut view = Self {
            contract,
            command,
            control_path,
            ack_path,
            trace: trace.map(|file| Rc::new(RefCell::new(file))),
            prepaint: Rc::new(RefCell::new(std::array::from_fn(|_| {
                PanelPrepaint::default()
            }))),
            activations: Rc::new(Cell::new(0)),
            probe_hit: Rc::new(Cell::new(None)),
            content,
            frames,
            items,
            focus: std::array::from_fn(|_| ProductFocusState::new()),
            scroll: std::array::from_fn(|_| ScrollHandle::new()),
            sink: cx.focus_handle(),
            solver,
            shared: SharedProgress {
                progress: 1.0,
                velocity: 0.0,
                target_open: true,
            },
            production_c: crate::product_shared_envelope::SharedMotion::endpoint(true),
            scales: [1.0; 5],
            clock: now,
            previous: now,
            epoch: 0,
            closing: false,
            motion_done: false,
            intent_clock: now,
            durations: [None; 3],
            open_durations: [None; 3],
        };
        view.poll_command()?;
        if view.control_path.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let Ok(continue_polling) =
                        this.update(cx, |view, cx| match view.poll_command() {
                            Ok(changed) => {
                                if changed || view.command.animate && !view.motion_done {
                                    cx.notify();
                                }
                                true
                            }
                            Err(e) => {
                                eprintln!("TOPOLOGY_LAB_HARNESS_ERROR {e}");
                                false
                            }
                        })
                    else {
                        break;
                    };
                    if !continue_polling {
                        break;
                    }
                }
            })
            .detach();
        }
        Ok(view)
    }
    fn poll_command(&mut self) -> Result<bool, String> {
        let Some(path) = self.control_path.as_ref() else {
            return Ok(false);
        };
        if !path.exists() {
            return Ok(false);
        }
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let next = LabCommand::parse(&value)?;
        if next.serial == self.command.serial {
            return Ok(false);
        }
        let geometry_changed = next.fixture != self.command.fixture
            || next.progress != self.command.progress
            || next.magnification != self.command.magnification
            || next.reorder != self.command.reorder
            || next.animate != self.command.animate;
        if next.fixture != self.command.fixture {
            self.content = Rc::new(RefCell::new(content_fixture(&next.fixture)?));
            self.scroll = std::array::from_fn(|_| ScrollHandle::new());
        }
        self.command = next;
        if geometry_changed {
            self.solver = None;
            self.shared = SharedProgress::default();
            self.production_c = Default::default();
            self.closing = false;
            self.motion_done = false;
            self.durations = [None; 3];
            self.open_durations = [None; 3];
            self.epoch = 0;
            self.previous = Instant::now();
            self.intent_clock = self.previous;
            self.scales = if self.command.animate {
                [1.0; 5]
            } else {
                MagProfile::EXPERIMENT.targets(self.command.magnification)
            };
            if !self.command.animate {
                self.freeze_sample()?;
            }
        }
        Ok(true)
    }
    fn freeze_sample(&mut self) -> Result<(), String> {
        let (dock, items) = item_projection(self.contract, self.scales, self.command.reorder)
            .map_err(|e| e.to_string())?;
        self.items = items;
        let mut a = current_frame(self.contract, dock, &mut self.solver, false, true)
            .map_err(|e| e.to_string())?;
        // Capture approximately requested progress without changing the current A solver.
        if self.command.progress == 1.0 {
            for _ in 0..400 {
                a = current_frame(self.contract, dock, &mut self.solver, true, false)
                    .map_err(|e| e.to_string())?;
            }
        } else if self.command.progress > 0.0 {
            for _ in 0..400 {
                let next = current_frame(self.contract, dock, &mut self.solver, true, false)
                    .map_err(|e| e.to_string())?;
                if next.progress >= self.command.progress {
                    if (next.progress - self.command.progress).abs()
                        < (a.progress - self.command.progress).abs()
                    {
                        a = next;
                    }
                    break;
                }
                a = next;
            }
        }
        self.production_c =
            crate::product_shared_envelope::SharedMotion::opening_sample(self.command.progress)
                .map_err(|e| e.to_string())?;
        self.frames = vec![
            a,
            shared_frame(self.contract, dock, self.command.progress, 0.0, Topology::B)
                .map_err(|e| e.to_string())?,
            production_c_frame(self.contract, dock, self.production_c)
                .map_err(|e| e.to_string())?,
        ];
        Ok(())
    }
    fn advance(&mut self) -> Result<f64, String> {
        if self.command.animate && !self.motion_done && self.epoch >= 1800 {
            return Err("HARNESS_ERROR: bounded comparison motion exceeded 1800 renders".into());
        }
        let now = Instant::now();
        let delta = now.duration_since(self.previous).as_secs_f64();
        self.previous = now;
        if self.command.animate && !self.motion_done {
            let elapsed = now.duration_since(self.intent_clock).as_secs_f64();
            // Same experimental magnification request and response in all panes.
            let hovered = if self.closing {
                None
            } else {
                self.command.magnification
            };
            let targets = MagProfile::EXPERIMENT.targets(hovered);
            let amount = 1.0 - (-delta as f32 / MagProfile::EXPERIMENT.response_seconds).exp();
            for (scale, target) in self.scales.iter_mut().zip(targets) {
                *scale += (target - *scale) * amount;
                if (*scale - target).abs() < 0.0001 {
                    *scale = target;
                }
            }
            let (dock, items) = item_projection(self.contract, self.scales, self.command.reorder)
                .map_err(|e| e.to_string())?;
            self.items = items;
            let reduced = self.content.borrow().design.reduced_motion();
            let a = current_frame(
                self.contract,
                dock,
                &mut self.solver,
                !self.closing,
                reduced,
            )
            .map_err(|e| e.to_string())?;
            self.shared.target_open = !self.closing;
            self.shared
                .step((delta as f32).max(0.00001), reduced)
                .map_err(|e| e.to_string())?;
            self.production_c
                .advance(!self.closing, reduced, delta as f32)
                .map_err(|e| e.to_string())?;
            self.frames = vec![
                a,
                shared_frame(
                    self.contract,
                    dock,
                    self.shared.progress,
                    self.shared.velocity,
                    Topology::B,
                )
                .map_err(|e| e.to_string())?,
                production_c_frame(self.contract, dock, self.production_c)
                    .map_err(|e| e.to_string())?,
            ];
            let settled = [
                self.solver
                    .as_ref()
                    .is_some_and(|s| s.is_settled_for(!self.closing)),
                self.shared.settled(),
                self.production_c.is_settled_for(!self.closing),
            ];
            for (i, is_settled) in settled.iter().enumerate() {
                if *is_settled && self.durations[i].is_none() {
                    self.durations[i] = Some(elapsed * 1000.0);
                }
            }
            if self.durations.iter().all(Option::is_some) {
                if !self.closing {
                    self.open_durations = self.durations;
                    self.durations = [None; 3];
                    self.closing = true;
                    self.intent_clock = now;
                } else {
                    self.motion_done = true;
                }
            }
        }
        self.epoch += 1;
        Ok(delta * 1000.0)
    }
    fn snapshot(&self, delta: f64, window: &Window) -> Value {
        let timestamp = self.clock.elapsed().as_secs_f64() * 1000.0;
        let content = self.content.borrow();
        let rows = appstack_rows(&content.runtime);
        let feedback = launch_feedback(content.runtime.launch_state());
        let frames=self.frames.iter().enumerate().map(|(i,frame)|{
            let mut j=frame.json(self.contract,&self.items,timestamp,delta);
            j["fixture"]=json!(content.name);j["text_scale"]=json!(content.design.text_scale());
            j["rows"]=json!(rows.iter().map(|r|json!({"entry":r.entry.as_str(),"name":r.name,"accessible_name":format!("AppStack · {}",r.name)})).collect::<Vec<_>>());
            j["focused_entry"]=json!(rows.iter().find(|r|self.focus[i].handle(&installed_application_control_identity(&r.entry)).is_some_and(|h|h.is_focused(window))).map(|r|r.entry.as_str()));
            for (index, item) in j["items"].as_array_mut().unwrap().iter_mut().enumerate() {
                let id = match index { 0 => Some(panel_control_identity("appstack")), 4 => Some(panel_control_identity("workspace")), _ => None };
                item["focus"] = json!(id.and_then(|id| self.focus[i].handle(&id)).is_some_and(|h|h.is_focused(window)));
            }
            if i==2 {
                let m = self.production_c;
                j["production_channels"]=json!({"target_open":m.target_open,"horizontal":m.horizontal.value,"vertical":m.vertical.value,"reveal":m.reveal.value,
                    "velocity":[m.horizontal.velocity,m.vertical.velocity,m.reveal.velocity],"interactable":m.target_open && [m.horizontal.value,m.vertical.value,m.reveal.value].iter().all(|p|*p>=0.995),
                    "settled":m.is_settled_for(m.target_open),"profile":"analytic critical damping omega=20/20/22 s^-1"});
            }
            j["feedback"]=json!(feedback.as_ref().map(|f|f.message.as_ref()));
            j["pointer_sample"]=json!(self.command.magnification.map(|h|{let r=self.items[h].painted;[(r.left()+r.right())*0.5,(r.top()+r.bottom())*0.5]}));
            j["pointer_hit"]=json!(self.command.magnification.and_then(|h|{let r=self.items[h].painted;hit_item(&self.items,(r.left()+r.right())*0.5,(r.top()+r.bottom())*0.5)}));
            j["reorder"]=if self.command.reorder{json!({"source":"Files","origin_slot":1,"target_slot":3,"slot_pitch":56,"insertion_marker_x":self.items[3].resting.right()+6.0,"authority":"NONE; presentation micro-probe only"})}else{Value::Null};
            j
        }).collect::<Vec<_>>();
        json!({"serial":self.command.serial,"epoch":self.epoch,"debug":self.command.debug,"animate":self.command.animate,"lab_activation_count":self.activations.get(),
            "phase":if !self.command.animate {"Frozen sample"}else if self.motion_done {"Closed"}else if self.shared.target_open {"Opening"}else{"Closing"},"motion_done":self.motion_done,
            "measurement":"GPUI render/paint projection; not Wayland presentation feedback",
            "phase_elapsed_ms":self.intent_clock.elapsed().as_secs_f64()*1000.0,
            "open_duration_ms":self.open_durations,"close_duration_ms":self.durations,"frames":frames,"common_contract":self.contract.json()})
    }
}
impl Render for TopologyLabView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let delta = self.advance().expect("valid lab geometry");
        let mut root = div()
            .id("topology-lab-root")
            .role(gpui::Role::Application)
            .aria_label("Experimental AppStack topology comparison")
            .track_focus(&self.sink)
            .on_key_down(crate::product_keyboard::dispatch_scene_key)
            .size_full()
            .relative()
            .bg(ProductPalette::canvas());
        for (i, frame) in self.frames.iter().enumerate() {
            let content = self.content.borrow();
            let rows = appstack_rows(&content.runtime);
            let design = content.design;
            let mut ids = vec![panel_control_identity("appstack")];
            ids.extend(
                rows.iter()
                    .map(|r| installed_application_control_identity(&r.entry)),
            );
            ids.push(panel_control_identity("workspace"));
            self.focus[i].reconcile(&ids);
            let handles = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    self.focus[i]
                        .ensure(id, &mut || traversable_focus_handle(cx))
                        .clone()
                        .tab_index((i * 1000 + index) as isize)
                })
                .collect::<Vec<_>>();
            if let Some(index) = rows.iter().position(|r| {
                self.focus[i]
                    .handle(&installed_application_control_identity(&r.entry))
                    .unwrap()
                    .is_focused(window)
            }) {
                self.scroll[i].scroll_to_item(index);
            }
            let mut column = div().flex().flex_col().gap(px(ProductSpacing::XXS));
            for (index, row) in rows.iter().enumerate() {
                let shared = self.content.clone();
                let entry = row.entry.clone();
                let entity = cx.entity().downgrade();
                let activations = self.activations.clone();
                column = column.child(
                    installed_application_item(
                        row.element_id(),
                        row.name.clone(),
                        row.entry.as_str(),
                        design,
                        &ProductControlFocus::new(
                            &handles[index + 1],
                            (i * 1000 + index + 1) as isize,
                        ),
                    )
                    .on_click(move |_, _, app| {
                        let mut content = shared.borrow_mut();
                        if let Some(selected) = content
                            .runtime
                            .installed_applications()
                            .into_iter()
                            .find(|s| s.reference.entry() == &entry)
                        {
                            activations.set(activations.get() + 1);
                            crate::product_launch::launch_lab_application(
                                &mut content.runtime,
                                &selected,
                            );
                        }
                        let _ = entity.update(app, |_, cx| cx.notify());
                    }),
                );
            }
            if rows.is_empty() {
                column = column.child(crate::product_design::appstack_empty_state(
                    "Aucune application disponible",
                    design,
                ));
            }
            let prepaint = self.prepaint.clone();
            let line_height = f32::from(
                gpui::TextStyle {
                    font_size: px(design.scaled_px(14.0)).into(),
                    ..Default::default()
                }
                .line_height_in_pixels(window.rem_size()),
            );
            let row_scroll = self.scroll[i].clone();
            // ScrollHandle indexes direct children. Attach it to the row column,
            // matching production, rather than to a wrapper whose only child is
            // the whole column (where scroll_to_item(39) cannot resolve a row).
            let column = column
                .on_children_prepainted(move |children, _, _| {
                    let mut samples = prepaint.borrow_mut();
                    // GPUI 0.3.5's tracked-scroll branch populates ScrollHandle
                    // bounds, but leaves the callback's children slice empty.
                    // Read the real registered children from that same handle.
                    let registered = (0..row_scroll.children_count())
                        .filter_map(|index| row_scroll.bounds_for_item(index))
                        .collect::<Vec<_>>();
                    let children = if children.is_empty() {
                        &registered
                    } else {
                        &children
                    };
                    let offset = row_scroll.offset();
                    samples[i].rows = children
                        .iter()
                        .map(|r| {
                            [
                                f32::from(r.origin.x + offset.x),
                                f32::from(r.origin.y + offset.y),
                                f32::from(r.size.width),
                                f32::from(r.size.height),
                            ]
                        })
                        .collect();
                    samples[i].line_height = line_height;
                })
                .id("topology-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll[i]);
            let feedback = launch_feedback(content.runtime.launch_state());
            let mut allocation = div()
                .absolute()
                .left(px(frame.content_allocation.left() - frame.clip.left()))
                .top(px(frame.content_allocation.top() - frame.clip.top()))
                .w(px(self.contract.content_width))
                .h(px(self.contract.content_height))
                .flex()
                .flex_col()
                .gap(px(ProductSpacing::XS))
                .child(column);
            if let Some(feedback) = &feedback {
                allocation =
                    allocation.child(crate::product_appstack_view::feedback(feedback, design));
            }
            let prepaint = self.prepaint.clone();
            let mut dock_inner = div()
                .on_children_prepainted(move |children, _, _| {
                    prepaint.borrow_mut()[i].cards = children
                        .iter()
                        .map(|r| {
                            [
                                f32::from(r.origin.x),
                                f32::from(r.origin.y),
                                f32::from(r.size.width),
                                f32::from(r.size.height),
                            ]
                        })
                        .collect();
                })
                .id(if frame.topology == Topology::C {
                    "DockInner"
                } else {
                    "dock-controls"
                })
                .role(gpui::Role::Group)
                .aria_label(if frame.topology == Topology::C {
                    "DockInner"
                } else {
                    "Dock"
                })
                .absolute()
                .left(px(frame.dock.left() - frame.outer.left()))
                .top(px(frame.dock.top() - frame.outer.top()))
                .w(px(frame.dock.width()))
                .h(px(frame.dock.height()));
            let appstack_inner = div()
                .id(if frame.topology == Topology::C {
                    "AppStackInner"
                } else {
                    "appstack-content"
                })
                .role(gpui::Role::Group)
                .aria_label(if frame.topology == Topology::C {
                    "AppStackInner"
                } else {
                    "AppStack"
                })
                .absolute()
                .left(px(frame.body.left() - frame.outer.left()))
                .top(px(frame.body.top() - frame.outer.top()))
                .w(px(frame.body.width()))
                .h(px(frame.body.height()))
                .child(
                    div()
                        .absolute()
                        .left(px(frame.clip.left() - frame.body.left()))
                        .top(px(frame.clip.top() - frame.body.top()))
                        .w(px(frame.clip.width()))
                        .h(px(frame.clip.height()))
                        .overflow_hidden()
                        .child(allocation),
                );
            let mut pane = div()
                .id(format!("topology-pane-{i}"))
                .absolute()
                .left(px(i as f32 * 1280.0))
                .top_0()
                .w(px(1280.0))
                .h(px(800.0))
                .overflow_hidden()
                .child(paint_frame(
                    frame.clone(),
                    self.items.clone(),
                    self.command.debug,
                    self.command.reorder,
                ));
            for (index, item) in self.items.iter().enumerate() {
                let focus = if index == 0 {
                    ProductControlFocus::new(&handles[0], (i * 1000) as isize)
                } else if index == 4 {
                    ProductControlFocus::new(
                        handles.last().unwrap(),
                        (i * 1000 + ids.len() - 1) as isize,
                    )
                } else {
                    ProductControlFocus::inert()
                };
                let label = if index == 0 { "A" } else { &item.id[..1] };
                let control = button(
                    format!("probe-card-{index}"),
                    label,
                    if index == 4 {
                        "Workspace control"
                    } else {
                        item.id
                    },
                    ProductControlState::Default,
                    design,
                    &focus,
                )
                .absolute()
                .left(px(item.painted.left() - frame.dock.left()))
                .top(px(item.painted.top() - frame.dock.top()))
                .w(px(item.painted.width()))
                .h(px(item.painted.height()));
                let hit = self.probe_hit.clone();
                let id = item.id;
                dock_inner = dock_inner.child(control.on_click(move |_, _, _| {
                    // Probe controls have no workspace/application authority.
                    // Native clickable semantics are retained for focus/a11y.
                    hit.set(Some(id));
                }));
            }
            pane = pane.child(
                div()
                    .id("SharedDockEnvelope")
                    .role(gpui::Role::Group)
                    .aria_label("Dock et AppStack")
                    .absolute()
                    .left(px(frame.outer.left()))
                    .top(px(frame.outer.top()))
                    .w(px(frame.outer.width()))
                    .h(px(frame.outer.height()))
                    .child(dock_inner)
                    .child(appstack_inner),
            );
            if self.command.debug {
                let label = format!(
                    "{} · outer {:.1}×{:.1} · Dock {:.1}×{:.1} · AppStack {:.1}×{:.1} · baseline {:.1} · joint/seam {:.1}",
                    frame.topology.label(),
                    frame.outer.width(),
                    frame.outer.height(),
                    frame.dock.width(),
                    frame.dock.height(),
                    frame.body.width(),
                    frame.body.height(),
                    frame.outer.bottom(),
                    frame.dock.top()
                );
                pane = pane.child(
                    crate::product_design::text(
                        label,
                        crate::product_design::ProductTextStyle::Caption,
                        ProductDesignContext::default(),
                    )
                    .absolute()
                    .left(px(24.0))
                    .top(px(24.0))
                    .text_color(ProductPalette::focus_ring()),
                );
            }
            root = root.child(pane);
        }
        let mut snapshot = self.snapshot(delta, window);
        let ack = self.ack_path.clone();
        let trace = self.trace.clone();
        let prepaint = self.prepaint.clone();
        root.child(
            canvas(
                move |_, _, _| (),
                move |_, _, _, _| {
                    for (i, observed) in prepaint.borrow().iter().enumerate() {
                        let local = |r: &[f32; 4]| [r[0] - i as f32 * 1280.0, r[1], r[2], r[3]];
                        snapshot["frames"][i]["gpui_prepaint"] = json!({
                            "cards_xywh": observed.cards.iter().map(local).collect::<Vec<_>>(),
                            "rows_xywh": observed.rows.iter().map(local).collect::<Vec<_>>(),
                            "row_line_height": observed.line_height,
                            "meaning": "actual GPUI child allocations; pane-local coordinates"
                        });
                    }
                    if let Some(trace) = &trace {
                        let mut trace = trace.borrow_mut();
                        writeln!(trace, "{snapshot}").expect("write lab paint trace");
                        trace.flush().expect("flush lab paint trace");
                    }
                    if let Some(path) = &ack {
                        let temp = path.with_extension("pending");
                        fs::write(&temp, snapshot.to_string())
                            .expect("write lab paint acknowledgement");
                        fs::rename(temp, path).expect("publish lab paint acknowledgement");
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
    }
}
fn paint_frame(
    frame: ComparisonFrame,
    items: Vec<ItemProjection>,
    debug: bool,
    reorder: bool,
) -> AnyElement {
    let topology = frame.topology;
    let shell = match frame.paint.clone() {
        PaintHull::Rounded(rect) => canvas(
            move |_, _, _| (),
            move |bounds, _, window, _| {
                if topology == Topology::C {
                    crate::product_hull::paint_shared_dock(bounds, rect, window);
                } else {
                    window.paint_path(
                        crate::product_hull::rounded_rect_path(bounds, rect),
                        ProductPalette::panel(),
                    );
                }
            },
        )
        .size_full()
        .absolute()
        .inset_0()
        .into_any_element(),
        PaintHull::Current(hull) => canvas(
            move |_, _, _| (),
            move |bounds, _, window, _| {
                window.paint_path(
                    crate::product_hull::connective_hull_path(bounds, *hull),
                    ProductPalette::panel(),
                )
            },
        )
        .size_full()
        .absolute()
        .inset_0()
        .into_any_element(),
    };
    div()
        .size_full()
        .absolute()
        .inset_0()
        .child(shell)
        .child(
            canvas(
                move |_, _, _| (),
                move |bounds, _, window, _| {
                    if reorder {
                        let marker = GeometryRect::try_new(
                            items[3].resting.right() + 5.0,
                            frame.dock.top() + 22.0,
                            items[3].resting.right() + 7.0,
                            frame.dock.bottom() - 22.0,
                            0.0,
                        )
                        .unwrap();
                        paint_rule(bounds, marker, ProductPalette::focus_ring(), window);
                    }
                    if debug {
                        for (r, color) in [
                            (frame.outer, gpui::rgba(0xf4cf69ff)),
                            (frame.dock, gpui::rgba(0x65dbafff)),
                            (frame.body, gpui::rgba(0x77aaffff)),
                            (frame.clip, gpui::rgba(0xdd88ffff)),
                        ] {
                            paint_outline(bounds, r, color, window);
                        }
                        for item in &items {
                            paint_outline(bounds, item.resting, gpui::rgba(0x999999ff), window);
                            paint_outline(bounds, item.painted, gpui::rgba(0x65dbafff), window);
                        }
                        let baseline = GeometryRect::try_new(
                            frame.outer.left() - 12.0,
                            frame.outer.bottom(),
                            frame.outer.right() + 12.0,
                            frame.outer.bottom() + 1.0,
                            0.0,
                        )
                        .unwrap();
                        paint_rule(bounds, baseline, gpui::rgba(0xf4cf69ff), window);
                        let seam = GeometryRect::try_new(
                            frame.body.left(),
                            frame.dock.top(),
                            frame.body.right(),
                            frame.dock.top() + 1.0,
                            0.0,
                        )
                        .unwrap();
                        paint_rule(bounds, seam, gpui::rgba(0xff8888ff), window);
                        for r in frame.input.iter().step_by(8) {
                            paint_outline(bounds, *r, gpui::rgba(0x44aacc88), window);
                        }
                    }
                },
            )
            .size_full()
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}
fn paint_rule(bounds: Bounds<Pixels>, r: GeometryRect, color: gpui::Rgba, window: &mut Window) {
    let x = |v| px(f32::from(bounds.origin.x) + v);
    let y = |v| px(f32::from(bounds.origin.y) + v);
    let mut path = PathBuilder::fill();
    path.move_to(point(x(r.left()), y(r.top())));
    path.line_to(point(x(r.right()), y(r.top())));
    path.line_to(point(x(r.right()), y(r.bottom())));
    path.line_to(point(x(r.left()), y(r.bottom())));
    path.close();
    window.paint_path(path.build().unwrap(), color);
}
fn paint_outline(bounds: Bounds<Pixels>, r: GeometryRect, color: gpui::Rgba, window: &mut Window) {
    for rect in [
        GeometryRect::try_new(r.left(), r.top(), r.right(), r.top() + 1.0, 0.0),
        GeometryRect::try_new(r.left(), r.bottom(), r.right(), r.bottom() + 1.0, 0.0),
        GeometryRect::try_new(r.left(), r.top(), r.left() + 1.0, r.bottom(), 0.0),
        GeometryRect::try_new(r.right(), r.top(), r.right() + 1.0, r.bottom(), 0.0),
    ] {
        paint_rule(bounds, rect.unwrap(), color, window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_variants_receive_same_fixture_identity_rows_design_and_feedback() {
        for name in [
            "appstack-basic",
            "appstack-joint",
            "appstack-long-title-2x",
            "appstack-refused",
            "appstack-empty",
            "appstack-duplicate-names",
            "many-rows",
        ] {
            let fixture = content_fixture(name).unwrap();
            let rows = appstack_rows(&fixture.runtime);
            for topology in Topology::ALL {
                let _ = topology;
                assert_eq!(appstack_rows(&fixture.runtime), rows);
                assert_eq!(
                    fixture.design.text_scale(),
                    if name == "appstack-long-title-2x" {
                        2.0
                    } else {
                        1.0
                    }
                );
            }
            if name == "appstack-refused" {
                let f = launch_feedback(fixture.runtime.launch_state()).unwrap();
                assert_eq!(f.code, "TERMINAL_REQUIRED");
                assert!(!f.launched);
            } else {
                assert!(launch_feedback(fixture.runtime.launch_state()).is_none());
            }
        }
    }
    #[test]
    fn native_prepaint_and_overflow_traversal_are_shared_across_all_panels() {
        use gpui::{InputEvent, TestAppContext, VisualTestContext, size};
        let mut app = TestAppContext::single();
        let window = app.add_window(|_, cx| {
            let mut view = TopologyLabView::new(None, None, None, cx).unwrap();
            view.content = Rc::new(RefCell::new(content_fixture("many-rows").unwrap()));
            view
        });
        let context = VisualTestContext::from_window(window.into(), &app).into_mut();
        context.simulate_resize(size(px(3840.0), px(800.0)));
        context.run_until_parked();
        let (content, activations) = {
            let cx = &app.app.borrow();
            let view = window.read(cx).unwrap();
            let samples = view.prepaint.borrow();
            for (i, sample) in samples.iter().enumerate() {
                assert_eq!(sample.cards.len(), 5);
                for (actual, item) in sample.cards.iter().zip(&view.items) {
                    assert!((actual[0] - i as f32 * 1280.0 - item.painted.left()).abs() < 1.1);
                    assert!((actual[1] - item.painted.top()).abs() < 1.1);
                }
                assert_eq!(sample.rows.len(), 40);
                assert!(sample.rows.iter().all(|r| r[3] >= sample.line_height));
            }
            (view.content.clone(), view.activations.clone())
        };
        for panel in 0..3 {
            let (handles, scroll) = {
                let cx = &app.app.borrow();
                let view = window.read(cx).unwrap();
                let handles = view.focus[panel]
                    .order()
                    .iter()
                    .map(|id| view.focus[panel].handle(id).unwrap().clone())
                    .collect::<Vec<_>>();
                (handles, view.scroll[panel].clone())
            };
            context.update(|window, cx| window.focus(&handles[0], cx));
            for handle in &handles[1..41] {
                context.run_until_parked();
                context.update(|window, cx| {
                    window.draw(cx).clear(cx);
                    window.dispatch_event(
                        gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("tab").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    assert!(handle.is_focused(window));
                });
            }
            context.run_until_parked();
            assert!(scroll.offset().y < px(0.0));
            let row = scroll.bounds_for_item(39).unwrap();
            assert!(row.top() + scroll.offset().y >= scroll.bounds().top());
            assert!(row.bottom() + scroll.offset().y <= scroll.bounds().bottom());
            context.update(|window, cx| {
                window.draw(cx).clear(cx);
                let keystroke = gpui::Keystroke::parse("enter").unwrap();
                window.dispatch_event(
                    gpui::KeyDownEvent {
                        keystroke: keystroke.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(gpui::KeyUpEvent { keystroke }.to_platform_input(), cx);
            });
            assert_eq!(
                activations.get(),
                panel as u64 + 1,
                "one native Enter -> one product intent"
            );
            let feedback = launch_feedback(content.borrow().runtime.launch_state()).unwrap();
            assert_eq!(feedback.entry.as_str(), "probe-39.desktop");
            assert!(
                !feedback.launched,
                "lab executor must never start a real process"
            );
        }
    }
    #[test]
    fn command_rejects_invalid_progress_and_unknown_experiments() {
        assert!(LabCommand::parse(&json!({"serial":1,"progress":-0.1})).is_err());
        assert!(LabCommand::parse(&json!({"serial":1,"magnification":"unknown"})).is_err());
        assert!(
            LabCommand::parse(&json!({"serial":1,"progress":0.5,"debug":true}))
                .unwrap()
                .debug
        );
    }
    #[test]
    fn magnified_native_hit_boxes_match_painted_cards_at_center_and_edge() {
        use gpui::{TestAppContext, VisualTestContext, size};
        for hovered in [0, 2] {
            let mut app = TestAppContext::single();
            let window = app.add_window(|_, cx| {
                let mut view = TopologyLabView::new(None, None, None, cx).unwrap();
                view.command.magnification = Some(hovered);
                view.scales = MagProfile::EXPERIMENT.targets(Some(hovered));
                view.freeze_sample().unwrap();
                view
            });
            let context = VisualTestContext::from_window(window.into(), &app).into_mut();
            context.simulate_resize(size(px(3840.0), px(800.0)));
            context.run_until_parked();
            let (items, hit) = {
                let cx = &app.app.borrow();
                let view = window.read(cx).unwrap();
                (view.items.clone(), view.probe_hit.clone())
            };
            for panel in 0..3 {
                for item in &items {
                    // Include a point near the enlarged right edge, which a
                    // resting-pitch lookup can assign to the wrong neighbour.
                    for x in [
                        (item.painted.left() + item.painted.right()) * 0.5,
                        item.painted.right() - 2.0,
                    ] {
                        hit.set(None);
                        context.simulate_click(
                            point(
                                px(panel as f32 * 1280.0 + x),
                                px((item.painted.top() + item.painted.bottom()) * 0.5),
                            ),
                            gpui::Modifiers::default(),
                        );
                        assert_eq!(hit.get(), Some(item.id));
                    }
                }
            }
        }
    }
}
