//! CURSOR-01: the pointer's shape, from what it is over and what is under
//! way.
//!
//! The shell hit-tests its own surfaces (the Bar, its panels, the Deck), so
//! GPUI's per-element cursor never applies to them: until now the pointer
//! stayed an arrow over everything. Each surface now says what the pointer
//! is over, in terms of meaning ([`Affordance`]); the scene adds what is
//! under way (a window or a module carried, an action waiting for its
//! answer), and [`cursor_style`] gives the one shape for the frame.
//!
//! The shapes are cursor-shape-v1's (CSS names): the compositor draws them
//! from the operator's theme, whatever it is.
//!
//! Precedence, from the first that applies:
//! 1. **Carrying** something: `grabbing`, or `no-drop` over a place that
//!    refuses it.
//! 2. **Text** under the pointer: `text`, even while busy (typing goes on).
//! 3. **Busy** (an action waiting for its answer, the interface still
//!    usable): `progress`.
//! 4. **What it is over**: `pointer` for what a press acts on, `grab` for
//!    what can be lifted, `context-menu` for what opens a menu, `help`,
//!    and the arrow for the rest, a disabled control included.

use gpui::CursorStyle;

/// What the pointer is over, as the surface under it means it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affordance {
    /// Nothing that answers the pointer.
    #[default]
    Inert,
    /// A press acts on it (a module, a row, a button, a switch, a slider).
    Press,
    /// It can be lifted and carried (a Bar module, a window in the
    /// workspaces panel, a Deck card).
    Grab,
    /// A press opens its menu (a tray item whose action is its menu).
    Menu,
    /// Text can be typed or selected there.
    Text,
    /// It explains itself.
    Help,
    /// It would act, but cannot now.
    Disabled,
}

/// Something carried by the pointer, and whether the place under it takes
/// it (`None`: not over a place that could).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carry {
    pub drop: Option<bool>,
}

/// What decides the shape this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorIntent {
    pub over: Affordance,
    pub carry: Option<Carry>,
    /// An action is waiting for its answer; the interface is still usable.
    pub busy: bool,
}

/// The one shape for this frame.
pub fn cursor_style(intent: CursorIntent) -> CursorStyle {
    if let Some(carry) = intent.carry {
        return if carry.drop == Some(false) {
            CursorStyle::NoDrop
        } else {
            CursorStyle::ClosedHand
        };
    }
    if intent.over == Affordance::Text {
        return CursorStyle::IBeam;
    }
    if intent.busy {
        return CursorStyle::Progress;
    }
    match intent.over {
        Affordance::Press => CursorStyle::PointingHand,
        Affordance::Grab => CursorStyle::OpenHand,
        Affordance::Menu => CursorStyle::ContextualMenu,
        Affordance::Help => CursorStyle::Help,
        Affordance::Text => CursorStyle::IBeam,
        Affordance::Inert | Affordance::Disabled => CursorStyle::Arrow,
    }
}

/// The CSS name of a shape (receipts).
pub fn css_name(style: CursorStyle) -> &'static str {
    match style {
        CursorStyle::Arrow => "default",
        CursorStyle::IBeam => "text",
        CursorStyle::Crosshair => "crosshair",
        CursorStyle::ClosedHand => "grabbing",
        CursorStyle::OpenHand => "grab",
        CursorStyle::PointingHand => "pointer",
        CursorStyle::ResizeLeft => "w-resize",
        CursorStyle::ResizeRight => "e-resize",
        CursorStyle::ResizeLeftRight => "ew-resize",
        CursorStyle::ResizeUp => "n-resize",
        CursorStyle::ResizeDown => "s-resize",
        CursorStyle::ResizeUpDown => "ns-resize",
        CursorStyle::ResizeUpLeftDownRight => "nwse-resize",
        CursorStyle::ResizeUpRightDownLeft => "nesw-resize",
        CursorStyle::ResizeColumn => "col-resize",
        CursorStyle::ResizeRow => "row-resize",
        CursorStyle::IBeamCursorForVerticalLayout => "vertical-text",
        CursorStyle::OperationNotAllowed => "not-allowed",
        CursorStyle::DragLink => "alias",
        CursorStyle::DragCopy => "copy",
        CursorStyle::ContextualMenu => "context-menu",
        CursorStyle::Progress => "progress",
        CursorStyle::Wait => "wait",
        CursorStyle::Help => "help",
        CursorStyle::Move => "move",
        CursorStyle::AllScroll => "all-scroll",
        CursorStyle::ZoomIn => "zoom-in",
        CursorStyle::ZoomOut => "zoom-out",
        CursorStyle::Cell => "cell",
        CursorStyle::NoDrop => "no-drop",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn over(a: Affordance) -> CursorIntent {
        CursorIntent {
            over: a,
            ..Default::default()
        }
    }

    #[test]
    fn what_the_pointer_is_over_gives_its_shape() {
        assert_eq!(cursor_style(over(Affordance::Inert)), CursorStyle::Arrow);
        assert_eq!(
            cursor_style(over(Affordance::Press)),
            CursorStyle::PointingHand
        );
        assert_eq!(cursor_style(over(Affordance::Grab)), CursorStyle::OpenHand);
        assert_eq!(
            cursor_style(over(Affordance::Menu)),
            CursorStyle::ContextualMenu
        );
        assert_eq!(cursor_style(over(Affordance::Text)), CursorStyle::IBeam);
        assert_eq!(cursor_style(over(Affordance::Help)), CursorStyle::Help);
        assert_eq!(cursor_style(over(Affordance::Disabled)), CursorStyle::Arrow);
    }

    #[test]
    fn carrying_comes_first_then_text_then_busy() {
        let carrying = |drop| CursorIntent {
            over: Affordance::Press,
            carry: Some(Carry { drop }),
            busy: true,
        };
        assert_eq!(cursor_style(carrying(None)), CursorStyle::ClosedHand);
        assert_eq!(cursor_style(carrying(Some(true))), CursorStyle::ClosedHand);
        assert_eq!(cursor_style(carrying(Some(false))), CursorStyle::NoDrop);
        let busy = |a| CursorIntent {
            over: a,
            carry: None,
            busy: true,
        };
        assert_eq!(cursor_style(busy(Affordance::Text)), CursorStyle::IBeam);
        assert_eq!(cursor_style(busy(Affordance::Press)), CursorStyle::Progress);
        assert_eq!(cursor_style(busy(Affordance::Inert)), CursorStyle::Progress);
    }

    #[test]
    fn every_shape_has_its_css_name() {
        assert_eq!(css_name(CursorStyle::Progress), "progress");
        assert_eq!(css_name(CursorStyle::NoDrop), "no-drop");
        assert_eq!(css_name(CursorStyle::ContextualMenu), "context-menu");
    }
}
