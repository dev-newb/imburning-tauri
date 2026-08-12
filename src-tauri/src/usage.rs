// Assembles the single usage document the renderer consumes, mirroring the
// Electron build's `fetch-usage-data` reply field for field.

use crate::providers::{antigravity, claude_code, codex, gemini, Limit, ProviderData};
use crate::store::Store;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Cache {
    entries: std::sync::Mutex<std::collections::HashMap<String, (Instant, Value)>>,
}

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

impl Cache {
    pub fn new() -> Self {
        Cache { entries: std::sync::Mutex::new(std::collections::HashMap::new()) }
    }

    fn get(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().ok()?;
        let (at, value) = entries.get(key)?;
        if at.elapsed() < CACHE_TTL {
            Some(value.clone())
        } else {
            None
        }
    }

    fn put(&self, key: &str, value: Value) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key.to_string(), (Instant::now(), value));
        }
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

/// Which Google surface feeds the section. "auto" prefers Antigravity when an
/// agy login exists (the only surface that meters agent usage) and otherwise
/// falls back to the classic Code Assist quota. Gemini is never removed as an
/// option — "gemini" forces it.
async fn google(client: &reqwest::Client, store: &Store) -> Option<ProviderData> {
    let source = store
        .get_or("settings.googleSource", json!("auto"))
        .as_str()
        .unwrap_or("auto")
        .to_string();
    let want_antigravity = source == "antigravity" || (source == "auto" && antigravity::available());
    if want_antigravity {
        if let Some(mut data) = antigravity::fetch(client).await {
            // The Claude/GPT-OSS pools Antigravity meters are real usage on the
            // same login, so they belong in the section — but they ship hidden,
            // because under a Google heading they surprise anyone who has never
            // opened Antigravity. The renderer hides each once, on first sight.
            let foreign = std::mem::take(&mut data.foreign);
            data.limits.extend(foreign.into_iter().map(|limit| Limit {
                default_hidden: true,
                ..limit
            }));
            return Some(data);
        }
        if source == "antigravity" {
            return None; // forced: do not silently fall back
        }
    }
    gemini::fetch(client).await
}

pub async fn fetch_all(client: &reqwest::Client, store: &Store, cache: &Cache, force: bool) -> Value {
    if !force {
        if let Some(cached) = cache.get("usage") {
            return cached;
        }
    }

    let show_google = store.get_or("settings.showGemini", json!(true)).as_bool().unwrap_or(true)
        || store.get_or("settings.showGeminiCli", json!(true)).as_bool().unwrap_or(true);
    let show_codex = store.get_or("settings.showCodex", json!(true)).as_bool().unwrap_or(true)
        || store.get_or("settings.showCodexCli", json!(true)).as_bool().unwrap_or(true);

    // Independent network calls — run them concurrently, not in sequence.
    let (anthropic, codex_data, google_data) = tokio::join!(
        async { if claude_code::available() { claude_code::fetch(client).await } else { None } },
        async { if show_codex { codex::fetch(client).await } else { None } },
        async { if show_google { google(client, store).await } else { None } },
    );

    let mut data = json!({
        "five_hour": Value::Null,
        "seven_day": Value::Null,
        "limits": [],
        "anthropic_source": "none",
        "claude_code_same_account": false,
    });

    if let Some(cc) = anthropic {
        data["five_hour"] = cc.get("five_hour").cloned().unwrap_or(Value::Null);
        data["seven_day"] = cc.get("seven_day").cloned().unwrap_or(Value::Null);
        data["limits"] = cc.get("limits").cloned().unwrap_or(json!([]));
        data["anthropic_source"] = json!("cli");
        data["claude_code_same_account"] = json!(true);
    }
    if let Some(cx) = codex_data {
        data["codex"] = serde_json::to_value(cx).unwrap_or(Value::Null);
    }
    if let Some(g) = google_data {
        data["gemini"] = serde_json::to_value(g).unwrap_or(Value::Null);
    }

    // History records the UNFILTERED document: the visibility toggles are a
    // display choice and must never change which series get recorded.
    crate::history::record(&data);
    store.set("latestUsageData", data.clone());
    cache.put("usage", data.clone());
    data
}
