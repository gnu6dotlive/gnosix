//! BAR-02A: the overflow panel in the scene.
//!
//! The panel (`overflow_panel`) is pure. Here it learns which modules gave
//! way, where the "…" stands, the tray's items and the window's title, and
//! what a click on a row does: each hidden module is used through the
//! authority that already carries it out, and its panel stands beside the
//! "…" (`module_anchor`). Receipts: `GNOS_UX_OVERFLOW`,
//! `GNOS_UX_OVERFLOW_ACTION`.

use super::*;
use crate::overflow_panel::{OverflowDetails, OverflowOutcome, OverflowPick};
use crate::reference_bar::IdleBarEntry;

impl SceneHost {
    /// The centre along the Bar at rest on `side` of the module a panel
    /// stands beside: `entry` itself, or the "…" while `entry` is hidden.
    pub(super) fn module_anchor(
        &self,
        side: crate::rail_topology::RailSide,
        entry: IdleBarEntry,
    ) -> Option<f32> {
        let (s, placements) = self.bar_rest_previous.as_ref()?;
        (*s == side).then_some(())?;
        let at = |entry: IdleBarEntry| placements.iter().find(|m| m.entry == entry);
        let m = at(entry).or_else(|| {
            self.bar_fit_hidden
                .contains(&entry)
                .then(|| at(IdleBarEntry::Overflow))
                .flatten()
        })?;
        Some(if side.is_horizontal() {
            m.rect.x + m.rect.width / 2.0
        } else {
            m.rect.y + m.rect.height / 2.0
        })
    }

    /// The "…" centre in scene space, where a tray item used from the panel
    /// is anchored.
    fn overflow_point(&self) -> Option<(f32, f32)> {
        let (_, placements) = self.bar_rest_previous.as_ref()?;
        let m = placements
            .iter()
            .find(|m| m.entry == IdleBarEntry::Overflow)?;
        Some((
            m.rect.x + m.rect.width / 2.0,
            m.rect.y + m.rect.height / 2.0,
        ))
    }

    /// The tray's items (those the Bar shows) and the window's title.
    fn overflow_details(&self) -> OverflowDetails {
        let tray = match self.product_runtime.tray() {
            crate::product_tray::TraySnapshot::Active { items } => items
                .iter()
                .filter(|item| item.status != crate::product_tray::TrayStatus::Passive)
                .map(|item| {
                    [
                        item.title.as_str(),
                        item.tooltip.as_ref().map_or("", |t| t.title.as_str()),
                        item.id.as_str(),
                    ]
                    .into_iter()
                    .find(|t| !t.is_empty())
                    .unwrap_or_default()
                    .to_owned()
                })
                .collect(),
            _ => Vec::new(),
        };
        OverflowDetails {
            tray,
            active_window: self.bar_active_title.clone(),
        }
    }

    pub(super) fn sync_overflow(
        &mut self,
        side: crate::rail_topology::RailSide,
        viewport: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let anchor = self.module_anchor(side, IdleBarEntry::Overflow);
        let hidden = self.bar_fit_hidden.clone();
        let details = self.overflow_details();
        if self.overflow.set_details(details) | self.overflow.place(side, viewport, anchor, &hidden)
        {
            self.report_overflow("changed");
            if self.overflow.needs_frame() {
                self.start_motion_driver(cx);
            }
            cx.notify();
        }
    }

    /// A click on the "…": through the shell's attention (FOCUS-01).
    pub(super) fn toggle_overflow(&mut self, cx: &mut Context<Self>) {
        self.attention_toggle(crate::attention::Attendant::Overflow, cx);
    }

    /// The panel out or in, as the shell's attention decided.
    pub(super) fn overflow_set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.overflow.is_open() != open && self.overflow.toggle() {
            self.report_overflow(if self.overflow.is_open() {
                "opening"
            } else {
                "closing"
            });
            self.start_motion_driver(cx);
            cx.notify();
        }
    }

    pub(super) fn report_overflow(&self, state: &str) {
        let hidden = self
            .overflow
            .hidden()
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join(",");
        // The rows in scene space, once the panel has a frame (LIVE-03).
        let rows = self
            .overflow
            .frame()
            .map(|f| {
                self.overflow
                    .rows()
                    .iter()
                    .filter_map(|r| match r {
                        crate::overflow_panel::OverflowRow::Item { pick, .. } => {
                            let (x, y, w, h) = r.rect();
                            Some(format!(
                                "{pick:?}@[{:.1},{:.1},{w:.1},{h:.1}]",
                                f.rect.left() + x,
                                f.rect.top() + y
                            ))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_default();
        println!(
            "GNOS_UX_OVERFLOW state={state} open={} hidden=[{hidden}] rows=[{rows}]",
            self.overflow.is_open()
        );
    }

    /// Carry out what the panel asks; returns whether it consumed the event.
    pub(super) fn apply_overflow_outcome(
        &mut self,
        outcome: OverflowOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(pick) = outcome.pick {
            self.overflow.close();
            self.report_overflow("closing");
            let done = match pick {
                OverflowPick::Module(entry) => self.use_hidden_module(entry, cx),
                OverflowPick::TrayItem(index) => self
                    .overflow_point()
                    .is_some_and(|at| self.rail_tray(index, false, at, cx)),
            };
            println!("GNOS_UX_OVERFLOW_ACTION pick={pick:?} done={done}");
        }
        if outcome.changed || outcome.pick.is_some() {
            self.start_motion_driver(cx);
            cx.notify();
        }
        outcome.consumed
    }

    /// Use a hidden module as its click would on the Bar. The panels that
    /// stand beside a module stand beside the "…" while it is hidden. The
    /// active window and a tray without items have no action of their own.
    pub(super) fn use_hidden_module(
        &mut self,
        entry: IdleBarEntry,
        cx: &mut Context<Self>,
    ) -> bool {
        match entry {
            IdleBarEntry::Power => {
                self.toggle_session(cx);
                true
            }
            IdleBarEntry::StatusIcons => {
                self.toggle_utilities(cx);
                true
            }
            IdleBarEntry::Clock => {
                self.attention_toggle(crate::attention::Attendant::Dashboard, cx);
                self.report_dashboard("pin");
                true
            }
            IdleBarEntry::Workspaces => {
                self.attention_toggle(crate::attention::Attendant::Workspaces, cx);
                true
            }
            _ => false,
        }
    }
}
