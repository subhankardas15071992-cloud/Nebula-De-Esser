use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const DEFAULT_EDITOR_WIDTH: f32 = 860.0;
const DEFAULT_EDITOR_HEIGHT: f32 = 640.0;
const MIN_EDITOR_SCALE: f32 = 0.65;
const MAX_EDITOR_SCALE: f32 = 3.0;

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
    #[serde(default)]
    pub stereo_mode: i32,
    #[serde(default)]
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
    pub(crate) fn effective_stereo_mode(&self) -> i32 {
        if self.stereo_mode != 0 {
            self.stereo_mode.clamp(0, 2)
        } else if self.stereo_mid_side {
            2
        } else {
            0
        }
    }

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

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(crate) struct StoredEditorSize {
    #[serde(default = "default_editor_width")]
    pub width: f32,
    #[serde(default = "default_editor_height")]
    pub height: f32,
}

impl Default for StoredEditorSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_EDITOR_WIDTH,
            height: DEFAULT_EDITOR_HEIGHT,
        }
    }
}

impl StoredEditorSize {
    pub(crate) fn clamped(self) -> Self {
        Self {
            width: self.width.clamp(
                DEFAULT_EDITOR_WIDTH * MIN_EDITOR_SCALE,
                DEFAULT_EDITOR_WIDTH * MAX_EDITOR_SCALE,
            ),
            height: self.height.clamp(
                DEFAULT_EDITOR_HEIGHT * MIN_EDITOR_SCALE,
                DEFAULT_EDITOR_HEIGHT * MAX_EDITOR_SCALE,
            ),
        }
    }
}

fn default_editor_width() -> f32 {
    DEFAULT_EDITOR_WIDTH
}

fn default_editor_height() -> f32 {
    DEFAULT_EDITOR_HEIGHT
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredStateFile {
    presets: Vec<StoredPreset>,
    midi_mappings: HashMap<u8, u8>,
    midi_enabled: bool,
    editor_size: StoredEditorSize,
}

impl Default for StoredStateFile {
    fn default() -> Self {
        Self {
            presets: Vec::new(),
            midi_mappings: HashMap::new(),
            midi_enabled: true,
            editor_size: StoredEditorSize::default(),
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
        let state = match read_state_file(&path) {
            Ok(Some(state)) => state,
            Ok(None) => load_legacy_state(&path).unwrap_or_default(),
            Err(()) => StoredStateFile::default(),
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

    pub(crate) fn editor_size(&self) -> StoredEditorSize {
        self.state.lock().editor_size.clamped()
    }

    pub(crate) fn save_editor_size(&self, editor_size: StoredEditorSize) {
        self.update(|state| {
            state.editor_size = editor_size.clamped();
        });
    }

    fn update(&self, f: impl FnOnce(&mut StoredStateFile)) {
        let mut state = self.state.lock();
        f(&mut state);
        if let Err(err) = write_state_file(&self.path, &state) {
            eprintln!(
                "Nebula De-Esser: failed to persist state at {}: {err}",
                self.path.display()
            );
        }
    }
}

fn load_legacy_state(current_path: &Path) -> Option<StoredStateFile> {
    let legacy_path = legacy_storage_path();
    let state = match read_state_file(&legacy_path) {
        Ok(Some(state)) => state,
        Ok(None) | Err(()) => return None,
    };

    if let Err(err) = write_state_file(current_path, &state) {
        eprintln!(
            "Nebula De-Esser: failed to migrate persisted state from {} to {}: {err}",
            legacy_path.display(),
            current_path.display()
        );
    }

    Some(state)
}

fn read_state_file(path: &Path) -> Result<Option<StoredStateFile>, ()> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            eprintln!(
                "Nebula De-Esser: failed to read persisted state at {}: {err}",
                path.display()
            );
            return Err(());
        }
    };

    match serde_json::from_slice::<StoredStateFile>(&bytes) {
        Ok(state) => Ok(Some(state)),
        Err(err) => {
            eprintln!(
                "Nebula De-Esser: failed to parse persisted state at {}: {err}",
                path.display()
            );
            Err(())
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
        .join("Nebula De-Esser")
        .join("state.json")
}

fn legacy_storage_path() -> PathBuf {
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
