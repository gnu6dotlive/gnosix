//! Typed gestures and bounded providers; imported text is not executable.
use in_gnu_os_settings_core::DesktopEntryId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod desktop_entry_id {
    use super::*;
    pub fn serialize<S: Serializer>(
        value: &DesktopEntryId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(value.as_str())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<DesktopEntryId, D::Error> {
        let raw = String::deserialize(deserializer)?;
        DesktopEntryId::parse(raw)
            .map_err(|_| serde::de::Error::custom("invalid desktop entry identity"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct KeyId(String);
impl KeyId {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.trim() != value
            || value.chars().any(char::is_whitespace)
            || value.contains('+')
        {
            return Err(value);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for KeyId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("invalid key identity"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Super,
    SuperLeft,
    SuperRight,
    Shift,
    Control,
    Alt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyTrigger {
    Press,
    Release,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerTrigger {
    Press,
    Release,
    Drag,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Gesture {
    ModifierTap {
        modifier: Modifier,
    },
    KeyChord {
        modifiers: Vec<Modifier>,
        key: KeyId,
        trigger: KeyTrigger,
    },
    PointerChord {
        modifiers: Vec<Modifier>,
        button: u32,
        trigger: PointerTrigger,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAction {
    ToggleApplicationList,
    OpenDefaultWebBrowser,
    CaptureScreenshot,
    MoveWindow,
    ResizeWindow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProviderOperation {
    Launch,
    Show,
    Hide,
    Toggle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityEvidence {
    Verified,
    Unverified,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinProvider {
    GnosixAppStack,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositorCapability {
    MoveWindow,
    ResizeWindow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GnosixCapability {
    ToggleApplicationList,
    CaptureScreenshot,
    OpenDefaultWebBrowser,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultApplicationRole {
    WebBrowser,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderRef {
    Builtin {
        provider: BuiltinProvider,
    },
    DesktopEntry {
        #[serde(with = "desktop_entry_id")]
        id: DesktopEntryId,
        operation: DesktopProviderOperation,
    },
    DefaultApplicationRole {
        role: DefaultApplicationRole,
    },
    Compositor {
        capability: CompositorCapability,
    },
    Gnosix {
        capability: GnosixCapability,
    },
    /// A registered compositor row whose execution target remains opaque.
    /// This is evidence about current state, never an executable provider.
    UnresolvedObserved,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingContext {
    Global,
    CompositorSubmap { name: String },
    FocusGroup { group: FocusGroup },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusGroup {
    Dock,
    AppStack,
    Desktop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub gesture: Gesture,
    pub semantic_action: SemanticAction,
    pub context: BindingContext,
    pub provider: ProviderRef,
    pub enabled: bool,
}
impl Binding {
    pub fn validate(&self) -> Result<(), GestureValidationError> {
        match &self.gesture {
            Gesture::ModifierTap { .. } => Ok(()),
            Gesture::KeyChord { modifiers, .. } => validate_modifiers(modifiers),
            Gesture::PointerChord {
                modifiers, button, ..
            } => {
                if *button == 0 {
                    return Err(GestureValidationError::InvalidPointerButton);
                }
                validate_modifiers(modifiers)
            }
        }?;
        if let BindingContext::CompositorSubmap { name } = &self.context
            && name.trim().is_empty()
        {
            return Err(GestureValidationError::EmptyContext);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureValidationError {
    DuplicateModifier,
    InvalidPointerButton,
    EmptyContext,
}
fn validate_modifiers(modifiers: &[Modifier]) -> Result<(), GestureValidationError> {
    let mut seen = std::collections::BTreeSet::new();
    if modifiers.iter().any(|modifier| !seen.insert(*modifier)) {
        Err(GestureValidationError::DuplicateModifier)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingResolution {
    Ready,
    Conflict { reasons: Vec<String> },
    Shadowed { by_submap: String },
    InvalidContext { reason: String },
    ProviderUnavailable { provider: String },
    AssociationConflict { role: String },
    UnsupportedGesture { gesture: String },
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAssociation {
    DefaultWebBrowser,
    Http,
    Https,
    TextHtml,
}
impl BrowserAssociation {
    pub const REQUIRED: [Self; 4] = [
        Self::DefaultWebBrowser,
        Self::Http,
        Self::Https,
        Self::TextHtml,
    ];
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssociationObservation {
    Observed {
        #[serde(with = "desktop_entry_id")]
        id: DesktopEntryId,
    },
    Unavailable {
        detail: String,
    },
    Invalid {
        raw: String,
        detail: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserAssociationEvidence {
    pub association: BrowserAssociation,
    pub observation: AssociationObservation,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_json_cannot_smuggle_arbitrary_execution_or_bus_fields() {
        let raw = r#"{"gesture":{"kind":"modifier_tap","modifier":"super"},"semantic_action":"toggle_application_list","context":{"kind":"global"},"provider":{"kind":"desktop_entry","id":"menu.desktop","operation":"toggle","argv":["sh","-c"]},"enabled":true}"#;
        assert!(serde_json::from_str::<Binding>(raw).is_err());
        let raw = r#"{"gesture":{"kind":"modifier_tap","modifier":"super"},"semantic_action":"toggle_application_list","context":{"kind":"global"},"provider":{"kind":"desktop_entry","id":"../bad.desktop","operation":"toggle"},"enabled":true}"#;
        assert!(serde_json::from_str::<Binding>(raw).is_err());
    }
    #[test]
    fn external_launch_capability_does_not_imply_list_toggle() {
        let launch = ProviderRef::DesktopEntry {
            id: DesktopEntryId::parse("menu.desktop").unwrap(),
            operation: DesktopProviderOperation::Launch,
        };
        assert!(!crate::resolution::supports(
            &launch,
            SemanticAction::ToggleApplicationList
        ));
        assert!(!crate::resolution::external_operation_is_advertisable(
            DesktopProviderOperation::Toggle,
            CapabilityEvidence::Unverified
        ));
        assert!(crate::resolution::external_operation_is_advertisable(
            DesktopProviderOperation::Toggle,
            CapabilityEvidence::Verified
        ));
    }
    #[test]
    fn key_identity_deserialization_runs_validation() {
        assert!(serde_json::from_str::<KeyId>("\"W\"").is_ok());
        assert!(serde_json::from_str::<KeyId>("\"W\\n\"").is_err());
    }
    #[test]
    fn binding_validation_rejects_duplicate_modifiers_invalid_pointer_and_empty_submap() {
        let base = |gesture, context| Binding {
            gesture,
            semantic_action: SemanticAction::MoveWindow,
            context,
            provider: ProviderRef::Compositor {
                capability: CompositorCapability::MoveWindow,
            },
            enabled: true,
        };
        assert_eq!(
            base(
                Gesture::KeyChord {
                    modifiers: vec![Modifier::Super, Modifier::Super],
                    key: KeyId::parse("W").unwrap(),
                    trigger: KeyTrigger::Press,
                },
                BindingContext::Global,
            )
            .validate(),
            Err(GestureValidationError::DuplicateModifier)
        );
        assert_eq!(
            base(
                Gesture::PointerChord {
                    modifiers: vec![Modifier::Super],
                    button: 0,
                    trigger: PointerTrigger::Drag,
                },
                BindingContext::Global,
            )
            .validate(),
            Err(GestureValidationError::InvalidPointerButton)
        );
        assert_eq!(
            base(
                Gesture::ModifierTap {
                    modifier: Modifier::Super,
                },
                BindingContext::CompositorSubmap { name: "  ".into() },
            )
            .validate(),
            Err(GestureValidationError::EmptyContext)
        );
    }
}
