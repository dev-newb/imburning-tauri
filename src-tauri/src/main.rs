// I'm Burning! — Tauri build.
//
// The frontend is the Electron build's renderer, unmodified apart from a shim
// that maps window.electronAPI onto Tauri's invoke(). Every command below
// exists because that shim calls it; the names mirror the Electron IPC
// channels so the two builds stay diffable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod providers;
mod store;
mod usage;

use serde_json::{json, Value};
use store::Store;
use tauri::{Emitter, Manager, State};
use usage::Cache;

struct AppState {
    store: Store,
    cache: Cache,
    http: reqwest::Client,
}

#[tauri::command]
async fn fetch_usage_data(state: State<'_, std::sync::Arc<AppState>>, force: Option<bool>) -> Result<Value, String> {
    let force = force.unwrap_or(false);
    Ok(usage::fetch_all(&state.http, &state.store, &state.cache, force).await)
}

#[tauri::command]
fn get_latest_usage(state: State<'_, std::sync::Arc<AppState>>) -> Value {
    state.store.get_or("latestUsageData", Value::Null)
}

#[tauri::command]
fn get_settings(state: State<'_, std::sync::Arc<AppState>>) -> Value {
    state.store.get_or("settings", json!({}))
}

#[tauri::command]
fn save_settings(state: State<'_, std::sync::Arc<AppState>>, settings: Value) -> Value {
    // Changing the Google source invalidates the cached fetch: the user
    // expects the newly chosen surface on the next tick, not in five minutes.
    let previous = state
        .store
        .get("settings.googleSource")
        .and_then(|v| v.as_str().map(String::from));
    let next = settings.get("googleSource").and_then(|v| v.as_str()).map(String::from);
    if previous != next {
        state.cache.clear();
    }
    state.store.set("settings", settings.clone());
    settings
}

/// Login STATE only — never a token. The field names are the renderer's
/// contract (`hasUsableCredentials` gates the whole UI on
/// cliFallbackAvailable / providerFallbackAvailable), so they match the
/// Electron reply exactly.
#[tauri::command]
fn get_credentials(state: State<'_, std::sync::Arc<AppState>>) -> Value {
    let claude_cli = providers::claude_code::available();
    let external = providers::codex::available()
        || providers::gemini::has_credentials()
        || providers::antigravity::available();
    json!({
        // This build has no widget-owned claude.ai login yet: everything runs
        // off local CLI logins, which is the "via CLI login" path.
        "loggedIn": false,
        "organizationId": state.store.get_or("organizationId", Value::Null),
        "organizations": state.store.get_or("organizations", json!([])),
        "cliFallbackAvailable": claude_cli,
        "localProviderCredentialsAvailable": claude_cli || external,
        "providerFallbackAvailable": external,
        "encryptionAvailable": false,
    })
}

#[tauri::command]
fn get_usage_history() -> Value {
    Value::Array(history::read())
}

#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Lets the frontend report what it actually rendered, for verifying a build
/// without a remote-debugging port (WKWebView has none). Inert unless
/// IMBURNING_DEV_REPORT=1 is set in the environment.
#[tauri::command]
fn dev_report(text: String) {
    dev_log(&text);
}

fn dev_log(text: &str) {
    if std::env::var("IMBURNING_DEV_REPORT").as_deref() != Ok("1") {
        return;
    }
    let path = std::env::temp_dir().join("imburning-dev-report.txt");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", text);
    }
}

// ---- window controls -------------------------------------------------------

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window(window: tauri::Window) {
    let _ = window.close();
}

#[tauri::command]
fn resize_window(window: tauri::Window, height: f64) {
    if let Ok(size) = window.inner_size() {
        let scale = window.scale_factor().unwrap_or(1.0);
        let logical_width = size.width as f64 / scale;
        let target = height.max(180.0);
        dev_log(&format!(
            "resize_window requested={:.0} applied={:.0} width={:.0}",
            height, target, logical_width
        ));
        let _ = window.set_size(tauri::LogicalSize::new(logical_width, target));
    }
}

#[tauri::command]
fn fit_landscape_width(window: tauri::Window, width: f64) {
    if width <= 0.0 {
        return;
    }
    if let Ok(size) = window.inner_size() {
        let scale = window.scale_factor().unwrap_or(1.0);
        let _ = window.set_size(tauri::LogicalSize::new(width, size.height as f64 / scale));
    }
}

#[tauri::command]
fn get_window_position(window: tauri::Window) -> Value {
    match (window.outer_position(), window.inner_size()) {
        (Ok(pos), Ok(size)) => {
            let scale = window.scale_factor().unwrap_or(1.0);
            json!({
                "x": pos.x as f64 / scale,
                "y": pos.y as f64 / scale,
                "width": size.width as f64 / scale,
                "height": size.height as f64 / scale
            })
        }
        _ => Value::Null,
    }
}

#[tauri::command]
fn set_window_position(window: tauri::Window, position: Value) {
    let (Some(x), Some(y)) = (
        position.get("x").and_then(|v| v.as_f64()),
        position.get("y").and_then(|v| v.as_f64()),
    ) else {
        return;
    };
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
}

#[tauri::command]
fn set_always_on_top(window: tauri::Window, flag: bool) {
    let _ = window.set_always_on_top(flag);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Held as an Arc, not just managed state: the refresh loop below
            // needs to keep it across an await, and a borrowed State<'_, _>
            // is not Send.
            history::seed_from_electron();
            let state = std::sync::Arc::new(AppState {
                store: Store::load(),
                cache: Cache::new(),
                http: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(16))
                    .build()
                    .expect("http client"),
            });
            app.manage(state.clone());

            // Poll on the interval the user chose, and tell the renderer to
            // repaint — the same cadence the Electron build's main process ran.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let minutes = state
                        .store
                        .get_or("settings.refreshInterval", json!(5))
                        .as_u64()
                        .unwrap_or(5)
                        .clamp(1, 60);
                    tokio::time::sleep(std::time::Duration::from_secs(minutes * 60)).await;
                    let _ = usage::fetch_all(&state.http, &state.store, &state.cache, true).await;
                    let _ = handle.emit("usage-updated", ());
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fetch_usage_data,
            get_latest_usage,
            get_settings,
            save_settings,
            get_credentials,
            get_usage_history,
            get_app_version,
            dev_report,
            minimize_window,
            close_window,
            resize_window,
            fit_landscape_width,
            get_window_position,
            set_window_position,
            set_always_on_top,
        ])
        .run(tauri::generate_context!())
        .expect("error while running I'm Burning!");
}
