#[cfg(unix)]
fn main() {
    use gnosix_input_runtime_observer::observe_listener_once;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: {} <unix-socket-path>", program.to_string_lossy());
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: {} <unix-socket-path>", program.to_string_lossy());
        std::process::exit(2);
    }

    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind {}: {error}", path.display());
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
        let _ = std::fs::remove_file(&path);
        eprintln!("failed to restrict {}: {error}", path.display());
        std::process::exit(1);
    }

    let result = observe_listener_once(&listener);
    drop(listener);
    let _ = std::fs::remove_file(&path);
    match result {
        Ok(receipt) => println!("{}", serde_json::to_string(&receipt).unwrap()),
        Err(error) => {
            eprintln!("runtime observation failed: {error:?}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("gnosix-input-runtime-observer requires Unix domain sockets");
    std::process::exit(2);
}
