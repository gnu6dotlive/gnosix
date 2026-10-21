//! BAR-02B: the Bar from the keyboard.
//!
//! `SIGUSR1` (the operator binds a key to send it, e.g. `systemctl --user
//! kill -s USR1 gnos-ux-scene.service`) asks the output Hyprland has focused
//! to take the keyboard. Its layer turns `Exclusive` for as long as the Bar
//! is driven, then returns to `OnDemand`; the runtime change is the
//! `set_keyboard_interactivity` patch in vendor GPUI.
//!
//! - **Arrows** move between modules along the Bar (either axis).
//! - **Enter** or **Space** uses the module as a click would, and gives the
//!   keyboard back.
//! - **Shift + arrows** move the module one place earlier or later in the
//!   order, across groups at their ends; the profile learns it as a drop
//!   would (BAR-01I).
//! - **Escape**, a second signal, a press anywhere or 30 s without a key
//!   give the keyboard back, so the shell never keeps it by accident.

use super::*;
use crate::bar_strip::{ModulePlacement, moved};
use crate::reference_bar::{BarModuleOrder, IdleBarEntry};

/// The keyboard returns after this long without a key.
const IDLE: Duration = Duration::from_secs(30);

/// One process-wide request: the output that should take the keyboard.
pub(crate) struct BarKeysSignal {
    pub generation: u64,
    pub output: Option<String>,
}

struct BarKeysGlobal(gpui::Entity<BarKeysSignal>);
impl gpui::Global for BarKeysGlobal {}

impl BarKeysSignal {
    /// The shared request, installing the `SIGUSR1` listener on first use.
    pub(crate) fn shared(cx: &mut App) -> gpui::Entity<Self> {
        if let Some(global) = cx.try_global::<BarKeysGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| Self {
            generation: 0,
            output: None,
        });
        cx.set_global(BarKeysGlobal(entity.clone()));
        let (sender, mut receiver) = async_mpsc::unbounded::<Option<String>>();
        match Signals::new([signal_hook::consts::SIGUSR1]) {
            Ok(mut signals) => {
                let spawned = thread::Builder::new()
                    .name("gnos-ux-bar-keys".to_owned())
                    .spawn(move || {
                        for _ in signals.forever() {
                            if sender.unbounded_send(focused_output()).is_err() {
                                break;
                            }
                        }
                    });
                if let Err(error) = spawned {
                    println!("GNOS_UX_BAR_KEYS state=unavailable reason={error}");
                }
            }
            Err(error) => println!("GNOS_UX_BAR_KEYS state=unavailable reason={error}"),
        }
        let target = entity.clone();
        cx.spawn(async move |cx| {
            while let Some(output) = receiver.next().await {
                target.update(cx, |this, cx| {
                    this.generation += 1;
                    this.output = output;
                    cx.notify();
                });
            }
        })
        .detach();
        entity
    }
}

/// The connector Hyprland has focused (`hyprctl -j monitors`).
fn focused_output() -> Option<String> {
    let out = std::process::Command::new("hyprctl")
        .args(["-j", "monitors"])
        .output()
        .ok()?;
    let monitors: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    monitors
        .as_array()?
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))
        .and_then(|m| m["name"].as_str())
        .map(str::to_owned)
}

/// The Bar being driven from the keyboard.
#[derive(Clone, Debug)]
pub(super) struct BarKeys {
    /// The module the keyboard is on.
    pub entry: IdleBarEntry,
    pub last_key: Instant,
}

/// What the next frame does with the layer's keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BarKeysWant {
    Take,
    Give,
}

impl SceneHost {
    /// The modules at rest, along the Bar.
    fn bar_keys_modules(&self) -> Vec<ModulePlacement> {
        let Some((side, placements)) = self.bar_rest_previous.as_ref() else {
            return Vec::new();
        };
        let mut modules: Vec<ModulePlacement> = placements
            .iter()
            .filter(|m| m.entry != IdleBarEntry::Spacer)
            .copied()
            .collect();
        if side.is_horizontal() {
            modules.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        } else {
            modules.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
        }
        modules
    }

    /// The signal reached this process: take the keyboard if this output is
    /// the focused one, or give it back if the Bar is already driven.
    pub(super) fn bar_keys_signal(&mut self, output: Option<&str>, cx: &mut Context<Self>) {
        let here = self.runtime_output.as_ref().map(RuntimeId::as_str);
        if self.bar_keys.is_some() {
            self.bar_keys_want = Some(BarKeysWant::Give);
        } else if output.is_some() && output == here {
            self.bar_keys_want = Some(BarKeysWant::Take);
        } else {
            return;
        }
        cx.notify();
    }

    /// Apply a pending take or give, with the window (each frame).
    pub(super) fn bar_keys_apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(keys) = &self.bar_keys
            && keys.last_key.elapsed() >= IDLE
        {
            self.bar_keys_want = Some(BarKeysWant::Give);
        }
        match self.bar_keys_want.take() {
            Some(BarKeysWant::Take) => {
                let Some(first) = self.bar_keys_modules().first().map(|m| m.entry) else {
                    println!("GNOS_UX_BAR_KEYS state=refused reason=no_modules");
                    return;
                };
                window.set_keyboard_interactivity(
                    gpui::layer_shell::KeyboardInteractivity::Exclusive,
                );
                window.focus(&self.settings_focus, cx);
                self.bar_keys = Some(BarKeys {
                    entry: first,
                    last_key: Instant::now(),
                });
                println!("GNOS_UX_BAR_KEYS state=start module={first:?}");
                self.schedule_bar_keys_idle(cx);
                cx.notify();
            }
            Some(BarKeysWant::Give) => {
                window
                    .set_keyboard_interactivity(gpui::layer_shell::KeyboardInteractivity::OnDemand);
                if self.bar_keys.take().is_some() {
                    println!("GNOS_UX_BAR_KEYS state=stop");
                }
                cx.notify();
            }
            None => {}
        }
    }

    /// Wake when the idle limit could have passed.
    fn schedule_bar_keys_idle(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(IDLE).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// A press anywhere gives the keyboard back.
    pub(super) fn bar_keys_pointer(&mut self, cx: &mut Context<Self>) {
        if self.bar_keys.is_some() {
            self.bar_keys_want = Some(BarKeysWant::Give);
            cx.notify();
        }
    }

    /// A key while the Bar is driven; true when it was the Bar's.
    pub(super) fn bar_keys_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(keys) = self.bar_keys.as_mut() else {
            return false;
        };
        keys.last_key = Instant::now();
        let entry = keys.entry;
        let shift = event.keystroke.modifiers.shift;
        let modules = self.bar_keys_modules();
        let at = modules.iter().position(|m| m.entry == entry).unwrap_or(0);
        let step: Option<isize> = match event.keystroke.key.as_str() {
            "left" | "up" => Some(-1),
            "right" | "down" => Some(1),
            _ => None,
        };
        match (event.keystroke.key.as_str(), step) {
            (_, Some(dir)) if shift => {
                let outcome = self.bar_keys_move(entry, dir, cx);
                println!(
                    "GNOS_UX_BAR_KEYS state=move module={entry:?} dir={dir} outcome={outcome}"
                );
            }
            (_, Some(dir)) => {
                let next = (at as isize + dir).clamp(0, modules.len() as isize - 1) as usize;
                if let (Some(m), Some(keys)) = (modules.get(next), self.bar_keys.as_mut()) {
                    keys.entry = m.entry;
                    println!("GNOS_UX_BAR_KEYS state=focus module={:?}", m.entry);
                }
            }
            ("enter" | "space", None) => {
                let done = self.bar_keys_use(entry, cx);
                println!("GNOS_UX_BAR_KEYS state=use module={entry:?} done={done}");
                self.bar_keys_want = Some(BarKeysWant::Give);
            }
            ("escape", None) => self.bar_keys_want = Some(BarKeysWant::Give),
            _ => {}
        }
        cx.notify();
        true
    }

    /// Move `entry` one place along the order, across a group's end.
    fn bar_keys_move(
        &mut self,
        entry: IdleBarEntry,
        dir: isize,
        cx: &mut Context<Self>,
    ) -> &'static str {
        let Some(order) = self.bar_rest_order.clone() else {
            return "NO_ORDER";
        };
        let Some((g, i)) = order
            .0
            .iter()
            .enumerate()
            .find_map(|(g, group)| group.iter().position(|e| *e == entry).map(|i| (g, i)))
        else {
            return "HIDDEN";
        };
        let to = if dir < 0 {
            if i > 0 {
                Some((g, i - 1))
            } else if g > 0 {
                Some((g - 1, order.0[g - 1].len()))
            } else {
                None
            }
        } else if i + 1 < order.0[g].len() {
            Some((g, i + 1))
        } else if g < 2 {
            Some((g + 1, 0))
        } else {
            None
        };
        let Some(to) = to else {
            return "AT_END";
        };
        let next: BarModuleOrder = moved(&order, entry, Some(to));
        let outcome = self.write_module_order(&next, cx);
        if outcome == "SENT" {
            self.bar_order_pending = Some((next, Instant::now()));
        }
        outcome
    }

    /// Use a module as a click on it would.
    fn bar_keys_use(&mut self, entry: IdleBarEntry, cx: &mut Context<Self>) -> bool {
        match entry {
            IdleBarEntry::Overflow => {
                self.toggle_overflow(cx);
                true
            }
            other => self.use_hidden_module(other, cx),
        }
    }

    /// The ring around the module the keyboard is on.
    pub(super) fn bar_keys_layer(&self) -> Option<gpui::Div> {
        let keys = self.bar_keys.as_ref()?;
        let m = self
            .bar_keys_modules()
            .into_iter()
            .find(|m| m.entry == keys.entry)?;
        let r = m.clip;
        Some(
            div()
                .absolute()
                .left(px(r.x - 3.0))
                .top(px(r.y - 3.0))
                .w(px(r.width + 6.0))
                .h(px(r.height + 6.0))
                .rounded(px(10.0))
                .border_2()
                .border_color(rgba(crate::reference_bar::COLOR_PRIMARY)),
        )
    }
}
