//! Filesystem browsing projection for Cabinet. No mutation or authority escalation.
//! Enumerations run off the GPUI UI thread and are bounded.
use crate::cabinet_filters::CabinetFilter;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const MAX_DIRECTORY_ENTRIES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CabinetEntry {
    pub path: PathBuf,
    pub label: String,
    pub folder: bool,
}

#[derive(Debug)]
pub struct DirectoryListing {
    pub entries: Vec<CabinetEntry>,
    pub truncated: bool,
}

/// Read at most MAX_DIRECTORY_ENTRIES+1 qualifying entries. The truncation
/// marker is explicit; a caller must never present the partial list as complete.
pub fn scan_directory(folder: &Path) -> io::Result<DirectoryListing> {
    scan_directory_filtered(folder, None)
}

pub fn scan_directory_filtered(
    folder: &Path,
    filter: Option<&CabinetFilter>,
) -> io::Result<DirectoryListing> {
    let mut entries = Vec::new();
    let mut truncated = false;
    for candidate in fs::read_dir(folder)? {
        let Ok(candidate) = candidate else {
            continue;
        };
        let path = candidate.path();
        let Ok(metadata) = candidate.metadata() else {
            continue;
        };
        if !metadata.is_dir() && !metadata.is_file() {
            continue;
        }
        if metadata.is_file() && filter.is_some_and(|rule| !rule.matches_path(&path)) {
            continue;
        }
        if entries.len() == MAX_DIRECTORY_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(CabinetEntry {
            label: candidate.file_name().to_string_lossy().into_owned(),
            path,
            folder: metadata.is_dir(),
        });
    }
    entries.sort_by(|a, b| {
        b.folder
            .cmp(&a.folder)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(DirectoryListing { entries, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "gnosix-cabinet-scan-{}-{stamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn lists_directories_before_files_and_preserves_paths() {
        let f = Fixture::new();
        fs::write(f.path.join("z.txt"), b"z").unwrap();
        fs::write(f.path.join("été.txt"), b"unicode").unwrap();
        fs::create_dir(f.path.join("B Dir")).unwrap();
        fs::create_dir(f.path.join("a dir")).unwrap();
        let scan = scan_directory(&f.path).unwrap();
        assert!(!scan.truncated);
        let names: Vec<_> = scan.entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(names, ["a dir", "B Dir", "z.txt", "été.txt"]);
        assert!(scan.entries[0].folder);
        assert!(scan.entries[1].folder);
        assert_eq!(scan.entries[3].path, f.path.join("été.txt"));
    }

    #[test]
    fn missing_folder_fails_without_fabricating_entries() {
        let f = Fixture::new();
        assert!(scan_directory(&f.path.join("not-here")).is_err());
    }

    #[test]
    fn matching_filter_keeps_all_directories_but_only_matching_files() {
        let f = Fixture::new();
        fs::create_dir(f.path.join("assets")).unwrap();
        fs::write(f.path.join("demo.PNG"), b"png").unwrap();
        fs::write(f.path.join("secret.txt"), b"txt").unwrap();
        let filter = CabinetFilter {
            name: "PNG".into(),
            rules: vec![(0, "*.[pP][nN][gG]".into())],
        };
        let found = scan_directory_filtered(&f.path, Some(&filter)).unwrap();
        let names: Vec<_> = found
            .entries
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(names, ["assets", "demo.PNG"]);
    }

    #[test]
    fn directory_limit_is_explicit() {
        let f = Fixture::new();
        for index in 0..(MAX_DIRECTORY_ENTRIES + 1) {
            fs::write(f.path.join(format!("{index:04}")), b"").unwrap();
        }
        let scan = scan_directory(&f.path).unwrap();
        assert_eq!(scan.entries.len(), MAX_DIRECTORY_ENTRIES);
        assert!(scan.truncated);
    }
}
