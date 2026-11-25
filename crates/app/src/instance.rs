//! One TinyHand per user. The running app listens on a localhost TCP port and writes
//! `<port> <token>` to `instance` in the config folder; `tinyhand toggle` (or a
//! `tinyhand://` URL passed on the command line) reads it, sends `<token> <words…>` and
//! exits. TCP on 127.0.0.1 works the same on every OS; the token, readable only through
//! the user's own config folder, keeps other users' processes from sending commands.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use tinyhand_core::automation::{self, Command};

fn instance_path() -> PathBuf {
    tinyhand_core::settings::config_dir().join("instance")
}

/// What `instance` says: the port and the token.
fn read_instance() -> Option<(u16, String)> {
    let text = std::fs::read_to_string(instance_path()).ok()?;
    let mut words = text.split_whitespace();
    let port = words.next()?.parse().ok()?;
    let token = words.next()?.to_string();
    Some((port, token))
}

fn connect(port: u16) -> Option<TcpStream> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    Some(stream)
}

/// Sends `words` to a running TinyHand. `Ok(true)` if one took them, `Ok(false)` if none is
/// running; `Err` if it answered with an error.
pub fn send(words: &[String]) -> Result<bool, String> {
    let Some((port, token)) = read_instance() else { return Ok(false) };
    let Some(mut stream) = connect(port) else { return Ok(false) };
    let line = format!("{token} {}\n", words.join(" "));
    if stream.write_all(line.as_bytes()).is_err() {
        return Ok(false);
    }
    let mut answer = String::new();
    let _ = BufReader::new(stream).read_line(&mut answer);
    match answer.trim() {
        "ok" => Ok(true),
        "" => Ok(false),
        other => Err(other.to_string()),
    }
}

/// Whether another TinyHand answers on the port in `instance`.
pub fn running() -> bool {
    send(&["ping".to_string()]).unwrap_or(true)
}

/// Listens for commands from later launches; each parsed command goes to `deliver`. The
/// listener thread lives as long as the process.
pub fn listen(deliver: impl Fn(Command) + Send + 'static) -> Result<(), String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = new_token();
    tinyhand_core::settings::write_atomic(&instance_path(), format!("{port} {token}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("tinyhand commands".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                let mut reader = BufReader::new(&stream);
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut words = line.split_whitespace().map(String::from);
                let answer = if words.next().as_deref() != Some(token.as_str()) {
                    "error: wrong token"
                } else {
                    let words: Vec<String> = words.collect();
                    if words.first().is_some_and(|w| w == "ping") {
                        "ok"
                    } else if let Some(command) = automation::parse_args(&words) {
                        deliver(command);
                        "ok"
                    } else {
                        "error: unknown command"
                    }
                };
                let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Removes `instance` if it is still ours (call on quit).
pub fn forget() {
    let _ = std::fs::remove_file(instance_path());
}

fn new_token() -> String {
    // Not a secret from the user, only from other users: time, pid and the address of a
    // stack value are plenty.
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.write_u32(std::process::id());
    let a = h.finish();
    h.write_u64(a);
    format!("{a:016x}{:016x}", h.finish())
}
