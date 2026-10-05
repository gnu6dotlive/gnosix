//! R12-only service. Transport authentication precedes every store access.
use in_gnu_os_caller_auth::DbusCallerAuthenticator;
use in_gnu_os_desktop_profiles::{bar_writer::BarProfileWriter, *};
use std::sync::Mutex;
use zbus::{
    blocking::{Connection, connection::Builder},
    fdo, interface,
    message::Header,
};

pub use in_gnu_os_desktop_profiles::{
    BAR_PROFILE_BUS_NAME as BUS_NAME, BAR_PROFILE_OBJECT_PATH as OBJECT_PATH,
};

fn exact<const N: usize>(v: Vec<u8>) -> fdo::Result<[u8; N]> {
    v.try_into()
        .map_err(|_| fdo::Error::InvalidArgs(format!("expected {N} bytes")))
}
fn decode_request(
    request_id: Vec<u8>,
    epoch: Vec<u8>,
    profile_id: Vec<u8>,
    revision: u64,
    digest: Vec<u8>,
    edge: &str,
) -> fdo::Result<SetBarEdge> {
    let edge = match edge {
        "top" => BarEdgePreference::Top,
        "right" => BarEdgePreference::Right,
        "bottom" => BarEdgePreference::Bottom,
        "left" => BarEdgePreference::Left,
        _ => return Err(fdo::Error::InvalidArgs("unsupported Bar edge".into())),
    };
    Ok(SetBarEdge {
        request_id: exact(request_id)?,
        epoch: exact(epoch)?,
        profile_id: DesktopProfileId::from_bytes(exact(profile_id)?),
        expected: ProfileVersion::Current {
            revision,
            etag: exact(digest)?,
        },
        edge,
    })
}

fn decode_preferences_request(
    request_id: Vec<u8>,
    epoch: Vec<u8>,
    profile_id: Vec<u8>,
    revision: u64,
    digest: Vec<u8>,
    preferences_json: &str,
) -> fdo::Result<SetBarPreferenceOverrides> {
    let preferences: BarPreferenceOverrides = serde_json::from_str(preferences_json)
        .map_err(|_| fdo::Error::InvalidArgs("invalid bounded Bar preferences".into()))?;
    Ok(SetBarPreferenceOverrides {
        request_id: exact(request_id)?,
        epoch: exact(epoch)?,
        profile_id: DesktopProfileId::from_bytes(exact(profile_id)?),
        expected: ProfileVersion::Current {
            revision,
            etag: exact(digest)?,
        },
        preferences,
    })
}
struct ProfileBoundary {
    auth: DbusCallerAuthenticator,
    writer: Mutex<BarProfileWriter>,
}
impl ProfileBoundary {
    fn authenticate(&self, header: &Header<'_>) -> fdo::Result<()> {
        self.auth
            .authenticate(header)
            .map(|_| ())
            .map_err(|error| fdo::Error::AccessDenied(format!("caller refused: {error:?}")))
    }
}
#[interface(name = "in.gnu.os.DesktopProfiles1")]
impl ProfileBoundary {
    fn read_bar_edge(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<String> {
        self.authenticate(&header)?;
        let snapshot = self
            .writer
            .lock()
            .map_err(|_| fdo::Error::Failed("authority unavailable".into()))?
            .read()
            .map_err(|_| fdo::Error::Failed("profile unavailable or recovery required".into()))?;
        serde_json::to_string(&snapshot).map_err(|_| fdo::Error::Failed("encoding failed".into()))
    }
    #[allow(clippy::too_many_arguments)]
    fn set_bar_edge(
        &self,
        request_id: Vec<u8>,
        epoch: Vec<u8>,
        profile_id: Vec<u8>,
        revision: u64,
        digest: Vec<u8>,
        edge: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        self.authenticate(&header)?;
        let request = decode_request(request_id, epoch, profile_id, revision, digest, edge)?;
        let receipt = self
            .writer
            .lock()
            .map_err(|_| fdo::Error::Failed("authority unavailable".into()))?
            .apply(request);
        serde_json::to_string(&receipt).map_err(|_| fdo::Error::Failed("encoding failed".into()))
    }
    #[allow(clippy::too_many_arguments)]
    fn set_bar_preferences(
        &self,
        request_id: Vec<u8>,
        epoch: Vec<u8>,
        profile_id: Vec<u8>,
        revision: u64,
        digest: Vec<u8>,
        preferences_json: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        self.authenticate(&header)?;
        let request = decode_preferences_request(
            request_id,
            epoch,
            profile_id,
            revision,
            digest,
            preferences_json,
        )?;
        let receipt = self
            .writer
            .lock()
            .map_err(|_| fdo::Error::Failed("authority unavailable".into()))?
            .apply_overrides(request);
        serde_json::to_string(&receipt).map_err(|_| fdo::Error::Failed("encoding failed".into()))
    }
}
/// Start only in the designated authority process. No consumer receives the writer.
pub fn serve(writer: BarProfileWriter) -> Result<Connection, zbus::Error> {
    let auth = DbusCallerAuthenticator::session()?;
    Builder::session()?
        .max_queued(32)
        .serve_at(
            OBJECT_PATH,
            ProfileBoundary {
                auth,
                writer: Mutex::new(writer),
            },
        )?
        .name(BUS_NAME)?
        .build()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_edge_and_ids_are_refused_before_dispatch() {
        assert!(
            decode_request(
                vec![1; 16],
                vec![2; 16],
                vec![3; 16],
                1,
                vec![4; 32],
                "diagonal"
            )
            .is_err()
        );
        assert!(
            decode_request(vec![1; 15], vec![2; 16], vec![3; 16], 1, vec![4; 32], "top").is_err()
        );
        assert!(
            decode_request(vec![1; 16], vec![2; 16], vec![3; 16], 1, vec![4; 31], "top").is_err()
        );
    }
    #[test]
    fn all_four_edges_decode_to_only_the_bounded_operation() {
        for (wire, edge) in [
            ("top", BarEdgePreference::Top),
            ("right", BarEdgePreference::Right),
            ("bottom", BarEdgePreference::Bottom),
            ("left", BarEdgePreference::Left),
        ] {
            let request =
                decode_request(vec![1; 16], vec![2; 16], vec![3; 16], 4, vec![5; 32], wire)
                    .unwrap();
            assert_eq!(request.edge, edge);
            assert_eq!(
                request.expected,
                ProfileVersion::Current {
                    revision: 4,
                    etag: [5; 32]
                }
            );
        }
        for wire in ["Top", "", "bar.compact", "set", "exec"] {
            assert!(
                decode_request(vec![1; 16], vec![2; 16], vec![3; 16], 4, vec![5; 32], wire)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod donor11_tests {
    use super::*;

    fn decode(raw: &str) -> fdo::Result<SetBarPreferenceOverrides> {
        decode_preferences_request(vec![1; 16], vec![2; 16], vec![3; 16], 4, vec![5; 32], raw)
    }

    #[test]
    fn donor_preferences_wire_decodes_only_the_five_bounded_fields() {
        let request = decode(r#"{"persistent":false,"show_on_hover":true,"workspace_count":20,"workspace_display":"icons","tray_compact":false}"#).unwrap();
        assert_eq!(request.request_id, [1; 16]);
        assert_eq!(request.epoch, [2; 16]);
        assert_eq!(request.profile_id, DesktopProfileId::from_bytes([3; 16]));
        assert_eq!(request.preferences.persistent, Some(false));
        assert_eq!(request.preferences.show_on_hover, Some(true));
        assert_eq!(request.preferences.workspace_count.unwrap().get(), 20);
        assert_eq!(
            request.preferences.workspace_display,
            Some(WorkspaceDisplayMode::Icons)
        );
        assert_eq!(request.preferences.tray_compact, Some(false));
    }

    #[test]
    fn donor_preferences_wire_refuses_unknown_enum_ordinal_and_out_of_range_count() {
        for raw in [
            r#"{"exec":"oops"}"#,
            r#"{"workspace_display":"ordinal-2"}"#,
            r#"{"workspace_display":2}"#,
            r#"{"workspace_count":0}"#,
            r#"{"workspace_count":21}"#,
        ] {
            assert!(decode(raw).is_err(), "accepted {raw}");
        }
        assert!(decode("{}").is_ok());
    }
}
