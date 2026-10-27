//! Private fixed read-only queries on Hyprland's existing observation socket.
use std::{
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    path::{Component, Path},
    time::{Duration, Instant},
};

const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum ReadQuery {
    Outputs,
    Devices,
    SemanticSnapshot,
    /// WS-04: the client windows alone (address and stable id).
    Clients,
}

impl ReadQuery {
    fn wire(self) -> &'static str {
        match self {
            Self::Outputs => "j/monitors",
            Self::Devices => "j/devices",
            Self::Clients => "j/clients",
            Self::SemanticSnapshot => {
                "[[BATCH]]j/monitors ; j/workspaces ; j/clients ; j/activeworkspace ; j/activewindow ; j/layers ; j/monitors ; j/workspaces ; j/clients ; j/activeworkspace ; j/activewindow ; j/layers"
            }
        }
    }
}

pub(crate) fn read_observation(instance: &str, query: ReadQuery) -> io::Result<Vec<u8>> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing runtime dir"))?;
    read_observation_at(Path::new(&root), instance, query)
}

pub(crate) fn read_observation_at(
    runtime: &Path,
    instance: &str,
    query: ReadQuery,
) -> io::Result<Vec<u8>> {
    read_bounded(runtime, instance, query, QUERY_TIMEOUT, MAX_REPLY_BYTES)
}

fn read_bounded(
    runtime: &Path,
    instance: &str,
    query: ReadQuery,
    timeout: Duration,
    max_bytes: usize,
) -> io::Result<Vec<u8>> {
    let mut components = Path::new(instance).components();
    if !runtime.is_absolute()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid observation session path",
        ));
    }
    let path = runtime.join("hypr").join(instance).join(".socket.sock");
    let until = Instant::now() + timeout;
    let mut stream = connect_nonblocking(&path)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(query.wire().as_bytes())?;
    let mut reply = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "observation deadline",
            ));
        }
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(reply),
            Ok(count) => {
                if count > max_bytes.saturating_sub(reply.len()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "observation response too large",
                    ));
                }
                reply.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

// Linux Unix sockets either connect immediately or fail with EAGAIN when the
// local accept queue is full. Never block the scene behind that queue, and do
// not retry an unknown peer here; existing read-only reconciliation owns retry.
pub(crate) fn connect_nonblocking(path: &Path) -> io::Result<UnixStream> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: zero is a valid initial value for sockaddr_un's integer/byte fields.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "observation socket path too long or contains NUL",
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    // SAFETY: fixed valid socket domain/type; successful descriptor becomes owned immediately.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: raw is fresh and owned by this function, not shared with another owner.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let length =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    // SAFETY: address is initialized and length covers exactly family/path/NUL within its storage.
    if unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(false)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn observation_reads_fixed_query_from_peer_without_hyprctl_dependency() {
        let root = std::env::temp_dir().join(format!("go-{}", uuid::Uuid::new_v4().simple()));
        let instance = "test-session";
        let dir = root.join("hypr").join(instance);
        fs::create_dir_all(&dir).unwrap();
        let listener = UnixListener::bind(dir.join(".socket.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let peer = thread::spawn(move || {
            let until = Instant::now() + Duration::from_millis(500);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0u8; 512];
                        let count = stream.read(&mut request).unwrap();
                        assert_eq!(&request[..count], b"j/monitors");
                        stream.write_all(b"[]").unwrap();
                        return;
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < until =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    _ => return,
                }
            }
        });
        let result = read_observation_at(&root, instance, ReadQuery::Outputs);
        peer.join().unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(result.unwrap(), b"[]");
    }
    fn with_peer(
        query: ReadQuery,
        response: Vec<u8>,
        limit: usize,
        hold: Duration,
    ) -> io::Result<Vec<u8>> {
        let root = std::env::temp_dir().join(format!("go-{}", uuid::Uuid::new_v4().simple()));
        let dir = root.join("hypr/test-session");
        fs::create_dir_all(&dir).unwrap();
        let listener = UnixListener::bind(dir.join(".socket.sock")).unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0u8; 512];
            let count = stream.read(&mut request).unwrap();
            assert_eq!(&request[..count], query.wire().as_bytes());
            for part in response.chunks(19) {
                if stream.write_all(part).is_err() {
                    break;
                }
            }
            thread::sleep(hold);
        });
        let timeout = if hold.is_zero() {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(40)
        };
        let result = read_bounded(&root, "test-session", query, timeout, limit);
        peer.join().unwrap();
        fs::remove_dir_all(&root).unwrap();
        result
    }

    #[test]
    fn devices_observation_is_one_fixed_read_only_query() {
        assert_eq!(
            with_peer(
                ReadQuery::Devices,
                br#"{"keyboards":[]}"#.to_vec(),
                1024,
                Duration::ZERO,
            )
            .unwrap(),
            br#"{"keyboards":[]}"#
        );
    }

    #[test]
    fn stable_semantic_batch_remains_twelve_fixed_read_queries() {
        let response = b"[] [] [] {} {} {} [] [] [] {} {} {}".to_vec();
        assert_eq!(ReadQuery::SemanticSnapshot.wire().matches("j/").count(), 12);
        assert_eq!(
            with_peer(
                ReadQuery::SemanticSnapshot,
                response.clone(),
                1024,
                Duration::ZERO
            )
            .unwrap(),
            response
        );
    }

    #[test]
    fn fragmented_large_reply_is_read_through_eof() {
        let response = vec![b'a'; 20_000];
        assert_eq!(
            with_peer(ReadQuery::Outputs, response.clone(), 21_000, Duration::ZERO).unwrap(),
            response
        );
    }

    #[test]
    fn oversized_response_is_refused() {
        assert_eq!(
            with_peer(ReadQuery::Outputs, vec![b'a'; 100], 32, Duration::ZERO)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn stalled_peer_has_a_bounded_deadline() {
        let error =
            with_peer(ReadQuery::Outputs, vec![], 100, Duration::from_millis(100)).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn malformed_session_or_socket_path_is_refused() {
        for value in ["", "../wrong", "/absolute", "a/b", ".", "a\0b"] {
            assert_eq!(
                read_observation_at(Path::new("/tmp"), value, ReadQuery::Outputs)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(
            read_observation_at(Path::new("relative"), "test", ReadQuery::Outputs)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            connect_nonblocking(Path::new(&format!("/tmp/{}", "x".repeat(200))))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
