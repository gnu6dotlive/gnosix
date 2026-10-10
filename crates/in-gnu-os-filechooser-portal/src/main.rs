//! GFC-01 protocol-only backend. Not ready to replace the user's active picker.
use in_gnu_os_filechooser_portal::{
    AdmitError, PickerInvocation, PickerKind, Registry, Resolution, BUS_NAME, MAX_REQUESTS,
    OBJECT_PATH,
};
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::fdo;
use zbus::zvariant::{Array, ObjectPath, OwnedValue, Value};
use zbus::Connection;

#[derive(Clone)]
struct FileChooser {
    registry: Registry,
    ui: mpsc::Sender<PickerInvocation>,
}

struct RequestClose {
    registry: Registry,
    handle: String,
    epoch: u64,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestClose {
    async fn close(&self) {
        self.registry.cancel_if_epoch(&self.handle, self.epoch);
    }
}

impl FileChooser {
    async fn invoke(
        &self,
        connection: &Connection,
        request: PickerInvocation,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        let handle = request.handle.clone();
        let mut ticket = self.registry.admit(&request).map_err(|error| match error {
            AdmitError::InvalidOptions => fdo::Error::InvalidArgs("invalid chooser options".into()),
            AdmitError::Duplicate => fdo::Error::InvalidArgs("duplicate request handle".into()),
            AdmitError::Capacity => fdo::Error::LimitsExceeded("request capacity reached".into()),
        })?;
        let path = ObjectPath::try_from(handle.as_str())
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        if let Err(error) = connection
            .object_server()
            .at(
                path.clone(),
                RequestClose {
                    registry: self.registry.clone(),
                    handle: handle.clone(),
                    epoch: ticket.epoch(),
                },
            )
            .await
        {
            self.registry.complete(&handle, Resolution::Unavailable);
            return Err(fdo::Error::Failed(error.to_string()));
        }

        // No renderer is attached in GFC-01. Never silently delegate to GTK/Qt,
        // never synthesize a user's selection, and never wedge the caller.
        if self.ui.try_send(request).is_err() {
            self.registry.complete(&handle, Resolution::Unavailable);
        }
        let reply = ticket.wait().await;
        // A completed request is no longer addressable through Request.Close.
        let _ = connection
            .object_server()
            .remove::<RequestClose, _>(&path)
            .await;

        let mut results = HashMap::new();
        if reply.code == 0 {
            results.insert("uris".into(), Value::new(Array::from(reply.uris)));
        }
        Ok((reply.code, results))
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    async fn open_file(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        self.invoke(
            connection,
            PickerInvocation {
                handle: handle.to_string(),
                app_id: app_id.into(),
                parent_window: parent_window.into(),
                title: title.into(),
                kind: PickerKind::OpenFile,
                options,
            },
        )
        .await
    }

    async fn save_file(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        self.invoke(
            connection,
            PickerInvocation {
                handle: handle.to_string(),
                app_id: app_id.into(),
                parent_window: parent_window.into(),
                title: title.into(),
                kind: PickerKind::SaveFile,
                options,
            },
        )
        .await
    }

    async fn save_files(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        self.invoke(
            connection,
            PickerInvocation {
                handle: handle.to_string(),
                app_id: app_id.into(),
                parent_window: parent_window.into(),
                title: title.into(),
                kind: PickerKind::SaveFiles,
                options,
            },
        )
        .await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(MAX_REQUESTS);
    let (ui, receiver) = mpsc::channel(1);
    // No GPUI renderer has been qualified yet. Without a consumer, every call
    // returns response=2 instead of silently choosing a path.
    drop(receiver);
    let backend = FileChooser { registry, ui };
    let connection = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, backend)?
        .build()
        .await?;
    eprintln!(
        "GFC-01 protocol-only backend on {BUS_NAME}; renderer unavailable; host defaults unchanged"
    );
    let _keep_connection = connection;
    std::future::pending::<()>().await;
    Ok(())
}
