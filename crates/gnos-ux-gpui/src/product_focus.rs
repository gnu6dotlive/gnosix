//! Stable focus ownership for interactive product controls.
//!
//! GPUI makes `Action::Focus` resolve a node to a [`FocusHandle`] only when
//! the rendered element calls `track_focus`. Semantic flags alone
//! (`focusable`, `tab_stop`, `tab_index`) advertise focusability to assistive
//! technology while leaving the request a silent no-op.
//!
//! This module owns one handle per logical product control, keyed by stable
//! product identity rather than render order or display label, so that:
//!
//! - the same control keeps the same handle across ordinary rerenders;
//! - a new semantic generation that still contains the same logical control
//!   does not move keyboard focus;
//! - a control that disappears retires its handle;
//! - tab order follows the order the controls are handed in.

use std::collections::HashMap;

use gnosix_application_catalog::DesktopEntryId;
use gnosix_desktop_model::WorkspaceId;
use gpui::{App, FocusHandle, SharedString};

/// A stable product identity for an interactive control.
///
/// The caller supplies the exact product reference (for example the observed
/// workspace reference), not a label, so two controls that happen to share a
/// name can never collide.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProductControlId(pub SharedString);

impl ProductControlId {
    /// The identity token as plain text, for diagnostics that must name a
    /// control without borrowing the handle.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// Stable focus identity for a workspace control.
///
/// The durable `WorkspaceId` is used rather than the generation-scoped
/// `ObservedWorkspaceReference`, because a new semantic generation must not
/// move keyboard focus onto a different handle for the same logical
/// workspace.
pub fn workspace_control_identity(workspace_id: &WorkspaceId) -> ProductControlId {
    let mut token = String::with_capacity(2 + workspace_id.as_bytes().len() * 2);
    token.push_str("ws:");
    for byte in workspace_id.as_bytes() {
        token.push_str(&format!("{byte:02x}"));
    }
    ProductControlId(token.into())
}

/// Stable focus identity for one installed application control.
///
/// Keyed by the desktop entry id, never by display name and never by render
/// position. Two installed applications can legitimately share a visible
/// title, and the list's order changes with the catalog, so a name-keyed or
/// index-keyed identity would move focus onto a different application when the
/// list re-sorted. The entry id is the only thing here that survives both.
///
/// Hex-encoded like the workspace identity so an identity can never be confused
/// with a label that happens to contain punctuation.
pub fn installed_application_control_identity(entry: &DesktopEntryId) -> ProductControlId {
    let mut token = String::with_capacity(4 + entry.as_str().len() * 2);
    token.push_str("app:");
    for byte in entry.as_str().as_bytes() {
        token.push_str(&format!("{byte:02x}"));
    }
    ProductControlId(token.into())
}

/// Stable focus identity for a surface-level affordance, such as the Dock's
/// own AppStack button.
pub fn panel_control_identity(panel: &str) -> ProductControlId {
    let mut token = String::with_capacity(6 + panel.len() * 2);
    token.push_str("panel:");
    for byte in panel.as_bytes() {
        token.push_str(&format!("{byte:02x}"));
    }
    ProductControlId(token.into())
}

/// Mint a focus handle that is actually *in* the traversal order.
///
/// gpui decides whether a handle is a tab stop from the handle itself, not
/// from the element it is bound to: `TabStopMap::insert` copies
/// `focus_handle.tab_stop`, and `focus_next` skips every node whose flag is
/// false. A handle minted with a bare `cx.focus_handle()` is therefore
/// invisible to traversal even when it is bound with `track_focus` and
/// focus lands on it correctly - which is exactly the shape of the defect
/// this function exists to prevent.
///
/// The default uses insertion order. Callers can assign explicit handle
/// indices when the desired traversal order crosses sibling surfaces.
pub fn traversable_focus_handle(cx: &mut App) -> FocusHandle {
    cx.focus_handle().tab_stop(true)
}

/// Owns the focus handles for the controls a product view currently exposes.
#[derive(Default)]
pub struct ProductFocusState {
    handles: HashMap<ProductControlId, FocusHandle>,
    /// Insertion order of the controls most recently presented, which is the
    /// order the product laid them out in.
    order: Vec<ProductControlId>,
}

impl ProductFocusState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile the owned handles against the controls actually presented.
    ///
    /// `ids` must be in product layout order. Handles for ids that are still
    /// present are retained, so focus survives an unrelated semantic
    /// generation. Handles for ids that disappeared are dropped.
    pub fn reconcile(&mut self, ids: &[ProductControlId]) {
        let keep: std::collections::HashSet<&ProductControlId> = ids.iter().collect();
        self.handles.retain(|id, _| keep.contains(id));
        self.order = ids.to_vec();
    }

    /// The handle for a control that is currently presented, if it owns one.
    pub fn handle(&self, id: &ProductControlId) -> Option<&FocusHandle> {
        self.handles.get(id)
    }

    /// Number of retained handles, for tests and diagnostics.
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// The presentation order recorded by the last reconcile.
    pub fn order(&self) -> &[ProductControlId] {
        &self.order
    }

    /// Register a control, returning its stable handle.
    ///
    /// Returns the existing handle when the control is already owned, which
    /// is what keeps focus identity stable across rerenders.
    pub fn ensure(
        &mut self,
        id: &ProductControlId,
        make: &mut dyn FnMut() -> FocusHandle,
    ) -> &FocusHandle {
        if !self.handles.contains_key(id) {
            self.handles.insert(id.clone(), make());
        }
        self.handles.get(id).expect("handle was just inserted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_design::{
        ProductControlFocus, ProductControlState, ProductDesignContext, button,
        installed_application_item,
    };
    use gpui::prelude::*;
    use gpui::{InputEvent, TestAppContext, div};

    fn id(name: &str) -> ProductControlId {
        ProductControlId(name.into())
    }

    /// Mint `count` real focus handles through the same `App::focus_handle`
    /// path the product uses, so the tests never fabricate handles.
    fn handles(count: usize) -> Vec<FocusHandle> {
        let app = TestAppContext::single();
        app.update(|cx: &mut gpui::App| (0..count).map(|_| cx.focus_handle()).collect())
    }

    #[test]
    fn retained_control_keeps_its_handle_across_reconciles() {
        let seed = handles(4);
        let mut next = seed.into_iter();
        let mut state = ProductFocusState::new();
        let a = id("workspace:a");
        let b = id("workspace:b");

        state.reconcile(&[a.clone(), b.clone()]);
        state.ensure(&a, &mut || next.next().expect("seeded handle"));
        state.ensure(&b, &mut || next.next().expect("seeded handle"));
        let first = state.handle(&a).unwrap().clone();
        let first_b = state.handle(&b).unwrap().clone();

        // An unrelated generation presents the same logical controls.
        state.reconcile(&[a.clone(), b.clone()]);

        assert_eq!(
            state.handle(&a).unwrap().clone(),
            first,
            "the same control must keep the same focus handle"
        );
        assert_eq!(state.handle(&b).unwrap().clone(), first_b);
    }

    #[test]
    fn removed_control_drops_its_handle() {
        let seed = handles(2);
        let mut next = seed.into_iter();
        let mut state = ProductFocusState::new();
        let a = id("workspace:a");
        let b = id("workspace:b");
        state.reconcile(&[a.clone(), b.clone()]);
        state.ensure(&a, &mut || next.next().expect("seeded handle"));
        state.ensure(&b, &mut || next.next().expect("seeded handle"));
        assert_eq!(state.len(), 2);

        state.reconcile(std::slice::from_ref(&a));

        assert_eq!(state.len(), 1);
        assert!(
            state.handle(&b).is_none(),
            "retired control must lose its handle"
        );
        assert!(state.handle(&a).is_some());
    }

    #[test]
    fn new_control_receives_a_fresh_handle() {
        let seed = handles(2);
        let mut next = seed.into_iter();
        let mut state = ProductFocusState::new();
        let a = id("workspace:a");
        state.reconcile(std::slice::from_ref(&a));
        state.ensure(&a, &mut || next.next().expect("seeded handle"));
        let before = state.handle(&a).unwrap().clone();

        let c = id("workspace:c");
        state.reconcile(&[a.clone(), c.clone()]);
        state.ensure(&c, &mut || next.next().expect("seeded handle"));

        assert_eq!(state.handle(&a).unwrap().clone(), before);
        assert!(state.handle(&c).is_some(), "new control must own a handle");
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn order_follows_product_layout_not_identity_ordering() {
        let mut state = ProductFocusState::new();
        let ids = vec![id("workspace:z"), id("workspace:a"), id("workspace:m")];
        state.reconcile(&ids);
        assert_eq!(state.order(), ids.as_slice());
    }

    /// Two controls in render order, each bound to a caller-owned handle.
    struct TraversalHarness {
        first: FocusHandle,
        second: FocusHandle,
    }

    struct OverflowingListHarness {
        identities: Vec<ProductControlId>,
        focus: ProductFocusState,
        scroll: gpui::ScrollHandle,
        launches: std::rc::Rc<std::cell::RefCell<TestLaunchRecords>>,
        applications: Vec<crate::product_runtime::ProductInstalledApplicationContent>,
        catalog_root: std::path::PathBuf,
        route_at_root: bool,
    }

    struct TestLaunchRecords {
        runtime: crate::product_runtime::ProductRuntime,
        records: Vec<crate::product_runtime::ProductLaunchRecord>,
    }

    struct TestCatalogCleanup(std::path::PathBuf);

    impl Drop for TestCatalogCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn isolated_catalog_root() -> std::path::PathBuf {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // PIDs can repeat across test process namespaces. Never reuse or erase
        // an existing fixture; allocate exclusively and retry bounded collisions.
        for _ in 0..64 {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "gnosix-focus-catalog-{}-{nonce}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create isolated catalog fixture: {error}"),
            }
        }
        panic!("could not allocate a unique catalog fixture after 64 attempts");
    }

    fn overflowing_fixture(
        cx: &mut gpui::Context<OverflowingListHarness>,
        route_at_root: bool,
    ) -> OverflowingListHarness {
        use gnosix_application_catalog::{
            ApplicationCatalog, ApplicationCatalogRoots, DesktopEnvironment, LocalePreferences,
        };
        let catalog_root = isolated_catalog_root();
        for row in 0..12 {
            std::fs::write(catalog_root.join(format!("example-{row:02}.desktop")),
                "[Desktop Entry]\nType=Application\nName=Duplicate label\nExec=/usr/bin/true\nTerminal=false\n").unwrap();
        }
        let catalog = ApplicationCatalog::discover(
            &ApplicationCatalogRoots::from_application_dirs(vec![catalog_root.clone()]),
            &LocalePreferences::neutral(),
            DesktopEnvironment::from_names(["GNOSIX".to_owned()]),
        );
        let mut runtime = crate::product_runtime::ProductRuntime::default();
        runtime.set_application_catalog(std::sync::Arc::new(catalog));
        let applications = runtime.installed_applications();
        assert_eq!(applications.len(), 12);
        let identities = std::iter::once(id("panel:appstack"))
            .chain(applications.iter().map(|application| {
                installed_application_control_identity(application.reference.entry())
            }))
            .chain(std::iter::once(id("workspace:main")))
            .collect::<Vec<_>>();
        let mut focus = ProductFocusState::new();
        focus.reconcile(&identities);
        for identity in &identities {
            focus.ensure(identity, &mut || traversable_focus_handle(cx));
        }
        OverflowingListHarness {
            identities,
            focus,
            scroll: gpui::ScrollHandle::new(),
            launches: std::rc::Rc::new(std::cell::RefCell::new(TestLaunchRecords {
                runtime,
                records: Vec::new(),
            })),
            applications,
            catalog_root,
            route_at_root,
        }
    }

    impl gpui::Render for OverflowingListHarness {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let handles = self
                .identities
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    self.focus
                        .handle(id)
                        .unwrap()
                        .clone()
                        .tab_index(index as isize)
                })
                .collect::<Vec<_>>();
            if let Some(index) = crate::scene_runtime::focused_appstack_row_index(
                &self.identities,
                &handles,
                _window,
            ) {
                self.scroll.scroll_to_item(index);
            }
            let design = ProductDesignContext::default();
            let panel_handle = &handles[0];
            let launches = self.launches.clone();
            let rows = self.identities[1..1 + 12]
                .iter()
                .enumerate()
                .map(|(row, id)| {
                    let launches = launches.clone();
                    let selected = self.applications[row].clone();
                    installed_application_item(
                        SharedString::from(id.as_str()),
                        selected.name.clone(),
                        selected.reference.entry().as_str(),
                        design,
                        &ProductControlFocus::new(&handles[row + 1], (row + 1) as isize),
                    )
                    .on_click(move |_, _, _| {
                        let mut launches = launches.borrow_mut();
                        let record = crate::product_launch::launch_test_application(
                            &mut launches.runtime,
                            &selected,
                        );
                        launches.records.push(record);
                    })
                    .into_any_element()
                });
            let workspace = button(
                "workspace-control",
                "Workspace",
                "Workspace control",
                ProductControlState::Default,
                design,
                &ProductControlFocus::new(&handles[13], 13),
            );
            let dock = div()
                .flex()
                .child(button(
                    "appstack-affordance",
                    "AppStack",
                    "Open AppStack",
                    ProductControlState::Default,
                    design,
                    &ProductControlFocus::new(panel_handle, 0),
                ))
                .child(workspace);
            let dock = if self.route_at_root {
                dock
            } else {
                dock.on_key_down(crate::product_keyboard::dispatch_scene_key)
            };
            let root = div()
                .flex()
                .flex_col()
                .h(gpui::px(180.0))
                .child(dock)
                .child(
                    div()
                        .h(gpui::px(80.0))
                        .id("appstack-list")
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .children(rows),
                );
            if self.route_at_root {
                root.on_key_down(crate::product_keyboard::dispatch_scene_key)
            } else {
                root
            }
        }
    }

    impl gpui::Render for TraversalHarness {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let first = self.first.clone();
            let second = self.second.clone();
            div()
                .id(SharedString::from("first"))
                .track_focus(&first)
                .child(div())
                .child(
                    div()
                        .id(SharedString::from("second"))
                        .track_focus(&second)
                        .child(div()),
                )
        }
    }

    /// Prove traversal reaches a caller-owned handle in both directions.
    ///
    /// This is the defect the product actually had: gpui reads the tab-stop
    /// flag from the *handle*, so a handle minted without it is bound,
    /// focusable, and correctly reported by `is_focused`, yet skipped by
    /// `focus_next`. Asserting only that focus lands on a control would
    /// have passed while Tab silently did nothing.
    fn traversal_reaches(mint: fn(&mut gpui::App) -> FocusHandle) -> [bool; 2] {
        use gpui::VisualTestContext;

        let mut app = TestAppContext::single();
        let window =
            app.add_window(
                |_, cx: &mut gpui::Context<TraversalHarness>| TraversalHarness {
                    first: mint(cx),
                    second: mint(cx),
                },
            );
        // Run the real draw pass: the tab-stop map traversal reads is only
        // filled once a frame has been painted.
        let context = VisualTestContext::from_window(window.into(), &app).into_mut();
        context.run_until_parked();
        let (first, second) = {
            let cx = &app.app.borrow();
            let harness = window.read(cx).expect("the test window must be open");
            (harness.first.clone(), harness.second.clone())
        };
        context.update(|window, cx| {
            window.focus(&first, cx);
            assert!(first.is_focused(window), "precondition: first is focused");
            window.focus_next(cx);
            let forward = second.is_focused(window);
            window.focus_prev(cx);
            let backward = first.is_focused(window);
            [forward, backward]
        })
    }

    #[test]
    fn a_product_handle_is_reachable_by_traversal_in_both_directions() {
        let [forward, backward] = traversal_reaches(traversable_focus_handle);
        assert!(forward, "Tab must reach the next product control");
        assert!(backward, "Shift+Tab must return to the previous control");
    }

    #[test]
    fn a_bare_handle_is_not_reachable_by_traversal() {
        // The premise of the fix. If gpui ever made bare handles
        // traversable this would fail, and the product would then have two
        // sources of truth for its tab order.
        let [forward, _] = traversal_reaches(|cx| cx.focus_handle());
        assert!(
            !forward,
            "a bare handle is expected to be skipped; if this now passes, \
             gpui changed and the product's explicit marking is redundant"
        );
    }

    #[test]
    fn overflowing_appstack_rows_are_tab_reachable_and_scroll_into_view() {
        use gpui::VisualTestContext;

        const ROWS: usize = 12;
        let mut app = TestAppContext::single();
        let window = app.add_window(|_, cx| overflowing_fixture(cx, true));
        // The test owns cleanup independently of GPUI entity destruction.
        let _catalog_cleanup =
            TestCatalogCleanup(window.read(&app.app.borrow()).unwrap().catalog_root.clone());
        let scroll = window.read(&app.app.borrow()).unwrap().scroll.clone();
        let context = VisualTestContext::from_window(window.into(), &app).into_mut();
        context.run_until_parked();
        let handles = {
            let cx = &app.app.borrow();
            let harness = window.read(cx).unwrap();
            harness
                .identities
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    harness
                        .focus
                        .handle(id)
                        .unwrap()
                        .clone()
                        .tab_index(index as isize)
                })
                .collect::<Vec<_>>()
        };
        context.update(|window, cx| window.focus(&handles[0], cx));
        for index in 0..ROWS {
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
                assert!(
                    handles[index + 1].is_focused(window),
                    "row {index} must be reachable by Tab"
                );
            });
        }
        context.run_until_parked();
        assert!(
            scroll.offset().y < gpui::px(0.0),
            "focus on an offscreen row must scroll the list"
        );
        let viewport = scroll.bounds();
        let focused_row = scroll
            .bounds_for_item(ROWS - 1)
            .expect("focused row bounds");
        let offset = scroll.offset().y;
        assert!(focused_row.top() + offset >= viewport.top());
        assert!(focused_row.bottom() + offset <= viewport.bottom());
        context.update(|window, cx| {
            window.draw(cx).clear(cx);
            let focus_color = gpui::Hsla::from(crate::product_design::ProductPalette::focus_ring());
            let rings = window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.border_color == focus_color
                        && quad.border_widths.top > gpui::ScaledPixels(0.0)
                })
                .collect::<Vec<_>>();
            // GPUI emits multiple quads for a rounded border. They must
            // all identify this one focused row, rather than other rows.
            assert!(
                !rings.is_empty(),
                "the focused application paints a focus ring"
            );
            let ring = &rings[0];
            assert!(rings.iter().all(|quad| quad.bounds == ring.bounds));
            let scale = window.scale_factor();
            assert_eq!(ring.bounds.top(), (focused_row.top() + offset).scale(scale));
            assert_eq!(
                ring.bounds.bottom(),
                (focused_row.bottom() + offset).scale(scale)
            );
            // Each border segment has its own mask. Together the painted
            // segments must reach both vertical edges of the focused row.
            assert!(rings.iter().any(|quad| {
                quad.content_mask.bounds.top() <= ring.bounds.top()
                    && quad.content_mask.bounds.bottom() > ring.bounds.top()
            }));
            assert!(rings.iter().any(|quad| {
                quad.content_mask.bounds.bottom() >= ring.bounds.bottom()
                    && quad.content_mask.bounds.top() < ring.bounds.bottom()
            }));
        });
        context.update(|window, cx| {
            window.focus_next(cx);
            assert!(
                handles[ROWS + 1].is_focused(window),
                "Dock workspace controls remain after the AppStack list"
            );
        });
        assert!(scroll.max_offset().y > gpui::px(0.0));
        context.update(|window, cx| window.focus(&handles[ROWS], cx));
        context.run_until_parked();
        let launches = {
            let cx = &app.app.borrow();
            window.read(cx).unwrap().launches.clone()
        };
        assert!(
            launches.borrow().records.is_empty(),
            "focus alone must not launch"
        );
        // GPUI's keystroke shortcut helper does not supply the release which
        // native clickable semantics require. Exercise the actual event pair.
        context.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(handles[ROWS].is_focused(window));
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
        let launches = launches.borrow();
        assert_eq!(
            launches.records.len(),
            1,
            "one Enter must produce one product launch record"
        );
        let record = &launches.records[0];
        assert_eq!(
            record.entry.as_str(),
            "example-11.desktop",
            "non-focused rows must not launch"
        );
        assert_eq!(
            &record.catalog_generation,
            launches.runtime.application_catalog().generation()
        );
        assert!(record.terminal.as_ref().unwrap().is_launched());
        assert!(record.detail.contains("LAUNCHED"));
        assert_eq!(
            launches.runtime.launch_state(),
            &crate::product_runtime::ProductLaunchState::Terminal(record.clone())
        );
    }

    #[test]
    fn routing_tab_on_dock_cannot_reach_the_second_appstack_row() {
        use gpui::VisualTestContext;

        let mut app = TestAppContext::single();
        // Reproduce the historical sibling structure without editing source:
        // Dock owns the key listener; AppStack's scroll list is its sibling.
        let window = app.add_window(|_, cx| overflowing_fixture(cx, false));
        let _catalog_cleanup =
            TestCatalogCleanup(window.read(&app.app.borrow()).unwrap().catalog_root.clone());
        let context = VisualTestContext::from_window(window.into(), &app).into_mut();
        context.run_until_parked();
        let handles = {
            let cx = app.app.borrow();
            let harness = window.read(&cx).unwrap();
            harness
                .identities
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    harness
                        .focus
                        .handle(id)
                        .unwrap()
                        .clone()
                        .tab_index(index as isize)
                })
                .collect::<Vec<_>>()
        };
        context.update(|window, cx| window.focus(&handles[0], cx));
        context.run_until_parked();
        for _ in 0..2 {
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
            });
            context.run_until_parked();
        }
        context.update(|window, _| {
            assert!(
                handles[1].is_focused(window),
                "the first row is reachable but Tab cannot leave it through a sibling's listener"
            );
            assert!(
                !handles[2].is_focused(window),
                "the second row is unreachable with Dock-only routing"
            );
        });
    }

    #[test]
    fn distinct_product_identities_never_collide() {
        let seed = handles(2);
        let mut next = seed.into_iter();
        let mut state = ProductFocusState::new();
        // Same display label, different product reference.
        let a = ProductControlId("workspace:aa:11".into());
        let b = ProductControlId("workspace:bb:11".into());
        state.reconcile(&[a.clone(), b.clone()]);
        state.ensure(&a, &mut || next.next().expect("seeded handle"));
        state.ensure(&b, &mut || next.next().expect("seeded handle"));
        assert_ne!(
            state.handle(&a).unwrap().clone(),
            state.handle(&b).unwrap().clone(),
            "distinct product identities must own distinct handles"
        );
    }

    #[test]
    fn duplicate_application_labels_keep_distinct_ordered_focus_handles() {
        let seeded = handles(2);
        let mut seeded = seeded.into_iter();
        let first = id("app:636d616b65.desktop");
        let second = id("app:636d616b65-copy.desktop");
        let mut state = ProductFocusState::new();
        state.reconcile(&[first.clone(), second.clone()]);
        state.ensure(&first, &mut || seeded.next().unwrap());
        state.ensure(&second, &mut || seeded.next().unwrap());
        assert_ne!(state.handle(&first), state.handle(&second));
        assert_eq!(state.order(), &[first, second]);
    }
}
