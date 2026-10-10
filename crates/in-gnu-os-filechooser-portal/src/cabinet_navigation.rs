//! Navigable, testable file-chooser location history. No filesystem mutation.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct CabinetNavigation {
    current: PathBuf,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}
const MAX_HISTORY: usize = 64;

impl CabinetNavigation {
    pub fn new(folder: PathBuf) -> Self {
        Self {
            current: folder,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }
    pub fn current(&self) -> &Path {
        &self.current
    }
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn navigate(&mut self, folder: PathBuf) -> bool {
        if folder == self.current || !folder.is_absolute() || !folder.is_dir() {
            return false;
        }
        Self::push(&mut self.back, std::mem::replace(&mut self.current, folder));
        self.forward.clear();
        true
    }
    pub fn back(&mut self) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        Self::push(
            &mut self.forward,
            std::mem::replace(&mut self.current, previous),
        );
        true
    }
    pub fn forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        Self::push(&mut self.back, std::mem::replace(&mut self.current, next));
        true
    }
    pub fn up(&mut self) -> bool {
        let Some(parent) = self.current.parent() else {
            return false;
        };
        self.navigate(parent.to_path_buf())
    }
    /// Location crumbs are real ancestors, not path splitting that breaks non-UTF8 names.
    pub fn crumbs(&self) -> Vec<(String, PathBuf)> {
        let mut ancestors: Vec<PathBuf> = self.current.ancestors().map(Path::to_path_buf).collect();
        ancestors.reverse();
        ancestors
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".into());
                (name, path)
            })
            .collect()
    }
    fn push(stack: &mut Vec<PathBuf>, folder: PathBuf) {
        if stack.len() >= MAX_HISTORY {
            stack.remove(0);
        }
        stack.push(folder);
    }
}

pub fn quick_locations(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut result = Vec::new();
    for (label, candidate) in [
        ("Home", home.to_path_buf()),
        ("Documents", home.join("Documents")),
        ("Downloads", home.join("Downloads")),
        ("Pictures", home.join("Pictures")),
        ("Projects", home.join("Projects")),
    ] {
        if candidate.is_dir() {
            result.push((label, candidate));
        }
    }
    result
}

pub fn query_matches(name: &str, needle: &str, show_hidden: bool) -> bool {
    let query = needle.trim();
    if !show_hidden && name.starts_with('.') {
        return false;
    }
    query.is_empty() || name.to_lowercase().contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    #[test]
    fn reversible_navigation_and_forward_branch_invalidation() {
        let root = std::env::temp_dir().join(format!(
            "cabinet-navigation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join("c")).unwrap();
        let mut nav = CabinetNavigation::new(root.clone());
        assert!(!nav.can_back());
        assert!(!nav.back());
        assert!(nav.navigate(root.join("a")));
        assert!(nav.navigate(root.join("a/b")));
        assert!(nav.back());
        assert_eq!(nav.current(), root.join("a"));
        assert!(nav.can_forward());
        assert!(nav.navigate(root.join("c")));
        assert!(!nav.can_forward());
        assert!(nav.up());
        assert_eq!(nav.current(), root);
        assert!(!nav.navigate(root.join("missing")));
        assert!(!nav.navigate(root.clone()));
        assert_eq!(nav.crumbs().last().unwrap().1, root);
        fs::remove_dir_all(nav.current()).unwrap();
    }
    #[test]
    fn casefold_local_search_hidden_toggle() {
        assert!(query_matches("Été Report.PNG", "report", false));
        assert!(query_matches("photos", "", false));
        assert!(!query_matches(".git", "", false));
        assert!(query_matches(".git", "git", true));
        assert!(!query_matches("report.txt", "image", true));
    }
    #[test]
    fn quick_locations_never_invent_missing_directories() {
        let path = std::env::temp_dir().join(format!("cabinet-locations-{}", std::process::id()));
        fs::create_dir_all(path.join("Projects")).unwrap();
        let entries = quick_locations(&path);
        assert_eq!(
            entries.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec!["Home", "Projects"]
        );
        fs::remove_dir_all(path).unwrap();
    }
}
