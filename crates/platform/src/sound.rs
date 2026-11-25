//! The tap sound. All audio work happens on one worker thread, like the Swift serial queue,
//! so a slow or stuck output device costs a missing sound, never a frozen pointer.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::cpal::traits::HostTrait;
use rodio::{DeviceSinkBuilder, DeviceTrait, Decoder, MixerDeviceSink, Source, cpal};

/// Drop the output stream after this long without a tap.
const IDLE: Duration = Duration::from_secs(30);
/// How often a tap checks whether the default output device changed.
const DEVICE_CHECK: Duration = Duration::from_secs(2);
/// How long to wait before trying to open a device that failed to open.
const RETRY_OPEN: Duration = Duration::from_secs(5);

/// A decoded sound: interleaved samples.
struct Clip {
    channels: u16,
    rate: u32,
    samples: Vec<f32>,
}

enum Msg {
    Play(String),
    Preload(String),
    Warm,
}

/// Plays `tap_<id>.wav` files. Cheap to hold; methods never block on audio.
pub struct TapSounds {
    tx: Mutex<Sender<Msg>>,
}

impl TapSounds {
    /// `search_dirs` are tried first, then the usual places next to the app.
    pub fn new(search_dirs: Vec<PathBuf>) -> TapSounds {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("tinyhand tap sound".into())
            .spawn(move || Worker::new(search_dirs).run(rx));
        if let Err(e) = spawned {
            eprintln!("tap sound: cannot start thread: {e}");
        }
        TapSounds { tx: Mutex::new(tx) }
    }

    /// Decode the sound ahead of the first tap.
    pub fn preload(&self, id: &str) {
        self.send(Msg::Preload(id.to_string()));
    }

    /// Play the sound now. Overlapping taps mix. Loads the file if it was not preloaded.
    pub fn play(&self, id: &str) {
        self.send(Msg::Play(id.to_string()));
    }

    /// Open the output stream ahead of the first tap.
    pub fn warm(&self) {
        self.send(Msg::Warm);
    }

    fn send(&self, msg: Msg) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(msg);
        }
    }
}

struct Worker {
    dirs: Vec<PathBuf>,
    clips: HashMap<String, Option<Arc<Clip>>>,
    sink: Option<MixerDeviceSink>,
    broken: Arc<AtomicBool>,
    device_id: Option<String>,
    last_device_check: Instant,
    last_open_failure: Option<Instant>,
    warned: bool,
}

impl Worker {
    fn new(dirs: Vec<PathBuf>) -> Worker {
        Worker {
            dirs,
            clips: HashMap::new(),
            sink: None,
            broken: Arc::new(AtomicBool::new(false)),
            device_id: None,
            last_device_check: Instant::now(),
            last_open_failure: None,
            warned: false,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Msg>) {
        loop {
            match rx.recv_timeout(IDLE) {
                Ok(Msg::Play(id)) => self.play(&id),
                Ok(Msg::Preload(id)) => {
                    self.clip(&id);
                }
                Ok(Msg::Warm) => self.ensure_sink(),
                Err(RecvTimeoutError::Timeout) => self.sink = None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn clip(&mut self, id: &str) -> Option<Arc<Clip>> {
        if !self.clips.contains_key(id) {
            let clip = find_sound(&self.dirs, id).and_then(|p| match decode(&p) {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => {
                    eprintln!("tap sound: cannot decode {}: {e}", p.display());
                    None
                }
            });
            if clip.is_none() {
                eprintln!("tap sound: no usable tap_{id}.wav");
            }
            self.clips.insert(id.to_string(), clip);
        }
        self.clips[id].clone()
    }

    fn play(&mut self, id: &str) {
        let Some(clip) = self.clip(id) else { return };
        self.ensure_sink();
        if let Some(sink) = &self.sink {
            let (Some(channels), Some(rate)) = (
                std::num::NonZero::new(clip.channels),
                std::num::NonZero::new(clip.rate),
            ) else {
                return;
            };
            sink.mixer()
                .add(SamplesBuffer::new(channels, rate, clip.samples.clone()));
        }
    }

    /// Make sure there is a healthy stream on the current default device, if one exists.
    fn ensure_sink(&mut self) {
        if self.sink.is_some() && self.last_device_check.elapsed() >= DEVICE_CHECK {
            self.last_device_check = Instant::now();
            if default_device_id() != self.device_id {
                self.sink = None;
            }
        }
        if self.sink.is_some() && self.broken.swap(false, Ordering::Relaxed) {
            self.sink = None;
        }
        if self.sink.is_some() {
            return;
        }
        if self.last_open_failure.is_some_and(|t| t.elapsed() < RETRY_OPEN) {
            return;
        }
        let broken = self.broken.clone();
        let opened = DeviceSinkBuilder::from_default_device().and_then(|b| {
            b.with_error_callback(move |e: cpal::Error| {
                if e.kind() != cpal::ErrorKind::DeviceChanged {
                    eprintln!("tap sound: stream error: {e}");
                    broken.store(true, Ordering::Relaxed);
                }
            })
            .open_stream()
        });
        match opened {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                self.sink = Some(sink);
                self.device_id = default_device_id();
                self.last_device_check = Instant::now();
                self.last_open_failure = None;
                self.warned = false;
            }
            Err(e) => {
                if !self.warned {
                    eprintln!("tap sound: no audio output: {e}");
                    self.warned = true;
                }
                self.last_open_failure = Some(Instant::now());
            }
        }
    }
}

fn default_device_id() -> Option<String> {
    let device = cpal::default_host().default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

/// Where `tap_<id>.wav` might be, in lookup order.
fn candidates(dirs: &[PathBuf], id: &str) -> Vec<PathBuf> {
    let name = format!("tap_{id}.wav");
    let mut all: Vec<PathBuf> = dirs.to_vec();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        all.push(exe_dir.clone());
        all.push(exe_dir.join("assets/sounds"));
        all.push(exe_dir.join("../share/tinyhand/sounds"));
    }
    all.push(PathBuf::from("assets/sounds"));
    all.into_iter().map(|d| d.join(&name)).collect()
}

fn find_sound(dirs: &[PathBuf], id: &str) -> Option<PathBuf> {
    candidates(dirs, id).into_iter().find(|p| p.is_file())
}

fn decode(path: &Path) -> Result<Clip, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let channels = decoder.channels().get();
    let rate = decoder.sample_rate().get();
    let samples: Vec<f32> = decoder.collect();
    if samples.is_empty() {
        return Err("no samples".into());
    }
    Ok(Clip { channels, rate, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/sounds");

    #[test]
    fn finds_and_decodes_tap_click() {
        let dirs = vec![PathBuf::from(DIR)];
        let path = find_sound(&dirs, "click").expect("tap_click.wav not found");
        let clip = decode(&path).unwrap();
        assert!(!clip.samples.is_empty());
        assert!(clip.channels >= 1 && clip.rate > 0);
        assert!(find_sound(&dirs, "no-such-sound").is_none());
    }

    #[test]
    #[ignore = "needs an audio device and plays a sound"]
    fn plays_a_tap() {
        let sounds = TapSounds::new(vec![PathBuf::from(DIR)]);
        sounds.warm();
        sounds.play("click");
        std::thread::sleep(Duration::from_millis(500));
    }
}
