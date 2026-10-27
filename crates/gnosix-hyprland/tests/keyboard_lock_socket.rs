//! Qualifies the production lock sampler and socket transport against a
//! private fake Hyprland socket: scripted lock changes, an outage (socket
//! removed) and recovery. The host compositor is never contacted.

use gnosix_hyprland::keyboard_lock::{
    KeyboardLockObservation as Obs, KeyboardLockSnapshot as Locks,
    observe_primary_keyboard_locks_at, sample_keyboard_locks,
};
use std::{
    io::{Read, Write},
    ops::ControlFlow,
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const INSTANCE: &str = "lock-test-session";

fn devices(primary_caps: bool, primary_num: bool) -> Vec<u8> {
    // A secondary keyboard with both locks on must never be reported.
    format!(
        r#"{{"mice":[],"keyboards":[{{"name":"secondary","main":false,"capsLock":true,"numLock":true}},{{"name":"primary","main":true,"capsLock":{primary_caps},"numLock":{primary_num}}}]}}"#
    )
    .into_bytes()
}

/// Serves the current reply to every connection. `None` removes the socket
/// (an outage) until a reply is set again.
fn serve(root: PathBuf, reply: Arc<Mutex<Option<Vec<u8>>>>) {
    let dir = root.join("hypr").join(INSTANCE);
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join(".socket.sock");
    std::thread::spawn(move || {
        loop {
            let current = reply.lock().unwrap().clone();
            let Some(body) = current else {
                let _ = std::fs::remove_file(&socket);
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            let listener = UnixListener::bind(&socket).unwrap();
            listener.set_nonblocking(true).unwrap();
            while reply.lock().unwrap().as_ref() == Some(&body) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut request = [0u8; 64];
                        let count = stream.read(&mut request).unwrap();
                        assert_eq!(&request[..count], b"j/devices", "only j/devices is read");
                        stream.write_all(&body).unwrap();
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(1)),
                }
            }
            drop(listener);
            let _ = std::fs::remove_file(&socket);
        }
    });
}

#[test]
fn sampler_delivers_primary_lock_changes_outage_and_recovery() {
    let root = std::env::temp_dir().join(format!(
        "gnos-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let reply = Arc::new(Mutex::new(Some(devices(false, true))));
    serve(root.clone(), reply.clone());
    std::thread::sleep(Duration::from_millis(30));

    // The fake compositor advances after each delivered observation, so each
    // step is a real change on the socket and nothing else is delivered.
    let mut script = vec![
        Some(devices(true, true)),
        None,
        Some(devices(true, true)),
        Some(devices(false, false)),
    ]
    .into_iter();
    let mut delivered = Vec::new();
    let read_root = root.clone();
    sample_keyboard_locks(
        || observe_primary_keyboard_locks_at(&read_root, INSTANCE),
        Duration::from_millis(5),
        |observation| {
            delivered.push(observation);
            match script.next() {
                Some(next) => {
                    *reply.lock().unwrap() = next;
                    ControlFlow::Continue(())
                }
                None => ControlFlow::Break(()),
            }
        },
    );
    let observed = |caps_lock, num_lock| {
        Obs::Observed(Locks {
            caps_lock,
            num_lock,
        })
    };
    assert_eq!(
        delivered,
        vec![
            observed(false, true),
            observed(true, true),
            Obs::Unavailable,
            observed(true, true),
            observed(false, false),
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}
