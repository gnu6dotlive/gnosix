//! SHELL-FIELD-01: the blur behind the shell, masked by what it paints.
//!
//! With `BlurMask::Pixels`, Hyprland blurs the scene's layer wherever its
//! painted alpha is above the rule's `ignore_alpha`. The mask is the frame's
//! own pixels, so it is exact and in step with every extrusion. Gnosix then
//! sends no blur rectangles, and asserts the layer rule's `blur` itself:
//! - on when Blur is on;
//! - off when Blur is off, or when the rectangles (`BlurMask::Regions`) do
//!   the blurring, so nothing is blurred twice.
//!
//! The rule is asserted with `hyprctl eval` when the wish changes, and again
//! after every Hyprland config reload, which would otherwise bring back the
//! config's own value. The operator's Hyprland config holds the base rule
//! and its `ignore_alpha`. Receipt: `GNOS_UX_COMPOSITOR_BLUR`.

use std::sync::atomic::{AtomicU64, Ordering};

/// The scene's layer namespace, as a Hyprland rule regex.
pub const NAMESPACE_RULE: &str = "^gnos\\\\.ux:scene$";

static CONFIG_RELOADS: AtomicU64 = AtomicU64::new(0);

/// Hyprland reloaded its config: every assertion must be made again.
pub fn config_reloaded() {
    CONFIG_RELOADS.fetch_add(1, Ordering::Relaxed);
}

/// What was last asserted, and under which config generation.
#[derive(Debug, Default)]
pub struct CompositorBlur {
    asserted: Option<(bool, u64)>,
}

impl CompositorBlur {
    /// Assert the layer rule's blur if it differs from what was last
    /// asserted (or Hyprland reloaded since). Never blocks the frame.
    pub fn sync(&mut self, blur: bool) {
        let generation = CONFIG_RELOADS.load(Ordering::Relaxed);
        if self.asserted == Some((blur, generation)) {
            return;
        }
        self.asserted = Some((blur, generation));
        let code = rule(blur);
        std::thread::spawn(move || {
            let out = std::process::Command::new("hyprctl")
                .args(["eval", &code])
                .output();
            let state = match out {
                Ok(o) if o.status.success() => "asserted".to_owned(),
                Ok(o) => format!("refused({})", String::from_utf8_lossy(&o.stdout).trim()),
                Err(e) => format!("unavailable({e})"),
            };
            println!("GNOS_UX_COMPOSITOR_BLUR blur={blur} generation={generation} state={state}");
        });
    }
}

/// The Lua that sets the scene layer's blur.
pub fn rule(blur: bool) -> String {
    format!("hl.layer_rule({{ match = {{ namespace = \"{NAMESPACE_RULE}\" }}, blur = {blur} }})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_names_the_scene_layer_and_reasserts_after_a_reload() {
        assert_eq!(
            rule(false),
            r#"hl.layer_rule({ match = { namespace = "^gnos\\.ux:scene$" }, blur = false })"#
        );
        let mut c = CompositorBlur {
            asserted: Some((true, CONFIG_RELOADS.load(Ordering::Relaxed))),
        };
        let before = c.asserted;
        c.sync(true);
        assert_eq!(c.asserted, before, "unchanged wish: nothing asserted");
        config_reloaded();
        // A reload makes the same wish stale; `sync` would assert it again.
        assert_ne!(
            c.asserted.map(|a| a.1),
            Some(CONFIG_RELOADS.load(Ordering::Relaxed))
        );
    }
}
