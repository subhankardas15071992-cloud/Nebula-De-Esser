use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredPresetSnapshot {
    pub threshold: f32,
    pub max_reduction: f32,
    pub min_freq: f32,
    pub max_freq: f32,
    pub mode_relative: bool,
    pub basis_mode: i32,
    pub use_wide_range: bool,
    pub filter_solo: bool,
    pub lookahead_enabled: bool,
    pub lookahead_ms: f32,
    pub trigger_hear: bool,
    pub stereo_link: f32,
    pub stereo_mid_side: bool,
    #[serde(default)]
    pub sidechain_mode: i32,
    #[serde(default)]
    pub sidechain_external: bool,
    pub vocal_mode: bool,
    pub input_level: f32,
    pub input_pan: f32,
    pub output_level: f32,
    pub output_pan: f32,
    pub bypass: bool,
    pub oversampling: i32,
    pub cut_width: f32,
    pub cut_depth: f32,
    pub mix: f32,
    pub cut_slope: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredPreset {
    pub name: String,
    pub snapshot: StoredPresetSnapshot,
}

impl StoredPresetSnapshot {
    pub(crate) fn effective_sidechain_mode(&self) -> i32 {
        if self.sidechain_mode != 0 {
            self.sidechain_mode.clamp(0, 2)
        } else if self.sidechain_external {
            1
        } else {
            0
        }
    }
}

#[derive(Clone)]
pub(crate) struct StoredMidiState {
    pub mappings: HashMap<u8, u8>,
    pub midi_enabled: bool,
}

impl Default for StoredMidiState {
    fn default() -> Self {
        Self {
            mappings: HashMap::new(),
            midi_enabled: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredStateFile {
    presets: Vec<StoredPreset>,
    midi_mappings: HashMap<u8, u8>,
    midi_enabled: bool,
}

impl Default for StoredStateFile {
    fn default() -> Self {
        Self {
            presets: Vec::new(),
            midi_mappings: HashMap::new(),
            midi_enabled: true,
        }
    }
}

pub(crate) struct PersistentStore {
    path: PathBuf,
    state: Mutex<StoredStateFile>,
}

impl PersistentStore {
    pub(crate) fn load() -> Self {
        let path = storage_path();
        let state = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StoredStateFile>(&bytes) {
                Ok(state) => state,
                Err(err) => {
                    eprintln!(
                        "Nebula DeEsser: failed to parse persisted state at {}: {err}",
                        path.display()
                    );
                    StoredStateFile::default()
                }
            },
            Err(err) if err.kind() == ErrorKind::NotFound => StoredStateFile::default(),
            Err(err) => {
                eprintln!(
                    "Nebula DeEsser: failed to read persisted state at {}: {err}",
                    path.display()
                );
                StoredStateFile::default()
            }
        };

        Self {
            path,
            state: Mutex::new(state),
        }
    }

    pub(crate) fn presets(&self) -> Vec<StoredPreset> {
        self.state.lock().presets.clone()
    }

    pub(crate) fn save_presets(&self, presets: Vec<StoredPreset>) {
        self.update(|state| {
            state.presets = presets;
        });
    }

    pub(crate) fn midi_state(&self) -> StoredMidiState {
        let state = self.state.lock();
        StoredMidiState {
            mappings: state.midi_mappings.clone(),
            midi_enabled: state.midi_enabled,
        }
    }

    pub(crate) fn save_midi_state(&self, midi_state: StoredMidiState) {
        self.update(|state| {
            state.midi_mappings = midi_state.mappings;
            state.midi_enabled = midi_state.midi_enabled;
        });
    }

    fn update(&self, f: impl FnOnce(&mut StoredStateFile)) {
        let mut state = self.state.lock();
        f(&mut state);
        if let Err(err) = write_state_file(&self.path, &state) {
            eprintln!(
                "Nebula DeEsser: failed to persist state at {}: {err}",
                self.path.display()
            );
        }
    }
}

fn write_state_file(path: &Path, state: &StoredStateFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let bytes = serde_json::to_vec_pretty(state).map_err(|err| err.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, bytes).map_err(|err| err.to_string())?;

    if let Err(rename_err) = fs::rename(&tmp_path, path) {
        if path.exists() {
            fs::remove_file(path).map_err(|err| err.to_string())?;
            fs::rename(&tmp_path, path).map_err(|err| err.to_string())?;
        } else {
            return Err(rename_err.to_string());
        }
    }

    Ok(())
}

fn storage_path() -> PathBuf {
    storage_root()
        .join("Nebula Audio")
        .join("Nebula DeEsser")
        .join("state.json")
}

#[cfg(target_os = "windows")]
fn storage_root() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|path| path.join("AppData").join("Roaming"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn storage_root() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join("Library").join("Application Support"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn storage_root() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
}
