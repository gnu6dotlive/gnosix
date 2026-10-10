//! GFC-02 private stdio contract. Only the backend may launch its renderer.
//! This is NOT a public IPC API or a portal permission grant.
use crate::cabinet_choices::{parse_choices, validate_result, CabinetChoice};
use crate::cabinet_filters::{parse_filters, CabinetFilter};
use crate::{PickerInvocation, PickerKind, Resolution};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use zbus::zvariant::OwnedValue;

pub const MAX_WIRE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub title: String,
    pub app_id: String,
    pub initial_folder: PathBuf,
    pub multiple: bool,
    pub directory: bool,
    pub kind: PickerKind,
    pub filename: String,
    pub requested_files: Vec<PathBuf>,
    pub accept_label: String,
    pub filters: Vec<CabinetFilter>,
    pub selected_filter: Option<usize>,
    pub choices: Vec<CabinetChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", content = "paths", rename_all = "snake_case")]
pub enum RenderResponse {
    Selected(Vec<PathBuf>),
    SelectedWithFilter {
        paths: Vec<PathBuf>,
        filter_index: usize,
    },
    SelectedWithMetadata {
        paths: Vec<PathBuf>,
        filter_index: Option<usize>,
        choices: Vec<(String, String)>,
    },
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    Unsupported,
    InvalidOptions,
    InvalidFolder,
}

impl RenderRequest {
    /// Fail closed on any semantics the GFC-02A renderer cannot honor.
    /// Modal parenting, Save*, filters, choices and writable semantics are
    /// explicitly unimplemented, not silently dropped.
    pub fn from_invocation(request: &PickerInvocation) -> Result<Self, RenderError> {
        if !request.parent_window.is_empty() {
            return Err(RenderError::Unsupported);
        }
        // Only implement semantics that Cabinet can faithfully present.
        let permitted: &[&str] = match request.kind {
            PickerKind::OpenFile => &[
                "multiple",
                "directory",
                "current_folder",
                "modal",
                "accept_label",
                "filters",
                "current_filter",
                "choices",
            ],
            PickerKind::SaveFile => &[
                "current_name",
                "current_folder",
                "modal",
                "accept_label",
                "filters",
                "current_filter",
            ],
            PickerKind::SaveFiles => &["files", "current_folder", "modal", "accept_label"],
        };
        for option in request.options.keys() {
            if !permitted.contains(&option.as_str()) {
                return Err(RenderError::Unsupported);
            }
        }
        let read_bool = |key: &str| -> Result<bool, RenderError> {
            match request.options.get(key) {
                Some(value) => bool::try_from(value).map_err(|_| RenderError::InvalidOptions),
                None => Ok(false),
            }
        };
        // Some callers need modal UI. Without parent export, we cannot claim it.
        if request.options.get("modal").is_some_and(|value| {
            bool::try_from(value).is_err() || bool::try_from(value) == Ok(true)
        }) {
            return Err(RenderError::Unsupported);
        }
        let initial_folder = match request.options.get("current_folder") {
            Some(value) => path_from_bytes(value)?,
            None => std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or(RenderError::InvalidFolder)?,
        };
        if !initial_folder.is_absolute() || !initial_folder.is_dir() {
            return Err(RenderError::InvalidFolder);
        }
        if request.kind == PickerKind::OpenFile && read_bool("directory")? && read_bool("multiple")?
        {
            // The current UI confirms exactly one folder.
            return Err(RenderError::Unsupported);
        }
        let filename = match request.options.get("current_name") {
            Some(value) => {
                let value =
                    String::try_from(value.try_clone().map_err(|_| RenderError::InvalidOptions)?)
                        .map_err(|_| RenderError::InvalidOptions)?;
                if !value.is_empty() && !safe_name(Path::new(&value)) {
                    return Err(RenderError::InvalidOptions);
                }
                value
            }
            None => String::new(),
        };
        let requested_files = match request.options.get("files") {
            Some(value) => {
                let values: Vec<Vec<u8>> =
                    Vec::try_from(value.try_clone().map_err(|_| RenderError::InvalidOptions)?)
                        .map_err(|_| RenderError::InvalidOptions)?;
                if values.is_empty() || values.len() > 128 {
                    return Err(RenderError::InvalidOptions);
                }
                let mut seen = HashSet::new();
                let mut files = Vec::new();
                for value in &values {
                    let name = name_from_bytes(value)?;
                    if !seen.insert(name.clone()) {
                        return Err(RenderError::InvalidOptions);
                    }
                    files.push(name);
                }
                files
            }
            None if request.kind == PickerKind::SaveFiles => {
                return Err(RenderError::InvalidOptions)
            }
            None => Vec::new(),
        };
        let accept_label = match request.options.get("accept_label") {
            Some(value) => {
                String::try_from(value.try_clone().map_err(|_| RenderError::InvalidOptions)?)
                    .map_err(|_| RenderError::InvalidOptions)?
            }
            None => String::new(),
        };
        if accept_label.chars().any(|ch| ch.is_control()) || accept_label.len() > 80 {
            return Err(RenderError::InvalidOptions);
        }
        let (filters, selected_filter) = parse_filters(
            request.options.get("filters"),
            request.options.get("current_filter"),
        )
        .map_err(|_| RenderError::InvalidOptions)?;
        let choices = parse_choices(request.options.get("choices"))
            .map_err(|_| RenderError::InvalidOptions)?;
        Ok(Self {
            filters,
            selected_filter,
            choices,
            kind: request.kind,
            filename,
            requested_files,
            accept_label,
            title: request.title.clone(),
            app_id: request.app_id.clone(),
            initial_folder,
            multiple: read_bool("multiple")?,
            directory: request.kind == PickerKind::SaveFiles || read_bool("directory")?,
        })
    }

    pub fn validate_selection(&self, paths: &[PathBuf]) -> bool {
        if paths.is_empty() {
            return false;
        }
        match self.kind {
            PickerKind::OpenFile => {
                (self.multiple || paths.len() == 1)
                    && paths.iter().all(|p| {
                        p.is_absolute()
                            && if self.directory {
                                p.is_dir()
                            } else {
                                p.is_file()
                            }
                    })
            }
            PickerKind::SaveFile => {
                paths.len() == 1
                    && paths.iter().all(|p| {
                        p.is_absolute()
                            && p.file_name().is_some()
                            && p.parent().is_some_and(Path::is_dir)
                            && !path_exists_or_symlink(p)
                    })
            }
            PickerKind::SaveFiles => {
                paths.len() == 1
                    && paths[0].is_absolute()
                    && paths[0].is_dir()
                    && !self.requested_files.is_empty()
                    && self.requested_files.iter().all(|name| {
                        safe_name(name) && !path_exists_or_symlink(&paths[0].join(name))
                    })
            }
        }
    }
}

fn path_exists_or_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn safe_name(name: &Path) -> bool {
    let mut c = name.components();
    matches!(c.next(), Some(Component::Normal(_))) && c.next().is_none()
}

fn name_from_bytes(bytes: &[u8]) -> Result<PathBuf, RenderError> {
    if bytes.last() != Some(&0) || bytes.len() < 2 {
        return Err(RenderError::InvalidOptions);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(
            bytes[..bytes.len() - 1].to_vec(),
        ));
        if !safe_name(&path) {
            return Err(RenderError::InvalidOptions);
        }
        Ok(path)
    }
    #[cfg(not(unix))]
    {
        let _ = bytes;
        Err(RenderError::Unsupported)
    }
}

fn path_from_bytes(value: &OwnedValue) -> Result<PathBuf, RenderError> {
    let bytes: Vec<u8> = Vec::try_from(value.try_clone().map_err(|_| RenderError::InvalidFolder)?)
        .map_err(|_| RenderError::InvalidFolder)?;
    if bytes.last() != Some(&0) || bytes.is_empty() {
        return Err(RenderError::InvalidFolder);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(bytes[..bytes.len() - 1].to_vec()).into())
    }
    #[cfg(not(unix))]
    {
        let _ = bytes;
        Err(RenderError::Unsupported)
    }
}

impl RenderResponse {
    pub fn into_resolution(self, request: &RenderRequest) -> Resolution {
        match self {
            Self::Cancelled => Resolution::Cancelled,
            // A filtered request requires an explicit choice from the renderer,
            // rather than silently inventing a default filter in the backend.
            Self::Selected(paths)
                if request.choices.is_empty()
                    && request.filters.is_empty()
                    && request.validate_selection(&paths) =>
            {
                match request.kind {
                    PickerKind::SaveFiles => Resolution::Selected(
                        request
                            .requested_files
                            .iter()
                            .map(|name| paths[0].join(name))
                            .collect(),
                    ),
                    _ => Resolution::Selected(paths),
                }
            }
            Self::SelectedWithFilter {
                paths,
                filter_index,
            } if request.choices.is_empty()
                && request.filters.get(filter_index).is_some()
                && request.validate_selection(&paths)
                && request.kind != PickerKind::SaveFiles =>
            {
                Resolution::SelectedWithFilter {
                    paths,
                    filter: request.filters[filter_index].clone(),
                }
            }
            Self::SelectedWithMetadata {
                paths,
                filter_index,
                choices,
            } if request.validate_selection(&paths)
                && validate_result(&request.choices, &choices)
                && match filter_index {
                    Some(index) => {
                        request.kind != PickerKind::SaveFiles
                            && request.filters.get(index).is_some()
                    }
                    None => request.filters.is_empty(),
                } =>
            {
                let filter = filter_index.map(|index| request.filters[index].clone());
                let paths = if request.kind == PickerKind::SaveFiles {
                    request
                        .requested_files
                        .iter()
                        .map(|name| paths[0].join(name))
                        .collect()
                } else {
                    paths
                };
                Resolution::SelectedWithMetadata {
                    paths,
                    filter,
                    choices,
                }
            }
            _ => Resolution::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn invocation() -> PickerInvocation {
        PickerInvocation {
            handle: "/r/1".into(),
            app_id: "test.app".into(),
            parent_window: "".into(),
            title: "Open".into(),
            kind: PickerKind::OpenFile,
            options: HashMap::new(),
        }
    }

    #[test]
    fn strict_unsupported_semantics() {
        let mut a = invocation();
        a.kind = PickerKind::SaveFile;
        let save = RenderRequest::from_invocation(&a).expect("SaveFile can solicit a filename");
        assert_eq!(save.kind, PickerKind::SaveFile);
        a = invocation();
        a.parent_window = "wayland:abc".into();
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::Unsupported)
        );
        a = invocation();
        a.options.insert("filters".into(), OwnedValue::from(1u32));
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::InvalidOptions)
        );
        a = invocation();
        a.options.insert("modal".into(), OwnedValue::from(true));
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::Unsupported)
        );
    }

    #[test]
    fn save_file_name_guards_and_collision_protection() {
        let mut a = invocation();
        a.kind = PickerKind::SaveFile;
        a.options.insert(
            "current_name".into(),
            OwnedValue::try_from(zbus::zvariant::Value::new("good.txt")).unwrap(),
        );
        let req = RenderRequest::from_invocation(&a).unwrap();
        let temp = std::env::temp_dir();
        let unique = format!("cabinet-test-{}-{}.txt", std::process::id(), 8942);
        let candidate = temp.join(unique);
        assert!(req.validate_selection(std::slice::from_ref(&candidate)));
        assert_eq!(
            RenderResponse::Selected(vec![candidate.clone()]).into_resolution(&req),
            Resolution::Selected(vec![candidate.clone()])
        );
        a.options.insert(
            "current_name".into(),
            OwnedValue::try_from(zbus::zvariant::Value::new("../escape.txt")).unwrap(),
        );
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::InvalidOptions)
        );
        a.options.insert(
            "current_name".into(),
            OwnedValue::try_from(zbus::zvariant::Value::new("nested/evil.txt")).unwrap(),
        );
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::InvalidOptions)
        );
        let dir = std::env::temp_dir();
        assert!(!req.validate_selection(&[dir]));
    }

    #[test]
    fn save_files_names_are_ordered_and_traversal_is_rejected() {
        let mut a = invocation();
        a.kind = PickerKind::SaveFiles;
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::InvalidOptions)
        );
        let names = vec![b"first.txt ".to_vec(), b"second.png ".to_vec()];
        let value = zbus::zvariant::Value::new(names);
        a.options.insert(
            "files".into(),
            zbus::zvariant::OwnedValue::try_from(value).unwrap(),
        );
        let req = RenderRequest::from_invocation(&a).unwrap();
        let base = std::env::temp_dir();
        let response = RenderResponse::Selected(vec![base.clone()]).into_resolution(&req);
        assert_eq!(
            response,
            Resolution::Selected(vec![base.join("first.txt"), base.join("second.png")])
        );
        let values = vec![b"../escape.txt ".to_vec()];
        a.options.insert(
            "files".into(),
            zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::new(values)).unwrap(),
        );
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::InvalidOptions)
        );
    }

    #[test]
    fn selection_must_provide_actual_filter_index_for_filtered_requests() {
        use zbus::zvariant::{OwnedValue, Value};
        let mut req = invocation();
        let filters = vec![
            ("Text".to_string(), vec![(0u32, "*.txt".to_string())]),
            ("Images".to_string(), vec![(1u32, "image/*".to_string())]),
        ];
        req.options.insert(
            "filters".into(),
            OwnedValue::try_from(Value::new(filters)).unwrap(),
        );
        let wire = RenderRequest::from_invocation(&req).unwrap();
        assert_eq!(wire.selected_filter, Some(0));
        let encoded = serde_json::to_vec(&RenderResponse::SelectedWithFilter {
            paths: vec!["/tmp/file.png".into()],
            filter_index: 1,
        })
        .unwrap();
        let decoded: RenderResponse = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            decoded,
            RenderResponse::SelectedWithFilter {
                paths: vec!["/tmp/file.png".into()],
                filter_index: 1,
            }
        );
        let fixture = std::env::temp_dir();
        // Files must exist to be selectable. Readable fixture path is not trusted.
        let png = fixture.join(format!("gfc03b-{}.png", std::process::id()));
        std::fs::write(&png, b"not an image").unwrap();
        assert_eq!(
            RenderResponse::Selected(vec![png.clone()]).into_resolution(&wire),
            Resolution::Unavailable
        );
        assert_eq!(
            RenderResponse::SelectedWithFilter {
                paths: vec![png.clone()],
                filter_index: 1
            }
            .into_resolution(&wire),
            Resolution::SelectedWithFilter {
                paths: vec![png.clone()],
                filter: wire.filters[1].clone()
            }
        );
        assert_eq!(
            RenderResponse::SelectedWithFilter {
                paths: vec![png.clone()],
                filter_index: 17
            }
            .into_resolution(&wire),
            Resolution::Unavailable
        );
        std::fs::remove_file(png).unwrap();
    }

    #[test]
    fn choices_enforced_even_for_valid_local_path() {
        use zbus::zvariant::{OwnedValue, Value};
        let mut inv = invocation();
        let items = vec![
            (
                "format".to_string(),
                "Format".to_string(),
                vec![
                    ("png".to_string(), "PNG".to_string()),
                    ("svg".to_string(), "SVG".to_string()),
                ],
                "svg".to_string(),
            ),
            (
                "compress".to_string(),
                "Compress".to_string(),
                Vec::<(String, String)>::new(),
                "false".to_string(),
            ),
        ];
        inv.options.insert(
            "choices".into(),
            OwnedValue::try_from(Value::new(items)).unwrap(),
        );
        let req = RenderRequest::from_invocation(&inv).unwrap();
        let file = std::env::temp_dir().join(format!("cabinet-choice-{}.txt", std::process::id()));
        std::fs::write(&file, b"test").unwrap();
        assert_eq!(
            RenderResponse::Selected(vec![file.clone()]).into_resolution(&req),
            Resolution::Unavailable
        );
        let choices = vec![
            ("format".to_string(), "png".to_string()),
            ("compress".to_string(), "true".to_string()),
        ];
        assert_eq!(
            RenderResponse::SelectedWithMetadata {
                paths: vec![file.clone()],
                filter_index: None,
                choices: choices.clone(),
            }
            .into_resolution(&req),
            Resolution::SelectedWithMetadata {
                paths: vec![file.clone()],
                filter: None,
                choices: choices.clone()
            }
        );
        assert_eq!(
            RenderResponse::SelectedWithMetadata {
                paths: vec![file.clone()],
                filter_index: None,
                choices: vec![
                    ("format".into(), "invalid".into()),
                    ("compress".into(), "true".into())
                ],
            }
            .into_resolution(&req),
            Resolution::Unavailable
        );
        assert_eq!(
            RenderResponse::SelectedWithMetadata {
                paths: vec![file.clone()],
                filter_index: None,
                choices: vec![
                    ("wrong".into(), "png".into()),
                    ("compress".into(), "true".into())
                ],
            }
            .into_resolution(&req),
            Resolution::Unavailable
        );
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn wire_roundtrip_and_selection_cardinality() {
        let a = invocation();
        let req = RenderRequest::from_invocation(&a).expect("home exists");
        let wire = serde_json::to_vec(&req).unwrap();
        assert_eq!(serde_json::from_slice::<RenderRequest>(&wire).unwrap(), req);
        assert!(!req.validate_selection(&[]));
        assert!(!req.validate_selection(&["relative.txt".into()]));
        assert_eq!(
            RenderResponse::Selected(vec!["relative.txt".into()]).into_resolution(&req),
            Resolution::Unavailable
        );
    }
}
