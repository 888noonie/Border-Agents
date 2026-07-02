//! Body-local settings persistence — dock, colour, and torso length per buddy.
//!
//! File: `<config>/border-buddies/body-settings.json`. Precedence at load: env →
//! persisted → compiled default. Saves are atomic (tmp + rename) and never crash paint.

use std::{
    collections::HashMap,
    fs,
    io,
    path::{Path, PathBuf},
};

use crate::render::{DockShow, BODY_LEN_DEFAULT, BODY_LEN_MAX, BODY_LEN_MIN, CLAY_DEFAULT};

const SETTINGS_DIR: &str = "border-buddies";
const SETTINGS_FILE: &str = "body-settings.json";

/// One buddy's persisted presentation fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuddySettings {
    pub dock: DockShow,
    pub color: [u8; 3],
    pub body_len: f32,
}

impl Default for BuddySettings {
    fn default() -> Self {
        Self {
            dock: DockShow::Both,
            color: CLAY_DEFAULT,
            body_len: BODY_LEN_DEFAULT,
        }
    }
}

impl BuddySettings {
    fn clamp_body_len(&mut self) {
        self.body_len = self.body_len.clamp(BODY_LEN_MIN, BODY_LEN_MAX);
    }
}

/// Resolve `<config>`: `BB_CONFIG_DIR` → `XDG_CONFIG_HOME` → `~/.config`.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BB_CONFIG_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".config"))
        .unwrap_or_else(|_| PathBuf::from(".config"))
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_DIR).join(SETTINGS_FILE)
}

fn buddy_env_key(buddy: &str, suffix: &str) -> String {
    format!(
        "{}_{}",
        buddy.trim().to_ascii_uppercase().replace('-', "_"),
        suffix
    )
}

fn parse_color_hex(raw: &str) -> Option<[u8; 3]> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn env_color_if_set(key: &str) -> Option<[u8; 3]> {
    let raw = std::env::var(key).ok()?;
    parse_color_hex(&raw)
}

/// Map a `BB_DOCK` env value — garbage and `none` fall back to `Both` (matches pre-H4a startup).
pub fn dock_from_env_value(raw: &str) -> DockShow {
    parse_dock_str(raw).unwrap_or(DockShow::Both)
}

fn env_dock_if_set() -> Option<DockShow> {
    std::env::var("BB_DOCK").ok().map(|raw| dock_from_env_value(&raw))
}

pub fn parse_dock_str(raw: &str) -> Option<DockShow> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "head" => Some(DockShow::Head),
        "bar" => Some(DockShow::Bar),
        "both" => Some(DockShow::Both),
        _ => None,
    }
}

pub fn dock_to_str(dock: DockShow) -> &'static str {
    match dock {
        DockShow::Head => "head",
        DockShow::Bar => "bar",
        DockShow::Both => "both",
    }
}

fn load_file_map(config_dir: &Path) -> HashMap<String, BuddySettings> {
    let path = settings_path(config_dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        eprintln!("[bb-desktop-body] settings: unreadable JSON at {}", path.display());
        return HashMap::new();
    };
    let Some(obj) = value.as_object() else {
        eprintln!("[bb-desktop-body] settings: expected object at root in {}", path.display());
        return HashMap::new();
    };
    obj.iter()
        .map(|(buddy, entry)| (buddy.clone(), parse_buddy_entry(entry)))
        .collect()
}

fn parse_buddy_entry(value: &serde_json::Value) -> BuddySettings {
    let mut settings = BuddySettings::default();
    let Some(obj) = value.as_object() else {
        return settings;
    };
    if let Some(dock) = obj.get("dock").and_then(|v| v.as_str()).and_then(parse_dock_str) {
        settings.dock = dock;
    }
    if let Some(arr) = obj.get("color").and_then(|v| v.as_array()) {
        if arr.len() == 3 {
            let r = arr[0].as_u64().and_then(|n| u8::try_from(n).ok());
            let g = arr[1].as_u64().and_then(|n| u8::try_from(n).ok());
            let b = arr[2].as_u64().and_then(|n| u8::try_from(n).ok());
            if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                settings.color = [r, g, b];
            }
        }
    }
    if let Some(n) = obj.get("body_len").and_then(|v| v.as_f64()) {
        settings.body_len = n as f32;
    }
    settings.clamp_body_len();
    settings
}

/// Load persisted settings for one buddy; missing file / bad entry → defaults per field.
pub fn load_buddy_settings(config_dir: &Path, buddy: &str) -> BuddySettings {
    load_file_map(config_dir)
        .get(buddy)
        .copied()
        .unwrap_or_default()
}

/// Startup resolution: env → persisted → default (per field).
pub fn resolve_startup(buddy: &str, config_dir: &Path) -> BuddySettings {
    let persisted = load_buddy_settings(config_dir, buddy);
    let dock = env_dock_if_set().unwrap_or(persisted.dock);
    let color = env_color_if_set("BB_COLOR")
        .or_else(|| {
            std::env::var(buddy_env_key(buddy, "COLOR"))
                .ok()
                .and_then(|v| parse_color_hex(&v))
        })
        .unwrap_or(persisted.color);
    let body_len = persisted.body_len.clamp(BODY_LEN_MIN, BODY_LEN_MAX);
    BuddySettings { dock, color, body_len }
}

fn buddy_entry_to_json(settings: &BuddySettings) -> serde_json::Value {
    serde_json::json!({
        "dock": dock_to_str(settings.dock),
        "color": [settings.color[0], settings.color[1], settings.color[2]],
        "body_len": settings.body_len,
    })
}

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!("{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    fs::write(&tmp, contents)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Read-modify-write: update this buddy's entry, preserving others. Atomic tmp+rename.
pub fn save_buddy_settings(config_dir: &Path, buddy: &str, settings: &BuddySettings) -> io::Result<()> {
    let mut map = load_file_map(config_dir);
    let mut entry = *settings;
    entry.clamp_body_len();
    map.insert(buddy.to_string(), entry);
    let mut root = serde_json::Map::new();
    for (name, s) in &map {
        root.insert(name.clone(), buddy_entry_to_json(s));
    }
    let path = settings_path(config_dir);
    let json = serde_json::to_string_pretty(&serde_json::Value::Object(root))?;
    atomic_write(&path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_test_guard() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn temp_config_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bb-settings-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_roundtrip_per_buddy() {
        let dir = temp_config_dir();
        let forge = BuddySettings {
            dock: DockShow::Bar,
            color: [96, 140, 180],
            body_len: 220.0,
        };
        let hermes = BuddySettings {
            dock: DockShow::Head,
            color: [120, 168, 110],
            body_len: 90.0,
        };
        save_buddy_settings(&dir, "forge", &forge).unwrap();
        save_buddy_settings(&dir, "hermes", &hermes).unwrap();
        assert_eq!(load_buddy_settings(&dir, "forge"), forge);
        assert_eq!(load_buddy_settings(&dir, "hermes"), hermes);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_missing_file_yields_defaults() {
        let dir = temp_config_dir();
        assert_eq!(load_buddy_settings(&dir, "forge"), BuddySettings::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_garbage_file_yields_defaults() {
        let dir = temp_config_dir();
        let path = settings_path(&dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json at all").unwrap();
        assert_eq!(load_buddy_settings(&dir, "forge"), BuddySettings::default());

        fs::write(
            &path,
            r#"{"forge": {"dock": 99, "color": "red", "body_len": "tall"}}"#,
        )
        .unwrap();
        assert_eq!(load_buddy_settings(&dir, "forge"), BuddySettings::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_partial_entry_fills_defaults() {
        let dir = temp_config_dir();
        let path = settings_path(&dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"forge": {"color": [176, 116, 180]}}"#).unwrap();
        let loaded = load_buddy_settings(&dir, "forge");
        assert_eq!(loaded.color, [176, 116, 180]);
        assert_eq!(loaded.dock, DockShow::Both);
        assert_eq!(loaded.body_len, BODY_LEN_DEFAULT);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn body_len_reclamped_on_load() {
        let dir = temp_config_dir();
        let path = settings_path(&dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"forge": {"body_len": 500.0}}"#).unwrap();
        assert_eq!(load_buddy_settings(&dir, "forge").body_len, BODY_LEN_MAX);
        fs::write(&path, r#"{"forge": {"body_len": 10.0}}"#).unwrap();
        assert_eq!(load_buddy_settings(&dir, "forge").body_len, BODY_LEN_MIN);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_overrides_persisted_dock() {
        let _guard = env_test_guard();
        let saved_config = std::env::var("BB_CONFIG_DIR").ok();
        let saved_dock = std::env::var("BB_DOCK").ok();

        let dir = temp_config_dir();
        save_buddy_settings(
            &dir,
            "forge",
            &BuddySettings {
                dock: DockShow::Bar,
                color: CLAY_DEFAULT,
                body_len: BODY_LEN_DEFAULT,
            },
        )
        .unwrap();
        std::env::set_var("BB_CONFIG_DIR", &dir);
        std::env::set_var("BB_DOCK", "head");
        assert_eq!(resolve_startup("forge", &dir).dock, DockShow::Head);

        std::env::remove_var("BB_DOCK");
        assert_eq!(resolve_startup("forge", &dir).dock, DockShow::Bar);

        match saved_config {
            Some(v) => std::env::set_var("BB_CONFIG_DIR", v),
            None => std::env::remove_var("BB_CONFIG_DIR"),
        }
        match saved_dock {
            Some(v) => std::env::set_var("BB_DOCK", v),
            None => std::env::remove_var("BB_DOCK"),
        }
        let _ = fs::remove_dir_all(&dir);
    }
}