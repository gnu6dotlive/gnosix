//! A captured window brought down to its miniature's size.
//!
//! The capture is the window's own size (a 2560 × 1440 browser is 14 MB);
//! the miniature is a few dozen pixels. Each target pixel is the mean of the
//! source pixels it covers (a box filter), which keeps text and edges as a
//! texture rather than aliasing them. Pixels stay 4 bytes in the source's
//! order (BGRA for the formats taken here), and an opaque format's padding
//! byte becomes full alpha.

/// The miniature's size for a source of `src` drawn within `max`, keeping
/// the source's shape. Never 0, never larger than the source.
pub fn fit((sw, sh): (u32, u32), (mw, mh): (u32, u32)) -> (u32, u32) {
    if sw == 0 || sh == 0 || mw == 0 || mh == 0 {
        return (1, 1);
    }
    let k = (mw as f64 / sw as f64).min(mh as f64 / sh as f64).min(1.0);
    (
        ((sw as f64 * k).round() as u32).clamp(1, sw),
        ((sh as f64 * k).round() as u32).clamp(1, sh),
    )
}

/// Box-filter `src` (`sw` × `sh`, `stride` bytes per row, 4 bytes per
/// pixel) down to `tw` × `th`. With `opaque`, the fourth byte is padding and
/// the result is fully opaque.
pub fn downscale(
    src: &[u8],
    (sw, sh): (u32, u32),
    stride: usize,
    (tw, th): (u32, u32),
    opaque: bool,
) -> Option<Vec<u8>> {
    let (sw, sh, tw, th) = (sw as usize, sh as usize, tw as usize, th as usize);
    if sw == 0 || sh == 0 || tw == 0 || th == 0 || tw > sw || th > sh || stride < sw * 4 {
        return None;
    }
    if src.len() < stride * (sh - 1) + sw * 4 {
        return None;
    }
    let mut out = vec![0u8; tw * th * 4];
    for ty in 0..th {
        let (y0, y1) = (ty * sh / th, ((ty + 1) * sh / th).max(ty * sh / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * sw / tw, ((tx + 1) * sw / tw).max(tx * sw / tw + 1));
            let mut acc = [0u64; 4];
            for y in y0..y1 {
                let row = &src[y * stride + x0 * 4..y * stride + x1 * 4];
                for px in row.chunks_exact(4) {
                    for (a, &v) in acc.iter_mut().zip(px) {
                        *a += u64::from(v);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            let o = (ty * tw + tx) * 4;
            for c in 0..4 {
                out[o + c] = ((acc[c] + n / 2) / n) as u8;
            }
            if opaque {
                out[o + 3] = 0xff;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_miniature_keeps_the_window_shape_inside_its_box() {
        assert_eq!(fit((2560, 1440), (160, 160)), (160, 90));
        assert_eq!(fit((1080, 1920), (160, 90)), (51, 90));
        // A source smaller than the box is not enlarged.
        assert_eq!(fit((100, 50), (400, 400)), (100, 50));
        assert_eq!(fit((0, 50), (40, 40)), (1, 1));
    }

    #[test]
    fn a_pixel_is_the_mean_of_what_it_covers() {
        // 2 × 2 → 1 × 1: B 0/100/200/100, G 10, R 20, A 255.
        let src = [
            0, 10, 20, 255, 100, 10, 20, 255, //
            200, 10, 20, 255, 100, 10, 20, 255,
        ];
        assert_eq!(
            downscale(&src, (2, 2), 8, (1, 1), false).unwrap(),
            vec![100, 10, 20, 255]
        );
    }

    #[test]
    fn an_opaque_format_comes_out_opaque_and_the_stride_is_honoured() {
        // 1 × 2 with 4 bytes of row padding; padding byte 0 must not leak.
        let src = [
            50, 60, 70, 0, 9, 9, 9, 9, //
            50, 60, 70, 0, 9, 9, 9, 9,
        ];
        assert_eq!(
            downscale(&src, (1, 2), 8, (1, 1), true).unwrap(),
            vec![50, 60, 70, 255]
        );
    }

    #[test]
    fn nonsense_is_refused_not_read_past() {
        assert!(downscale(&[0; 4], (2, 2), 8, (1, 1), false).is_none());
        assert!(downscale(&[0; 16], (2, 2), 4, (1, 1), false).is_none());
        assert!(downscale(&[0; 16], (2, 2), 8, (3, 1), false).is_none());
    }
}
