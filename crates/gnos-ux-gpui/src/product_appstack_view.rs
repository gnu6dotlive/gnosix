//! Shared AppStack presentation components used by the installed scene and lab.

use gpui::{AnyElement, IntoElement, Styled};

use crate::{
    product_appstack::AppStackFeedback,
    product_design::{ProductDesignContext, ProductTextStyle, text},
};

/// Render product-owned launch feedback with the same treatment everywhere.
/// The semantic message and code come from `product_appstack::launch_feedback`;
/// this module only presents that decision.
pub fn feedback(feedback: &AppStackFeedback, design: ProductDesignContext) -> AnyElement {
    text(feedback.message.clone(), ProductTextStyle::Caption, design)
        .flex_shrink_0()
        .into_any_element()
}
