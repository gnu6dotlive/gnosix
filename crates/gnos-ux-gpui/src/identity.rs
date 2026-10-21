//! Gnosix's identity in the shell: the cube on the Session button.
//!
//! The Bar has no logo button of its own: the donor's (a logo that opened an
//! app launcher) is not booted, since its role is the AppStack in the Dock.
//! The Gnosix cube stands on the Session button instead, where the power
//! symbol stood, unless the operator keeps the symbol (Settings › Advanced).
//!
//! The cube is the design system's alpha master, resampled to the sizes
//! embedded here (`assets/identity`, see its README); it is painted as an
//! image, never recoloured.

use gpui::{Div, Image, ImageFormat, ParentElement, Styled, div, img, px};
use std::sync::{Arc, OnceLock};

const CUBE_64: &[u8] = include_bytes!("../assets/identity/gnosix-cube-64.png");
const CUBE_128: &[u8] = include_bytes!("../assets/identity/gnosix-cube-128.png");

fn cube_image(size: f32) -> Arc<Image> {
    static SMALL: OnceLock<Arc<Image>> = OnceLock::new();
    static LARGE: OnceLock<Arc<Image>> = OnceLock::new();
    // The larger source keeps the cube sharp once scaled above 32 px.
    let (cell, bytes) = if size > 32.0 {
        (&LARGE, CUBE_128)
    } else {
        (&SMALL, CUBE_64)
    };
    cell.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())))
        .clone()
}

/// The Gnosix cube, `size` px square.
pub fn cube(size: f32) -> Div {
    div()
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .child(img(cube_image(size)).w(px(size)).h(px(size)))
}

/// Whether the Session button carries the cube (else the power symbol).
pub fn session_logo() -> bool {
    #[cfg(feature = "component-lab")]
    return crate::surface_prefs::advanced("session_logo") != 0;
    #[cfg(not(feature = "component-lab"))]
    true
}

/// The Session button's mark: the cube, or the power symbol `icon` in
/// `colour` when the operator keeps it.
pub fn session_mark(icon: &str, size: f32, colour: u32) -> Div {
    let mark = if session_logo() {
        cube(size + 4.0)
    } else {
        crate::icons::icon(icon.to_owned(), size, colour)
    };
    div().flex().items_center().justify_center().child(mark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_cubes_are_square_pngs() {
        for (bytes, side) in [(CUBE_64, 64u32), (CUBE_128, 128)] {
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            // IHDR width and height, big-endian, right after the signature.
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((w, h), (side, side));
        }
    }
}
