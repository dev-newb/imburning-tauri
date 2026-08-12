// A JSON-file store shaped exactly like electron-store's.
//
// It keeps its OWN file rather than writing the Electron build's: both apps
// hold the whole document in memory and rewrite it wholesale, so running them
// at the same time would let one silently discard the other's settings and
// history. On first run the Electron config is COPIED in, so a user switching
// over keeps their settings, hidden rows and history — after that the two
// diverge, which is the honest behaviour for two independent apps.

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Store {
    path: PathBuf,
    data: Mutex<Value>,
}

impl Store {
    pub fn load() -> Self {
        let path = Self::config_path();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}));
        Store { path, data: Mutex::new(data) }
    }

    fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("imburning-tauri");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("config.json");
        if !path.exists() {
            // First run: inherit the Electron build's settings if it is
            // installed. A failed copy is not an error — it just means a fresh
            // start, which is what a first run looks like anyway.
            let electron = base.join("claude-usage-widget").join("config.json");
            if electron.exists() {
                let _ = fs::copy(&electron, &path);
            }
        }
        path
    }

    /// Dotted-path read: `get("settings.googleSource")`.
    pub fn get(&self, key: &str) -> Option<Value> {
        let data = self.data.lock().ok()?;
        let mut cur = &*data;
        for part in key.split('.') {
            cur = cur.get(part)?;
        }
        Some(cur.clone())
    }

    pub fn get_or(&self, key: &str, fallback: Value) -> Value {
        self.get(key).unwrap_or(fallback)
    }

    pub fn set(&self, key: &str, value: Value) {
        if let Ok(mut data) = self.data.lock() {
            let mut cur = &mut *data;
            let parts: Vec<&str> = key.split('.').collect();
            for part in &parts[..parts.len() - 1] {
                if !cur.get(*part).map(|v| v.is_object()).unwrap_or(false) {
                    cur[*part] = json!({});
                }
                cur = cur.get_mut(*part).unwrap();
            }
            cur[parts[parts.len() - 1]] = value;
        }
        self.flush();
    }

    fn flush(&self) {
        if let Ok(data) = self.data.lock() {
            if let Ok(text) = serde_json::to_string_pretty(&*data) {
                // Write through a temp file: a truncated config.json costs the
                // user every setting and their whole usage history.
                let tmp = self.path.with_extension("json.tmp");
                if fs::write(&tmp, text).is_ok() {
                    let _ = fs::rename(&tmp, &self.path);
                }
            }
        }
    }
}
