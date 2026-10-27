//! WALL-01: the wallpaper, drawn by Gnosix on each output's desktop layer.
//!
//! The operator's choice lives in `~/.config/gnosix/wallpaper.json`: whether
//! Gnosix draws a wallpaper at all (another shell may draw its own on the
//! same layer), the picture, and how it fits. The picture is prepared once
//! per output size, off the UI thread, into `~/.cache/gnosix/wallpapers/`:
//! a 6000-pixel photograph is never uploaded whole to each output.

use image::{GenericImageView, RgbaImage, imageops::FilterType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the picture fills an output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperFit {
    /// Scaled to cover the output, the overflow cropped evenly.
    #[default]
    Cover,
    /// Scaled to fit inside, on black.
    Contain,
    /// At its own size, centred (cropped when larger).
    Center,
}

impl WallpaperFit {
    pub fn next(self) -> Self {
        match self {
            Self::Cover => Self::Contain,
            Self::Contain => Self::Center,
            Self::Center => Self::Cover,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cover => "Fill the screen", // i18n
            Self::Contain => "Whole picture", // i18n
            Self::Center => "Centred",        // i18n
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WallpaperPrefs {
    /// Gnosix draws the wallpaper.
    pub enabled: bool,
    /// The picture; `None` takes the one another shell shows, else the
    /// folder's first.
    pub picture: Option<PathBuf>,
    pub fit: WallpaperFit,
    /// Where pictures are offered from; `None` is `~/Pictures/Wallpapers`.
    pub folder: Option<PathBuf>,
}

impl Default for WallpaperPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            picture: None,
            fit: WallpaperFit::Cover,
            folder: None,
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".config")))
}

fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".cache")))
}

pub fn prefs_path() -> Option<PathBuf> {
    config_dir().map(|c| c.join("gnosix").join("wallpaper.json"))
}

pub fn load() -> WallpaperPrefs {
    prefs_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(prefs: &WallpaperPrefs) -> std::io::Result<()> {
    let path = prefs_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config directory"))?;
    let dir = path.parent().expect("a file in a directory");
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".wallpaper.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(prefs)?)?;
    std::fs::rename(&tmp, path)
}

impl WallpaperPrefs {
    pub fn folder(&self) -> Option<PathBuf> {
        self.folder
            .clone()
            .or_else(|| home().map(|h| h.join("Pictures").join("Wallpapers")))
    }

    /// The picture to show: the chosen one, else the one Caelestia shows,
    /// else the folder's first.
    pub fn picture(&self) -> Option<PathBuf> {
        self.picture
            .clone()
            .or_else(|| {
                home()
                    .map(|h| h.join(".local/state/caelestia/wallpaper/current"))
                    .and_then(|p| std::fs::canonicalize(p).ok())
            })
            .or_else(|| {
                self.folder()
                    .and_then(|f| pictures(&f).list.into_iter().next())
            })
    }
}

fn is_picture(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp"
        )
    })
}

/// The pictures a folder holds, as far as they were read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pictures {
    /// By name.
    pub list: Vec<PathBuf>,
    /// The folder holds more than were read: `list` is not the whole of it,
    /// and must never be shown as if it were.
    pub truncated: bool,
}

impl Pictures {
    /// How many, as shown: "4096+" when the folder holds more.
    pub fn count_label(&self) -> String {
        format!(
            "{}{}",
            self.list.len(),
            if self.truncated { "+" } else { "" }
        )
    }
}

/// The pictures in `folder`, read in one bounded pass (at most the folder
/// cap, a setting): Settings reads it on the UI thread, and a folder of
/// fifty thousand files must not stall the scene.
pub fn pictures(folder: &Path) -> Pictures {
    pictures_capped(folder, folder_cap())
}

fn folder_cap() -> usize {
    #[cfg(feature = "component-lab")]
    {
        crate::surface_prefs::advanced("wallpaper_folder_cap") as usize
    }
    #[cfg(not(feature = "component-lab"))]
    {
        4096
    }
}

/// At most `cap` pictures of `folder`, by name; the entry type comes with
/// the listing, so no file is opened to tell a picture from a folder.
pub fn pictures_capped(folder: &Path, cap: usize) -> Pictures {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Pictures::default();
    };
    let mut found = Pictures::default();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !entry
            .file_type()
            .is_ok_and(|t| t.is_file() || t.is_symlink())
            || !is_picture(&path)
        {
            continue;
        }
        if found.list.len() == cap {
            found.truncated = true;
            break;
        }
        found.list.push(path);
    }
    found.list.sort();
    found
}

/// The picture one step from `current` in `list` (wrapping).
pub fn step_picture(list: &[PathBuf], current: Option<&Path>, dir: i8) -> Option<PathBuf> {
    if list.is_empty() {
        return None;
    }
    let len = list.len() as isize;
    let at = current
        .and_then(|c| list.iter().position(|p| p == c))
        .map_or(if dir < 0 { 0 } else { -1 }, |i| i as isize);
    let next = (at + isize::from(dir.signum())).rem_euclid(len);
    Some(list[next as usize].clone())
}

/// The picture laid out on a `width` × `height` output.
pub fn render(
    source: &image::DynamicImage,
    fit: WallpaperFit,
    width: u32,
    height: u32,
) -> RgbaImage {
    let (sw, sh) = source.dimensions();
    let mut out = RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 255]));
    let place = |scaled: &RgbaImage, out: &mut RgbaImage| {
        let x = (i64::from(width) - i64::from(scaled.width())) / 2;
        let y = (i64::from(height) - i64::from(scaled.height())) / 2;
        image::imageops::overlay(out, scaled, x, y);
    };
    match fit {
        WallpaperFit::Cover | WallpaperFit::Contain => {
            let sx = f64::from(width) / f64::from(sw);
            let sy = f64::from(height) / f64::from(sh);
            let s = if fit == WallpaperFit::Cover {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            let w = ((f64::from(sw) * s).round() as u32).max(1);
            let h = ((f64::from(sh) * s).round() as u32).max(1);
            let scaled = source.resize_exact(w, h, FilterType::Triangle).to_rgba8();
            place(&scaled, &mut out);
        }
        WallpaperFit::Center => place(&source.to_rgba8(), &mut out),
    }
    out
}

/// The picture prepared for one output size: from the cache when it was
/// made before (same file, size and fit), else made now. Slow: never on the
/// UI thread.
pub fn prepare(
    picture: &Path,
    fit: WallpaperFit,
    width: u32,
    height: u32,
) -> Result<PathBuf, String> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err("bad output size".into());
    }
    let meta = std::fs::metadata(picture).map_err(|e| e.to_string())?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (picture, meta.len(), modified, fit as u8, width, height).hash(&mut h);
        format!("{:016x}-{width}x{height}.png", h.finish())
    };
    let dir = cache_dir()
        .ok_or("no cache directory")?
        .join("gnosix")
        .join("wallpapers");
    let path = dir.join(key);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let source = image::open(picture).map_err(|e| e.to_string())?;
    let out = render(&source, fit, width, height);
    let tmp = dir.join(format!(".{}.tmp.png", std::process::id()));
    out.save(&tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32) -> image::DynamicImage {
        // Left half red, right half blue.
        image::DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        }))
    }

    #[test]
    fn cover_fills_contain_letterboxes_center_keeps_the_size() {
        let wide = picture(400, 100);
        let cover = render(&wide, WallpaperFit::Cover, 100, 100);
        assert_eq!(cover.dimensions(), (100, 100));
        // Cropped evenly: both halves still meet in the middle, no black.
        assert_eq!(cover.get_pixel(10, 50)[0], 255);
        assert_eq!(cover.get_pixel(90, 50)[2], 255);
        assert!(cover.pixels().all(|p| p[0] > 0 || p[2] > 0));
        let contain = render(&wide, WallpaperFit::Contain, 100, 100);
        // 100 × 25 in the middle, black above and below.
        assert_eq!(*contain.get_pixel(50, 5), image::Rgba([0, 0, 0, 255]));
        assert_eq!(contain.get_pixel(10, 50)[0], 255);
        let center = render(&picture(20, 20), WallpaperFit::Center, 100, 100);
        assert_eq!(*center.get_pixel(5, 5), image::Rgba([0, 0, 0, 255]));
        assert_eq!(center.get_pixel(45, 50)[0], 255);
    }

    #[test]
    fn pictures_are_listed_and_stepped_through_wrapping() {
        let dir = std::env::temp_dir().join(format!("gnosix-wallpapers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.jpg", "a.png", "notes.txt", "c.WEBP"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let list = pictures_capped(&dir, 4096).list;
        let names: Vec<_> = list
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["a.png", "b.jpg", "c.WEBP"]);
        assert_eq!(step_picture(&list, Some(&list[2]), 1).unwrap(), list[0]);
        assert_eq!(step_picture(&list, Some(&list[0]), -1).unwrap(), list[2]);
        assert_eq!(step_picture(&list, None, 1).unwrap(), list[0]);
        assert_eq!(step_picture(&[], None, 1), None);
        // The cap is honoured and said: never a partial list shown as whole.
        let capped = pictures_capped(&dir, 2);
        assert_eq!(capped.list.len(), 2);
        assert!(capped.truncated);
        assert_eq!(capped.count_label(), "2+");
        assert_eq!(pictures_capped(&dir, 3).count_label(), "3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_prepared_picture_is_made_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("gnosix-wall-prep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("p.png");
        picture(64, 32).save(&source).unwrap();
        // SAFETY of the test: the cache goes under the temp directory.
        let cached = {
            let out = render(&image::open(&source).unwrap(), WallpaperFit::Cover, 16, 16);
            out.dimensions()
        };
        assert_eq!(cached, (16, 16));
        assert!(prepare(&source, WallpaperFit::Cover, 0, 10).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
