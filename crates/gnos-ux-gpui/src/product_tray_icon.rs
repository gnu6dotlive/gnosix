//! Realisation of a Tray icon source into something GPUI can paint.
//!
//! This is the stage between `TrayIconSource` (what the provider facts select)
//! and pixels: themed names go through the native icon-theme resolver, pixmaps
//! are converted in place, and a base with an overlay stays two images painted
//! one over the other. Application imagery is never recoloured and is never
//! replaced by a Gnosix icon: when nothing can be realised the result is an
//! explicit [`RealizedTrayIcon::Missing`] with its reason.

use std::{path::PathBuf, sync::Arc};

use gnosix_icon_theme::{
    IconFormat, IconLookup, IconRequest, IconThemeSearch, UnsupportedReason, lookup,
};
use gpui::{Div, ObjectFit, RenderImage, Styled, StyledImage, div, img, prelude::*, px};

use crate::{
    product_tray::TrayPixmap,
    product_tray_presentation::{TrayIconSource, closest_pixmap},
};

/// One paintable image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayImage {
    /// A resolved icon file; GPUI decodes PNG and rasterises SVG itself.
    File { path: PathBuf, format: IconFormat },
    /// Provider pixels, already in GPUI's BGRA layout (straight alpha).
    Pixels(Arc<RenderImage>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingReason {
    /// The item published no icon for the selected path.
    NoSource,
    NotFound(String),
    Unsupported(String),
    /// A pixmap whose byte length does not match its size.
    MalformedPixmap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RealizedTrayIcon {
    Image {
        base: TrayImage,
        /// Painted over the base, scaled to it. A missing overlay leaves the
        /// base alone, as the reference shell does.
        overlay: Option<TrayImage>,
    },
    Missing(MissingReason),
}

/// SNI pixmaps are ARGB32 in network byte order (`A, R, G, B` per pixel);
/// GPUI wants `B, G, R, A`, not premultiplied.
fn pixels_from_argb(pixmap: &TrayPixmap) -> Option<Arc<RenderImage>> {
    let (width, height) = (pixmap.width, pixmap.height);
    if width <= 0
        || height <= 0
        || pixmap.argb_bytes.len() != (width as usize) * (height as usize) * 4
    {
        return None;
    }
    let mut bgra = Vec::with_capacity(pixmap.argb_bytes.len());
    for pixel in pixmap.argb_bytes.chunks_exact(4) {
        bgra.extend_from_slice(&[pixel[3], pixel[2], pixel[1], pixel[0]]);
    }
    let image = image::RgbaImage::from_raw(width as u32, height as u32, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}

fn realize_single(
    source: &TrayIconSource,
    search: &IconThemeSearch,
    size: u32,
) -> Result<TrayImage, MissingReason> {
    match source {
        TrayIconSource::None => Err(MissingReason::NoSource),
        TrayIconSource::Named { name, theme_path } => {
            let request = IconRequest {
                name,
                size,
                scale: 1,
                item_theme_path: theme_path.as_deref().map(std::path::Path::new),
            };
            match lookup(search, &request) {
                IconLookup::Resolved(icon) => Ok(TrayImage::File {
                    path: icon.path,
                    format: icon.format,
                }),
                IconLookup::NotFound => Err(MissingReason::NotFound(name.clone())),
                IconLookup::Unsupported(UnsupportedReason::Format(format)) => {
                    Err(MissingReason::Unsupported(format))
                }
                IconLookup::Unsupported(UnsupportedReason::InvalidName) => {
                    Err(MissingReason::Unsupported("invalid name".to_owned()))
                }
            }
        }
        TrayIconSource::Pixmaps(pixmaps) => {
            let size = size as i32;
            let chosen = closest_pixmap((size, size), pixmaps).ok_or(MissingReason::NoSource)?;
            pixels_from_argb(chosen)
                .map(TrayImage::Pixels)
                .ok_or(MissingReason::MalformedPixmap)
        }
        // A composite is only meaningful at the top level.
        TrayIconSource::Composite { base, .. } => realize_single(base, search, size),
    }
}

/// Realise `source` for an icon box of `size` logical pixels.
pub fn realize_tray_icon(
    source: &TrayIconSource,
    search: &IconThemeSearch,
    size: u32,
) -> RealizedTrayIcon {
    match source {
        TrayIconSource::Composite { base, overlay } => match realize_single(base, search, size) {
            Ok(base) => RealizedTrayIcon::Image {
                base,
                overlay: realize_single(overlay, search, size).ok(),
            },
            Err(reason) => RealizedTrayIcon::Missing(reason),
        },
        other => match realize_single(other, search, size) {
            Ok(base) => RealizedTrayIcon::Image {
                base,
                overlay: None,
            },
            Err(reason) => RealizedTrayIcon::Missing(reason),
        },
    }
}

fn paint_image(image: &TrayImage) -> gpui::Img {
    match image {
        TrayImage::File { path, .. } => img(path.clone()),
        TrayImage::Pixels(pixels) => img(pixels.clone()),
    }
}

/// A fixed `size` square with the realised images stacked inside it. Nothing is
/// drawn for a missing icon: the slot stays, empty, and the reason stays in data.
pub fn tray_icon_element(realized: &RealizedTrayIcon, size: f32) -> Div {
    let slot = div().relative().w(px(size)).h(px(size)).flex_shrink_0();
    let RealizedTrayIcon::Image { base, overlay } = realized else {
        return slot;
    };
    let layer = |image: &TrayImage| {
        paint_image(image)
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(px(size))
            .h(px(size))
            .object_fit(ObjectFit::Contain)
    };
    let slot = slot.child(layer(base));
    match overlay {
        Some(overlay) => slot.child(layer(overlay)),
        None => slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU32, Ordering},
    };

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "gnosix-tray-icon-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "icon").unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn search(fixture: &Fixture) -> IconThemeSearch {
        let theme = "[Icon Theme]\nDirectories=24\n[24]\nSize=24\nType=Fixed\n";
        fs::create_dir_all(fixture.0.join("icons/Fix")).unwrap();
        fs::write(fixture.0.join("icons/Fix/index.theme"), theme).unwrap();
        IconThemeSearch {
            roots: vec![fixture.0.join("icons")],
            fallback_dirs: Vec::new(),
            theme: "Fix".into(),
        }
    }

    fn named(name: &str) -> TrayIconSource {
        TrayIconSource::Named {
            name: name.into(),
            theme_path: None,
        }
    }

    fn pixmap(width: i32, height: i32, argb: [u8; 4]) -> TrayPixmap {
        TrayPixmap {
            width,
            height,
            argb_bytes: argb.repeat((width * height) as usize),
        }
    }

    #[test]
    fn a_named_icon_realises_to_the_resolved_file() {
        let f = Fixture::new();
        let path = f.file("icons/Fix/24/mail.png");
        let realized = realize_tray_icon(&named("mail"), &search(&f), 24);
        assert_eq!(
            realized,
            RealizedTrayIcon::Image {
                base: TrayImage::File {
                    path,
                    format: IconFormat::Png
                },
                overlay: None
            }
        );
    }

    #[test]
    fn a_missing_icon_is_explicit_and_never_a_substitute() {
        let f = Fixture::new();
        assert_eq!(
            realize_tray_icon(&named("ghost"), &search(&f), 24),
            RealizedTrayIcon::Missing(MissingReason::NotFound("ghost".into()))
        );
        assert_eq!(
            realize_tray_icon(&TrayIconSource::None, &search(&f), 24),
            RealizedTrayIcon::Missing(MissingReason::NoSource)
        );
    }

    #[test]
    fn an_unsupported_format_is_reported_with_its_reason() {
        let f = Fixture::new();
        f.file("icons/Fix/24/old.xpm");
        assert_eq!(
            realize_tray_icon(&named("old"), &search(&f), 24),
            RealizedTrayIcon::Missing(MissingReason::Unsupported("xpm".into()))
        );
    }

    #[test]
    fn pixmaps_become_bgra_straight_alpha_and_keep_their_size() {
        let f = Fixture::new();
        // ARGB bytes A=0x80, R=0x10, G=0x20, B=0x30.
        let source = TrayIconSource::Pixmaps(vec![pixmap(2, 2, [0x80, 0x10, 0x20, 0x30])]);
        let RealizedTrayIcon::Image {
            base: TrayImage::Pixels(image),
            overlay: None,
        } = realize_tray_icon(&source, &search(&f), 24)
        else {
            panic!("expected pixels");
        };
        let bytes = image.as_bytes(0).unwrap();
        assert_eq!(&bytes[..4], &[0x30, 0x20, 0x10, 0x80]);
        assert_eq!(bytes.len(), 2 * 2 * 4);
        let size = image.size(0);
        assert_eq!((size.width.0, size.height.0), (2, 2));
    }

    #[test]
    fn the_closest_pixmap_size_is_realised() {
        let f = Fixture::new();
        let source = TrayIconSource::Pixmaps(vec![
            pixmap(16, 16, [0xff, 1, 2, 3]),
            pixmap(32, 32, [0xff, 4, 5, 6]),
            pixmap(64, 64, [0xff, 7, 8, 9]),
        ]);
        let RealizedTrayIcon::Image {
            base: TrayImage::Pixels(image),
            ..
        } = realize_tray_icon(&source, &search(&f), 24)
        else {
            panic!("expected pixels");
        };
        assert_eq!(image.size(0).width.0, 32);
    }

    #[test]
    fn a_malformed_pixmap_is_refused_not_repaired() {
        let f = Fixture::new();
        let short = TrayPixmap {
            width: 4,
            height: 4,
            argb_bytes: vec![0; 10],
        };
        assert_eq!(
            realize_tray_icon(&TrayIconSource::Pixmaps(vec![short]), &search(&f), 24),
            RealizedTrayIcon::Missing(MissingReason::MalformedPixmap)
        );
    }

    #[test]
    fn a_composite_keeps_base_and_overlay_as_two_images() {
        let f = Fixture::new();
        let base = f.file("icons/Fix/24/mail.png");
        let badge = f.file("icons/Fix/24/badge.png");
        let source = TrayIconSource::Composite {
            base: Box::new(named("mail")),
            overlay: Box::new(named("badge")),
        };
        assert_eq!(
            realize_tray_icon(&source, &search(&f), 24),
            RealizedTrayIcon::Image {
                base: TrayImage::File {
                    path: base,
                    format: IconFormat::Png
                },
                overlay: Some(TrayImage::File {
                    path: badge,
                    format: IconFormat::Png
                }),
            }
        );
    }

    #[test]
    fn a_missing_overlay_leaves_the_base_and_a_missing_base_leaves_nothing() {
        let f = Fixture::new();
        let base = f.file("icons/Fix/24/mail.png");
        let search = search(&f);
        let with_missing_overlay = TrayIconSource::Composite {
            base: Box::new(named("mail")),
            overlay: Box::new(named("no-badge")),
        };
        assert_eq!(
            realize_tray_icon(&with_missing_overlay, &search, 24),
            RealizedTrayIcon::Image {
                base: TrayImage::File {
                    path: base,
                    format: IconFormat::Png
                },
                overlay: None
            }
        );
        let missing_base = TrayIconSource::Composite {
            base: Box::new(named("no-mail")),
            overlay: Box::new(named("mail")),
        };
        assert_eq!(
            realize_tray_icon(&missing_base, &search, 24),
            RealizedTrayIcon::Missing(MissingReason::NotFound("no-mail".into()))
        );
    }

    #[test]
    fn the_item_theme_path_reaches_the_resolver() {
        let f = Fixture::new();
        let private = f.file("itemdir/private.png");
        let source = TrayIconSource::Named {
            name: "private".into(),
            theme_path: Some(f.0.join("itemdir").to_string_lossy().into_owned()),
        };
        assert_eq!(
            realize_tray_icon(&source, &search(&f), 24),
            RealizedTrayIcon::Image {
                base: TrayImage::File {
                    path: private,
                    format: IconFormat::Png
                },
                overlay: None
            }
        );
    }
}
