// Assembles the single usage document the renderer consumes, mirroring the
// Electron build's `fetch-usage-data` reply field for field.

use crate::providers::{antigravity, claude_code, codex, gemini, ProviderData};
use crate::store::Store;
use tauri::Emitter;
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

/// CLI-account adoption (consent). CLI-borrowed credentials never feed the
/// stats until adopted; a fresh install detects them and offers instead. An
/// install that already showed CLI-derived data is grandfathered as adopted.
/// Mirrors the Electron build's cliAdoptionState().
fn cli_adopted(store: &Store) -> (bool, bool, bool) {
    if let Some(v) = store.get("settings.cliAdopted") {
        if v.is_object() {
            let flag = |k: &str| v.get(k).and_then(|b| b.as_bool()).unwrap_or(false);
            return (flag("anthropic"), flag("openai"), flag("google"));
        }
    }
    let existing = store.get("latestUsageData").is_some();
    store.set(
        "settings.cliAdopted",
        json!({ "anthropic": existing, "openai": existing, "google": existing }),
    );
    (existing, existing, existing)
}

/// Detected-but-unadopted CLI logins, for the renderer's offer chips.
/// Local reads only — nothing an unadopted account owns goes on the wire.
fn detect_cli_offers(adopted: (bool, bool, bool)) -> Value {
    let mut offers = serde_json::Map::new();
    if !adopted.0 && claude_code::available() {
        offers.insert("anthropic".into(), json!({ "email": Value::Null, "label": "Claude Code CLI login" }));
    }
    if !adopted.1 {
        if let Some(email) = codex::cli_offer_email() {
            offers.insert("openai".into(), json!({ "email": email, "label": "codex CLI login" }));
        }
    }
    if !adopted.2 {
        let gem = gemini::cli_offer_email();
        if gem.is_some() || antigravity::available() {
            offers.insert("google".into(), json!({ "email": gem.flatten(), "label": "gemini CLI login" }));
        }
    }
    Value::Object(offers)
}

/// Which Google surface feeds the section. "auto" prefers Antigravity when an
/// agy login exists (the only surface that meters agent usage) and otherwise
/// falls back to the classic Code Assist quota. Gemini is never removed as an
/// option — "gemini" forces it.
async fn google(client: &reqwest::Client, store: &Store, cli_allowed: bool) -> Option<ProviderData> {
    let source = store
        .get_or("settings.googleSource", json!("auto"))
        .as_str()
        .unwrap_or("auto")
        .to_string();
    // Antigravity rides the agy CLI's keychain credentials wholesale, so the
    // whole surface is gated on Google CLI adoption.
    let want_antigravity = cli_allowed
        && (source == "antigravity" || (source == "auto" && antigravity::available()));
    if want_antigravity {
        if let Some(data) = antigravity::fetch(client, store).await {
            return Some(data);
        }
        if source == "antigravity" {
            return None; // forced: do not silently fall back
        }
    }
    gemini::fetch(client, cli_allowed).await
}

/// `app` is optional so the fetch can run before the UI exists; without it the
/// claude.ai path is skipped, since that traffic has to go through a webview.
pub async fn fetch_all(
    client: &reqwest::Client,
    store: &Store,
    cache: &Cache,
    force: bool,
    app: Option<&tauri::AppHandle>,
) -> Value {
    if !force {
        if let Some(cached) = cache.get("usage") {
            return cached;
        }
    }

    let show_google = store.get_or("settings.showGemini", json!(true)).as_bool().unwrap_or(true)
        || store.get_or("settings.showGeminiCli", json!(true)).as_bool().unwrap_or(true);
    let show_codex = store.get_or("settings.showCodex", json!(true)).as_bool().unwrap_or(true)
        || store.get_or("settings.showCodexCli", json!(true)).as_bool().unwrap_or(true);

    let adopted = cli_adopted(store);
    // Independent network calls — run them concurrently, not in sequence.
    let (anthropic, codex_data, google_data) = tokio::join!(
        async { if adopted.0 && claude_code::available() { claude_code::fetch(client).await } else { None } },
        async { if show_codex { codex::fetch(client, adopted.1).await } else { None } },
        async { if show_google { google(client, store, adopted.2).await } else { None } },
    );

    let mut data = json!({
        "five_hour": Value::Null,
        "seven_day": Value::Null,
        "limits": [],
        "anthropic_source": "none",
        "claude_code_same_account": false,
    });

    // claude.ai (the widget's own login) takes precedence over the CLI
    // fallback, exactly as in Electron: it is the richer source, carrying
    // extra-usage and the scoped weekly pools.
    let mut web_usage: Option<Value> = None;
    if let (Some(app), Some(org)) = (
        app,
        store.get("organizationId").and_then(|v| v.as_str().map(String::from)),
    ) {
        // NOT gated on the stored session key. The request rides the webview's
        // cookie jar — the key is only a cached answer to "are we logged in",
        // and gating on it meant one transient failure that cleared the cache
        // permanently disabled a working login.
        {
            match crate::anthropic::fetch_usage(app, &org).await {
                Ok(v) => {
                    // A success is also the best moment to refresh the cache.
                    crate::anthropic::remember_session(app);
                    web_usage = Some(v)
                }
                Err(e) => {
                    // Every failure gets recorded. Swallowing the non-auth ones
                    // meant a broken fetch was indistinguishable from having no
                    // login at all — the section just read "none".
                    crate::log_error(&format!("anthropic usage fetch: {}", e));
                    // A 401/403 from ONE endpoint does not prove the session is
                    // dead — the usage endpoint can refuse for reasons of its
                    // own. Deleting the key on that basis threw away a working
                    // login and left the section permanently empty. Ask the
                    // session probe instead, and only then give up on it.
                    if e.starts_with("AuthFailure")
                        && !crate::anthropic::session_is_valid(app).await
                    {
                        crate::log_error("anthropic: session confirmed dead, clearing");
                        crate::anthropic::delete_session_key();
                        let _ = app.emit("session-expired", ());
                    }
                }
            }
        }
    }

    if let Some(web) = web_usage {
        for key in ["five_hour", "seven_day", "limits", "extra_usage"] {
            if let Some(v) = web.get(key) {
                data[key] = v.clone();
            }
        }
        data["anthropic_source"] = json!("web");
        data["claude_code_same_account"] = json!(true);
        if let Some(app) = app {
            if let Some(email) = crate::anthropic::account_email(app).await {
                data["anthropic_email"] = json!(email);
            }
        }
    } else if let Some(cc) = anthropic {
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

    data["offers"] = detect_cli_offers(adopted);

    // History records the UNFILTERED document: the visibility toggles are a
    // display choice and must never change which series get recorded.
    crate::history::record(&data);

    // Analytics run AFTER the sample is recorded, so this refresh's own figure
    // is part of the series they read — the Electron build orders it the same
    // way, and a forecast that ignores the newest point lags by one interval.
    let history = crate::history::read();
    data["forecasts"] = crate::analytics::compute_forecasts(&history, store);
    data["sessionPlans"] = crate::analytics::compute_session_plans(&history, store);
    data["frozenProviders"] = crate::analytics::compute_frozen_providers(&data, &history);
    crate::analytics::check_burn_anomalies(&history, store);
    data["burningSeries"] = crate::analytics::burning_series_map();
    store.set("latestUsageData", data.clone());
    cache.put("usage", data.clone());
    data
}
