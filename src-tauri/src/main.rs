// I'm Burning! — Tauri build.
//
// The frontend is the Electron build's renderer, unmodified apart from a shim
// that maps window.electronAPI onto Tauri's invoke(). Every command below
// exists because that shim calls it; the names mirror the Electron IPC
// channels so the two builds stay diffable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod providers;
mod settings;
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
    settings::with_defaults(&state.store)
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
        let floor = MIN_HEIGHT.lock().map(|h| *h).unwrap_or(180.0);
        let target = height.max(if COMPACT.load(std::sync::atomic::Ordering::Relaxed) { 90.0 } else { floor });
        dev_log(&format!(
            "resize_window requested={:.0} applied={:.0} width={:.0}",
            height, target, logical_width
        ));
        // Record what WE asked for, so the resize listener can tell a
        // programmatic fit from the user dragging the frame.
        if let Ok(mut slot) = LAST_SET_HEIGHT.lock() {
            *slot = target;
        }
        let _ = window.set_size(tauri::LogicalSize::new(logical_width, target));
    }
}

/// The height the backend last set, used to distinguish a programmatic fit
/// from a hand-resize. Electron tracked the same thing as `_lastSetHeight`.
static LAST_SET_HEIGHT: std::sync::Mutex<f64> = std::sync::Mutex::new(0.0);

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

/// Compact mode needs a narrower window than the normal minimum allows, so the
/// floor moves with the mode — otherwise the renderer asks for compact
/// geometry and the OS clamps it back to the normal-mode minimum, leaving the
/// widget stuck at a size its compact layout was never drawn for.
#[tauri::command]
fn set_compact_mode(window: tauri::Window, compact: bool) {
    let (min_w, min_h, width) = if compact { (280.0, 90.0, 320.0) } else { (290.0, 180.0, 590.0) };
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(min_w, min_h)));
    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
        let _ = window.set_size(tauri::LogicalSize::new(width, size.height as f64 / scale));
    }
    COMPACT.store(compact, std::sync::atomic::Ordering::Relaxed);
}

static COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Minimum height the renderer asks for as its layout changes (landscape needs
/// more). Kept so resize_window's floor tracks the live layout instead of a
/// constant that is wrong half the time.
static MIN_HEIGHT: std::sync::Mutex<f64> = std::sync::Mutex::new(180.0);

#[tauri::command]
fn set_min_height(window: tauri::Window, height: f64) {
    let height = height.clamp(80.0, 2000.0);
    if let Ok(mut slot) = MIN_HEIGHT.lock() {
        *slot = height;
    }
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(290.0, height)));
}

/// Custom alert sounds live outside the bundle. Hand the renderer the bytes as
/// a data: URL rather than opening the CSP up to arbitrary file: reads — the
/// same reasoning as the Electron build.
#[tauri::command]
fn read_sound_file(path: String) -> Value {
    use base64::Engine;
    if path.is_empty() {
        return json!({ "ok": false, "error": "No file" });
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return json!({ "ok": false, "error": "Could not read file" });
    };
    // Refuse anything implausible for an alert sound rather than trying to
    // inline tens of megabytes into a data: URL.
    if bytes.len() > 25 * 1024 * 1024 {
        return json!({ "ok": false, "error": "File too large" });
    }
    let mime = match std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "audio/wav",
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    json!({ "ok": true, "dataUrl": format!("data:{};base64,{}", mime, encoded) })
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

            // The renderer applies its squeeze/responsive classes only when
            // told the window is user-sized, so without this event the whole
            // responsive layout is dead. Electron sent it from a debounced
            // resize handler; same here, debounced so a drag does not emit a
            // hundred times. "User-sized" means the window no longer matches
            // the height the renderer last asked for.
            if let Some(window) = app.get_webview_window("main") {
                let emitter = window.clone();
                let pending = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                window.on_window_event(move |event| {
                    if !matches!(event, tauri::WindowEvent::Resized(_)) {
                        return;
                    }
                    let seq = pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let emitter = emitter.clone();
                    let pending = pending.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                        // A newer resize landed while waiting — let it emit.
                        if pending.load(std::sync::atomic::Ordering::Relaxed) != seq {
                            return;
                        }
                        let user_sized = match (emitter.inner_size(), emitter.scale_factor()) {
                            (Ok(size), Ok(scale)) => {
                                let logical = size.height as f64 / scale;
                                let expected = LAST_SET_HEIGHT.lock().map(|h| *h).unwrap_or(0.0);
                                expected > 0.0 && (logical - expected).abs() > 4.0
                            }
                            _ => false,
                        };
                        let _ = emitter.emit("window-user-sized", user_sized);
                    });
                });
            }

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
            set_compact_mode,
            set_min_height,
            read_sound_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running I'm Burning!");
}
