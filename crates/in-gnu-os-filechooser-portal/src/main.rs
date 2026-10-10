//! GFC-02 opt-in backend. Never switches the user's active XDG portal.
use in_gnu_os_filechooser_portal::render_wire::{RenderRequest, RenderResponse, MAX_WIRE_BYTES};
use in_gnu_os_filechooser_portal::{
    AdmitError, PickerInvocation, PickerKind, Registry, Resolution, BUS_NAME, MAX_REQUESTS,
    OBJECT_PATH,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::{watch, Semaphore};
use zbus::fdo;
use zbus::zvariant::{Array, ObjectPath, OwnedValue, Value};
use zbus::Connection;

#[derive(Clone)]
struct FileChooser {
    registry: Registry,
    ui: mpsc::Sender<RenderJob>,
}

struct RenderJob {
    request: PickerInvocation,
    epoch: u64,
    cancellation: watch::Receiver<bool>,
}

async fn launch_renderer(
    request: &PickerInvocation,
    mut cancelled: watch::Receiver<bool>,
    renderer: &PathBuf,
) -> Resolution {
    if *cancelled.borrow() {
        return Resolution::Cancelled;
    }
    let wire = match RenderRequest::from_invocation(request) {
        Ok(wire) => wire,
        Err(_) => return Resolution::Unavailable,
    };
    let input = match serde_json::to_vec(&wire) {
        Ok(input) if input.len() <= MAX_WIRE_BYTES => input,
        _ => return Resolution::Unavailable,
    };
    let mut child = match Command::new(renderer)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Resolution::Unavailable,
    };
    let Some(mut stdin) = child.stdin.take() else {
        return Resolution::Unavailable;
    };
    if stdin.write_all(&input).await.is_err() {
        return Resolution::Unavailable;
    }
    drop(stdin);
    let Some(stdout) = child.stdout.take() else {
        return Resolution::Unavailable;
    };
    let read = async move {
        let mut buffer = Vec::new();
        stdout
            .take((MAX_WIRE_BYTES + 1) as u64)
            .read_to_end(&mut buffer)
            .await?;
        if buffer.len() > MAX_WIRE_BYTES {
            return Ok::<_, std::io::Error>(None);
        }
        let status = child.wait().await?;
        Ok(Some((status, buffer)))
    };
    tokio::select! {
        _ = cancelled.changed() => Resolution::Cancelled,
        output = tokio::time::timeout(Duration::from_secs(600), read) => {
            let Ok(Ok(Some((status, output)))) = output else { return Resolution::Unavailable; };
            if !status.success() {
                return Resolution::Unavailable;
            }
            match serde_json::from_slice::<RenderResponse>(&output) {
                Ok(response) => response.into_resolution(&wire),
                Err(_) => Resolution::Unavailable,
            }
        }
    }
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

        // No renderer or unsupported request: fail closed; never delegate
        // silently to GTK/Qt or fabricate the user's selection.
        if self
            .ui
            .try_send(RenderJob {
                request,
                epoch: ticket.epoch(),
                cancellation: ticket.cancellation(),
            })
            .is_err()
        {
            self.registry
                .complete_if_epoch(&handle, ticket.epoch(), Resolution::Unavailable);
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
    let (ui, mut receiver) = mpsc::channel::<RenderJob>(MAX_REQUESTS);
    // Activation requires an explicit absolute renderer path. No renderer:
    // requests are rejected, and the user's installed portal stays untouched.
    let configured = std::env::var_os("GNOSIX_GFC02_RENDERER")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file());
    if let Some(renderer) = configured {
        let registry_for_worker = registry.clone();
        let running_limit = Arc::new(Semaphore::new(8));
        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let registry = registry_for_worker.clone();
                let renderer = renderer.clone();
                let permits = running_limit.clone();
                tokio::spawn(async move {
                    let Ok(_permit) = permits.acquire_owned().await else {
                        return;
                    };
                    let handle = job.request.handle.clone();
                    let result = launch_renderer(&job.request, job.cancellation, &renderer).await;
                    registry.complete_if_epoch(&handle, job.epoch, result);
                });
            }
        });
    } else {
        // Receiver dropped: any method returns fail-closed status=2.
        drop(receiver);
    }
    let backend = FileChooser { registry, ui };
    let connection = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, backend)?
        .build()
        .await?;
    eprintln!("GFC-02 opt-in backend on {BUS_NAME}; host portal defaults unchanged");
    let _keep_connection = connection;
    std::future::pending::<()>().await;
    Ok(())
}
