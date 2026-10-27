//! Tell a running interview window what to do from outside: a compositor key, a script.
//!
//! The same small channel as the Python window, so `careertwin-interview-ctl screenshot` bound to a key in niri
//! or sway works with either: a Unix socket in a directory only this user can enter, one word per connection
//! (`screenshot`, `ask` or `ping`), answered with `ok`.

pub const COMMANDS: [&str; 3] = ["screenshot", "ask", "ping"];
pub const NAME: &str = "careertwin-interview";

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::{COMMANDS, NAME};

    /// A real directory (not a link) that belongs to this user and nobody else can enter.
    fn private(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|info| {
            info.is_dir() && info.uid() == rustix::process::getuid().as_raw() && info.mode() & 0o077 == 0
        })
    }

    /// Where the window listens: the runtime directory when it is private, else `<tmp>/careertwin-interview-<uid>/`,
    /// created with mode 0700 and checked (a directory someone else made there, or a link, is refused): a socket in
    /// the shared /tmp could otherwise be connected to by another account between `bind` and `chmod`.
    pub fn socket_path() -> Result<PathBuf, String> {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|dir| private(dir)) {
            return Ok(runtime.join(format!("{NAME}.sock")));
        }
        let folder = std::env::temp_dir().join(format!("{NAME}-{}", rustix::process::getuid().as_raw()));
        match std::fs::create_dir(&folder) {
            Ok(()) => std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("cannot protect {}: {error}", folder.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("cannot create {}: {error}", folder.display())),
        }
        if !private(&folder) {
            return Err(format!("{} is not private to you; remove it and start the window again", folder.display()));
        }
        Ok(folder.join(format!("{NAME}.sock")))
    }

    /// Send one command to the window listening at `path` and return its one-line reply.
    pub fn send_to(path: &Path, command: &str) -> Result<String, String> {
        if !COMMANDS.contains(&command) {
            return Err(format!("Unknown command {command:?}; use one of {}", COMMANDS.join(", ")));
        }
        let not_running = |_| "CareerTwin Interview is not running".to_owned();
        let mut client = UnixStream::connect(path).map_err(not_running)?;
        client.set_read_timeout(Some(Duration::from_secs(2))).map_err(not_running)?;
        client.write_all(format!("{command}\n").as_bytes()).map_err(not_running)?;
        let mut reply = String::new();
        BufReader::new(client).read_line(&mut reply).map_err(not_running)?;
        Ok(reply.trim().to_owned())
    }

    pub fn send(command: &str) -> Result<String, String> {
        send_to(&socket_path()?, command)
    }

    /// The window's side: a thread that reads one-word commands and hands them to `callback`. The socket file is
    /// removed when this is dropped.
    pub struct Server {
        path: PathBuf,
    }

    impl Server {
        /// Listen at `path`; an error when another window already holds it.
        pub fn start_at(path: &Path, callback: impl Fn(&str) + Send + 'static) -> Result<Self, String> {
            if send_to(path, "ping").is_ok() {
                return Err("another CareerTwin Interview window is already running".to_owned());
            }
            let _ = std::fs::remove_file(path); // nobody answers, so a file left here is from a crashed run
            let listener = UnixListener::bind(path).map_err(|error| error.to_string())?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
            std::thread::Builder::new()
                .name("control".into())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let mut line = String::new();
                        if BufReader::new(&stream).read_line(&mut line).is_err() {
                            continue;
                        }
                        let name = line.trim();
                        let mut stream = stream;
                        if COMMANDS.contains(&name) {
                            if name != "ping" {
                                callback(name);
                            }
                            let _ = stream.write_all(b"ok\n");
                        } else {
                            let _ = stream.write_all(format!("unknown command {name:?}\n").as_bytes());
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(Self { path: path.to_path_buf() })
        }

        pub fn start(callback: impl Fn(&str) + Send + 'static) -> Result<Self, String> {
            Self::start_at(&socket_path()?, callback)
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
pub use unix::{Server, send, send_to, socket_path};

/// Windows has the key built in (a global hotkey), and no socket.
#[cfg(not(unix))]
pub fn send(_command: &str) -> Result<String, String> {
    Err("not needed on Windows: Ctrl+Shift+Enter works in every program".to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn a_command_reaches_the_window_and_is_answered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.sock");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let _server = Server::start_at(&path, move |command| record.lock().unwrap().push(command.to_owned())).unwrap();
        assert_eq!(send_to(&path, "ping").unwrap(), "ok");
        assert_eq!(send_to(&path, "screenshot").unwrap(), "ok");
        assert_eq!(send_to(&path, "ask").unwrap(), "ok");
        for _ in 0..50 {
            if seen.lock().unwrap().len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(*seen.lock().unwrap(), ["screenshot", "ask"]); // a ping is not a command for the window
    }

    #[test]
    fn a_second_window_is_refused_and_an_unknown_command_is_not_sent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.sock");
        let _server = Server::start_at(&path, |_| {}).unwrap();
        assert!(Server::start_at(&path, |_| {}).err().unwrap().contains("already running"));
        assert!(send_to(&path, "reboot").unwrap_err().contains("Unknown command"));
    }

    #[test]
    fn nobody_listening_says_so_and_the_socket_is_removed_with_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.sock");
        assert_eq!(send_to(&path, "ping").unwrap_err(), "CareerTwin Interview is not running");
        drop(Server::start_at(&path, |_| {}).unwrap());
        assert!(!path.exists());
        // a socket left by a crashed run is replaced
        let stale = UnixListener::bind(&path);
        drop(stale);
        assert!(Server::start_at(&path, |_| {}).is_ok());
    }

    use std::os::unix::net::UnixListener;
}
