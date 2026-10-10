//! GFC-02 private stdio contract. Only the backend may launch its renderer.
//! This is NOT a public IPC API or a portal permission grant.
use crate::{PickerInvocation, PickerKind, Resolution};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", content = "paths", rename_all = "snake_case")]
pub enum RenderResponse {
    Selected(Vec<PathBuf>),
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
        if request.kind != PickerKind::OpenFile || !request.parent_window.is_empty() {
            return Err(RenderError::Unsupported);
        }
        // Unknown future portal options are not silently ignored.
        for option in request.options.keys() {
            if !matches!(
                option.as_str(),
                "multiple" | "directory" | "current_folder" | "modal"
            ) {
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
        if read_bool("directory")? && read_bool("multiple")? {
            // The current UI confirms exactly one folder.
            return Err(RenderError::Unsupported);
        }
        Ok(Self {
            title: request.title.clone(),
            app_id: request.app_id.clone(),
            initial_folder,
            multiple: read_bool("multiple")?,
            directory: read_bool("directory")?,
        })
    }

    pub fn validate_selection(&self, paths: &[PathBuf]) -> bool {
        if paths.is_empty() || (!self.multiple && paths.len() != 1) {
            return false;
        }
        paths.iter().all(|path| {
            path.is_absolute()
                && if self.directory {
                    path.is_dir()
                } else {
                    path.is_file()
                }
        })
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
            Self::Selected(paths) if request.validate_selection(&paths) => {
                Resolution::Selected(paths)
            }
            Self::Selected(_) => Resolution::Unavailable,
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
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::Unsupported)
        );
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
            Err(RenderError::Unsupported)
        );
        a = invocation();
        a.options.insert("modal".into(), OwnedValue::from(true));
        assert_eq!(
            RenderRequest::from_invocation(&a),
            Err(RenderError::Unsupported)
        );
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
