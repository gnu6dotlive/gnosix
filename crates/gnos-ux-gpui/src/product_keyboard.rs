//! One keyboard routing point for the interactive product surface.
//!
//! The product scene is a persistent layer-shell surface, so keyboard
//! behaviour has to be decided in exactly one place. When it was decided
//! nowhere, a focused control was a semantic dead end: the focus landed,
//! and nothing could be done with it. Per-control Tab handlers would be
//! the wrong fix for that, because they would give every control its own
//! traversal opinion and could move focus more than once per keypress.
//!
//! The mapping is a pure function of the key and the shift modifier, so
//! the policy is testable without a compositor, a window, or a running
//! application, and so that the scene has one traversal order: gpui's,
//! which is built from `tab_stop` and `tab_index` and therefore cannot
//! drift from the accessibility tree.

/// What the scene should do with a key it has decided to handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKeyAction {
    /// Move focus to the next control this window presents.
    TraverseForward,
    /// Move focus to the previous control this window presents.
    TraverseBackward,
    /// An activation key on a focused control. The product must let it
    /// through untouched.
    ///
    /// gpui already turns Enter and Space on a focused clickable element
    /// into that element's own click, so the product's controls are
    /// already keyboard-activatable. Adding a second activation here would
    /// run the same product intent twice per keypress - once through
    /// gpui's button semantics and once through the product. So this
    /// action exists to say "do nothing, and in particular do not prevent
    /// the default", not to add behaviour.
    DeferToElement,
    /// Not a product key. The scene must leave it alone.
    Ignore,
}

/// The product's keyboard policy, as one pure function.
///
/// `key` is gpui's key name (`"tab"`, `"enter"`, `"space"`), and `shift`
/// is the shift modifier. Anything not named here is ignored, so an
/// unrelated key can never be swallowed by the product.
#[must_use]
pub fn route_scene_key(key: &str, shift: bool) -> SceneKeyAction {
    match key {
        // Shift+Tab reverses; plain Tab advances. Traversal stays inside
        // the window that has focus on purpose: crossing outputs is a
        // separate decision from arbitrating focus between them.
        "tab" if shift => SceneKeyAction::TraverseBackward,
        "tab" => SceneKeyAction::TraverseForward,
        "enter" | "space" => SceneKeyAction::DeferToElement,
        _ => SceneKeyAction::Ignore,
    }
}

/// Apply the shared traversal policy at the common ancestor of the product
/// controls. Enter/Space remain owned by GPUI's focused clickable element.
pub fn dispatch_scene_key(
    event: &gpui::KeyDownEvent,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    match route_scene_key(&event.keystroke.key, event.keystroke.modifiers.shift) {
        SceneKeyAction::TraverseForward => window.focus_next(cx),
        SceneKeyAction::TraverseBackward => window.focus_prev(cx),
        SceneKeyAction::DeferToElement | SceneKeyAction::Ignore => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_routes_to_forward_traversal() {
        assert_eq!(
            route_scene_key("tab", false),
            SceneKeyAction::TraverseForward
        );
    }

    #[test]
    fn shift_tab_routes_to_reverse_traversal() {
        assert_eq!(
            route_scene_key("tab", true),
            SceneKeyAction::TraverseBackward
        );
    }

    #[test]
    fn enter_is_left_to_the_focused_element() {
        // gpui's own button semantics activate on Enter. The product must
        // not also activate, or the intent runs twice.
        assert_eq!(
            route_scene_key("enter", false),
            SceneKeyAction::DeferToElement
        );
    }

    #[test]
    fn space_is_left_to_the_focused_element() {
        assert_eq!(
            route_scene_key("space", false),
            SceneKeyAction::DeferToElement
        );
    }

    #[test]
    fn an_activation_key_with_a_modifier_is_left_alone_too() {
        // gpui's button semantics require an unmodified key. The product
        // must not compensate by activating anyway, or Shift+Enter would
        // activate where gpui deliberately does not.
        assert_eq!(
            route_scene_key("enter", true),
            SceneKeyAction::DeferToElement
        );
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        for key in ["a", "escape", "arrowdown", "f5", "left", "up"] {
            assert_eq!(
                route_scene_key(key, false),
                SceneKeyAction::Ignore,
                "{key} must be left to the rest of the system"
            );
        }
    }

    #[test]
    fn traversal_and_activation_are_distinct_intents() {
        // Guards against a refactor that collapses the two.
        assert_ne!(
            route_scene_key("tab", false),
            route_scene_key("enter", false)
        );
        assert_ne!(route_scene_key("tab", false), route_scene_key("tab", true));
    }
}
