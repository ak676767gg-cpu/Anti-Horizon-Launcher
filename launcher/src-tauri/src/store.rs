// Where the launcher keeps things (AntiHorizonLauncher in %APPDATA% on Windows, ~/Library/Application Support on a
// Mac, ~/.local/share on Linux; never the normal .minecraft) and its settings.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub fn root() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = crate::platform::data_dir().join("AntiHorizonLauncher");
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

/// A string field of a JSON object, or "" when it is missing.
pub fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Writes JSON through a temporary file, so a crash never leaves half a file behind.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(e)?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(e)?).map_err(e)?;
    std::fs::rename(&tmp, path).map_err(e)
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub memory_mb: u32,
    pub tuned_gc: bool,
    pub minimize_on_launch: bool,
    pub java_path: String,
    pub jvm_args: String,
    /// Turn off JEI's "this server does not provide recipes" chat message before each launch.
    pub quiet_jei: bool,
    /// Put the Anti-Horizon client in every Fabric 1.21.11 instance, not only ones made from a preset.
    pub client_everywhere: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory_mb: 4096,
            tuned_gc: true,
            minimize_on_launch: true,
            java_path: String::new(),
            jvm_args: String::new(),
            quiet_jei: true,
            client_everywhere: true,
        }
    }
}

pub fn load_settings() -> Settings {
    read_json(&root().join("settings.json")).unwrap_or_default()
}

#[tauri::command]
pub fn get_settings() -> Settings {
    load_settings()
}

#[tauri::command]
pub fn save_settings(settings: Settings) -> Result<(), String> {
    let mut s = settings;
    s.memory_mb = s.memory_mb.clamp(1024, 65536);
    write_json(&root().join("settings.json"), &s)
}

/// This computer's memory in MB, so Settings can size the memory slider to it. 0 if the system won't say.
#[tauri::command]
pub fn system_memory_mb() -> u64 {
    crate::platform::memory_mb()
}

pub fn open_path(path: &Path) {
    crate::platform::open(path);
}

#[tauri::command]
pub fn open_data_folder() {
    open_path(&root());
}

/// Opens a web page in the default browser (https only).
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("Only https links can be opened.".into());
    }
    crate::platform::open_link(&url).map_err(e)?;
    Ok(())
}
