//! `tinyhand://` commands (Automation.swift) and the `state.json` file launchers read.

use crate::custom_hands::CustomHand;
use crate::styles;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundAction {
    On,
    Off,
    Toggle,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    On,
    Off,
    Toggle,
    Clear,
    /// Pick this style and turn on. The id is not checked here.
    Style(String),
    Sound(SoundAction),
}

/// Parses `tinyhand://<command>[/<argument>]`. The scheme and command are not case
/// sensitive; the argument is.
pub fn parse_url(url: &str) -> Option<Command> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("tinyhand") {
        return None;
    }
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let argument = path.split('/').find(|s| !s.is_empty()).map(percent_decode);
    build(&percent_decode(host).to_lowercase(), argument.as_deref())
}

/// Parses command line words, the same as the URL ones: `toggle`, `style lavender`,
/// `sound off`. A whole `tinyhand://` URL as the first word works too.
pub fn parse_args(args: &[String]) -> Option<Command> {
    let first = args.first()?;
    if first.contains("://") {
        return parse_url(first);
    }
    build(&first.to_lowercase(), args.get(1).map(String::as_str))
}

fn build(command: &str, argument: Option<&str>) -> Option<Command> {
    match command {
        "on" => Some(Command::On),
        "off" => Some(Command::Off),
        "toggle" => Some(Command::Toggle),
        "clear" => Some(Command::Clear),
        "style" => argument
            .filter(|a| !a.is_empty())
            .map(|a| Command::Style(a.to_string())),
        "sound" => match argument {
            Some("on") => Some(Command::Sound(SoundAction::On)),
            Some("off") => Some(Command::Sound(SoundAction::Off)),
            Some("toggle") | None => Some(Command::Sound(SoundAction::Toggle)),
            Some(_) => None,
        },
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StyleEntry {
    pub id: String,
    pub name: String,
    /// "hand", "custom" or "laser".
    #[serde(rename = "type")]
    pub kind: String,
}

/// What `state.json` says. Fields are in key order, so the file has sorted keys.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct State {
    pub active: bool,
    pub pid: u32,
    #[serde(rename = "soundOn")]
    pub sound_on: bool,
    pub style: String,
    pub styles: Vec<StyleEntry>,
}

impl State {
    /// The state of this process, listing every style.
    pub fn new(active: bool, style: &str, sound_on: bool, custom: &[CustomHand]) -> State {
        State {
            active,
            pid: std::process::id(),
            sound_on,
            style: style.to_string(),
            styles: styles::all_styles(custom)
                .into_iter()
                .map(|s| StyleEntry {
                    id: s.id,
                    name: s.title,
                    kind: s.kind.as_str().to_string(),
                })
                .collect(),
        }
    }
}

/// `state.json` in the settings folder.
pub fn state_path() -> PathBuf {
    crate::settings::config_dir().join("state.json")
}

/// Writes the state as pretty JSON, atomically, creating the folder if needed.
pub fn write_state(path: &Path, state: &State) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    crate::settings::write_atomic(path, &json)
}

/// Deletes the file; a missing file is fine.
pub fn remove_state(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
