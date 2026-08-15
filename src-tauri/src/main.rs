// I'm Burning! — Tauri build.
//
// The frontend is the Electron build's renderer, unmodified apart from a shim
// that maps window.electronAPI onto Tauri's invoke(). Every command below
// exists because that shim calls it; the names mirror the Electron IPC
// channels so the two builds stay diffable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analytics;
mod anthropic;
mod history;
mod providers;
mod settings;
mod store;
mod tray;
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
async fn fetch_usage_data(
    app: tauri::AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    force: Option<bool>,
) -> Result<Value, String> {
    let force = force.unwrap_or(false);
    let data = usage::fetch_all(&state.http, &state.store, &state.cache, force, Some(&app)).await;
    tray::sync(&app, &data, &state.store);
    emit_burn_alerts(&app, &state).await;
    Ok(data)
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
fn save_settings(window: tauri::Window, state: State<'_, std::sync::Arc<AppState>>, settings: Value) -> Value {
    // Settings with a window side effect have to be APPLIED here, not merely
    // stored: the config hard-codes alwaysOnTop, so without this the toggle
    // writes a value nothing reads and the widget stays pinned forever.
    if let Some(on_top) = settings.get("alwaysOnTop").and_then(|v| v.as_bool()) {
        let _ = window.set_always_on_top(on_top);
    }
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

#[tauri::command]
async fn anthropic_login(app: tauri::AppHandle, state: State<'_, std::sync::Arc<AppState>>) -> Result<Value, String> {
    let result = anthropic::login(&app).await;
    // Remember the chosen org: every later usage call is scoped to it.
    if result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        if let Some(id) = result.get("organizationId").and_then(|v| v.as_str()) {
            state.store.set("organizationId", json!(id));
        }
        if let Some(orgs) = result.get("organizations") {
            state.store.set("organizations", orgs.clone());
        }
        state.cache.clear(); // pick the new login up on the next tick
    }
    Ok(result)
}

#[tauri::command]
fn delete_credentials(state: State<'_, std::sync::Arc<AppState>>) -> Value {
    anthropic::delete_session_key();
    state.store.set("organizationId", Value::Null);
    state.store.set("organizations", json!([]));
    state.cache.clear();
    json!({ "success": true })
}

#[tauri::command]
fn set_organization(state: State<'_, std::sync::Arc<AppState>>, org_id: String) -> Value {
    state.store.set("organizationId", json!(org_id));
    state.cache.clear();
    json!({ "success": true })
}

/// Login STATE only — never a token. The field names are the renderer's
/// contract (`hasUsableCredentials` gates the whole UI on
/// cliFallbackAvailable / providerFallbackAvailable), so they match the
/// Electron reply exactly.
#[tauri::command]
fn get_credentials(state: State<'_, std::sync::Arc<AppState>>) -> Value {
    let claude_cli = providers::claude_code::available();
    let logged_in = anthropic::read_session_key().is_some();
    let external = providers::codex::available()
        || providers::gemini::has_credentials()
        || providers::antigravity::available();
    json!({
        "loggedIn": logged_in,
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

/// Surface any burn-spike alerts the detector raised during a fetch. Electron
/// fires these from its main process; here the fetch has no AppHandle, so the
/// callers that do drain the queue and emit. Notification AND webhook, because
/// the Electron build sends both.
async fn emit_burn_alerts(app: &tauri::AppHandle, state: &std::sync::Arc<AppState>) {
    use tauri_plugin_notification::NotificationExt;
    for alert in analytics::drain_alerts() {
        let _ = app
            .notification()
            .builder()
            .title("I'm Burning! — unusual token burn")
            .body(&alert.body)
            .show();
        post_webhook(state, "burn_spike", "Unusual token burn", &alert.body).await;
        let _ = app.emit("burn-alert", &alert.body);
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

/// The renderer distinguishes a background refit from a deliberate one. A
/// background refit must never collapse a window the user sized by hand;
/// a preset fit or a direct click (graph toggle, row hide) may. Dropping these
/// flags — as the first cut of the shim did — let every background refresh
/// yank a hand-sized window back to content height.
#[tauri::command]
fn resize_window(
    window: tauri::Window,
    height: f64,
    force: Option<bool>,
    fit_preset: Option<bool>,
    user_action: Option<bool>,
) {
    let deliberate = fit_preset.unwrap_or(false) || user_action.unwrap_or(false);
    if force.unwrap_or(false) && !deliberate && window_is_user_sized(&window) {
        return;
    }
    if !force.unwrap_or(false) && window_is_user_sized(&window) {
        return;
    }
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

/// True once the live height no longer matches what we last set — i.e. the
/// user dragged the frame. Zero means we have not sized it yet, so nothing has
/// been overridden.
fn window_is_user_sized(window: &tauri::Window) -> bool {
    let expected = LAST_SET_HEIGHT.lock().map(|h| *h).unwrap_or(0.0);
    if expected <= 0.0 {
        return false;
    }
    match (window.inner_size(), window.scale_factor()) {
        (Ok(size), Ok(scale)) => ((size.height as f64 / scale) - expected).abs() > 4.0,
        _ => false,
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

/// Wide / tall window presets. Sizes and the clamp-to-work-area behaviour
/// mirror the Electron handler; "reset" returns to the auto-sized widget and
/// clears the tracker so the renderer resumes auto-height.
#[tauri::command]
fn apply_window_preset(window: tauri::Window, preset: String) {
    const WIDGET_WIDTH: f64 = 590.0;
    const WIDGET_HEIGHT: f64 = 155.0;
    const WIDE_WIDTH: f64 = 1000.0;

    let (mut width, mut height) = match preset.as_str() {
        "wide" => (WIDE_WIDTH, 600.0),
        "tall" => (WIDGET_WIDTH, 1150.0),
        "reset" => (WIDGET_WIDTH, WIDGET_HEIGHT),
        _ => return,
    };

    // Clamp to the monitor's work area so a tall preset cannot run off-screen.
    if let Ok(Some(monitor)) = window.current_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        width = width.min(size.width as f64 / scale);
        height = height.min(size.height as f64 / scale);
    }
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if let Ok(mut slot) = LAST_SET_HEIGHT.lock() {
        // On reset the tracker matches the window again, so windowIsUserSized
        // reads false and auto-height resumes; a preset deliberately leaves a
        // mismatch, which is what makes the renderer's reflow engage.
        *slot = if preset == "reset" { height } else { 0.0 };
    }
}

/// Fire the user's alert webhook. Refuses anything but https, or http to
/// loopback — an alert must not be a way to send usage data in the clear.
/// ntfy.sh takes a plain-text body with the title in a header; everything else
/// gets JSON, same as the Electron build.
#[tauri::command]
async fn send_alert_webhook(
    state: State<'_, std::sync::Arc<AppState>>,
    event: String,
    title: String,
    message: String,
) -> Result<(), String> {
    post_webhook(&state, &event, &title, &message).await;
    Ok(())
}

async fn post_webhook(state: &std::sync::Arc<AppState>, event: &str, title: &str, message: &str) {
    let webhook = state.store.get_or("settings.webhook", json!({}));
    if !webhook.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }
    let Some(url) = webhook.get("url").and_then(|v| v.as_str()).filter(|u| !u.is_empty()) else {
        return;
    };
    let Ok(parsed) = reqwest::Url::parse(url) else { return };
    let host = parsed.host_str().unwrap_or("");
    let is_local = host == "localhost" || host == "127.0.0.1";
    if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && is_local)) {
        return;
    }

    let request = if host.to_lowercase().contains("ntfy") {
        state
            .http
            .post(parsed)
            .header("Content-Type", "text/plain")
            .header("Title", title)
            .header("Tags", "chart_with_upwards_trend")
            .body(message.to_string())
    } else {
        state.http.post(parsed).json(&json!({
            "event": event,
            "title": title,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "source": "claude-usage-widget",
        }))
    };
    let _ = request.send().await; // best effort: an alert must never block the UI
}

// ---- detachable graph window ----------------------------------------------

/// The chart in its own window, so it can sit beside the widget instead of
/// inside it. Bounds persist; "always on top" is its own setting, separate
/// from the main window's.
#[tauri::command]
fn open_graph_window(app: tauri::AppHandle, state: State<'_, std::sync::Arc<AppState>>) {
    if let Some(existing) = app.get_webview_window("graph") {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let saved = state.store.get_or("graphWindowBounds", json!({}));
    let num = |k: &str, d: f64| saved.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
    let on_top = state
        .store
        .get_or("settings.graphAlwaysOnTop", json!(true))
        .as_bool()
        .unwrap_or(true);

    let mut builder = tauri::WebviewWindowBuilder::new(
        &app,
        "graph",
        tauri::WebviewUrl::App("graph.html".into()),
    )
    .title("I'm Burning! — Graph")
    .inner_size(num("width", 660.0), num("height", 400.0))
    .min_inner_size(360.0, 240.0)
    .background_color(tauri::window::Color(0x16, 0x16, 0x1e, 0xff))
    .always_on_top(on_top);
    if saved.get("x").is_some() && saved.get("y").is_some() {
        builder = builder.position(num("x", 0.0), num("y", 0.0));
    }
    let _ = builder.build();
}

#[tauri::command]
fn close_graph_window(app: tauri::AppHandle, state: State<'_, std::sync::Arc<AppState>>) {
    if let Some(window) = app.get_webview_window("graph") {
        // Remember where the user left it before it goes.
        if let (Ok(pos), Ok(size), Ok(scale)) =
            (window.outer_position(), window.inner_size(), window.scale_factor())
        {
            state.store.set(
                "graphWindowBounds",
                json!({
                    "x": pos.x as f64 / scale,
                    "y": pos.y as f64 / scale,
                    "width": size.width as f64 / scale,
                    "height": size.height as f64 / scale
                }),
            );
        }
        let _ = window.close();
        let _ = app.emit("graph-window-closed", ());
    }
}

#[tauri::command]
fn is_graph_window_open(app: tauri::AppHandle) -> bool {
    app.get_webview_window("graph").is_some()
}

#[tauri::command]
fn graph_set_always_on_top(app: tauri::AppHandle, state: State<'_, std::sync::Arc<AppState>>, flag: bool) {
    state.store.set("settings.graphAlwaysOnTop", json!(flag));
    if let Some(window) = app.get_webview_window("graph") {
        let _ = window.set_always_on_top(flag);
    }
}

#[tauri::command]
fn graph_get_always_on_top(state: State<'_, std::sync::Arc<AppState>>) -> bool {
    state
        .store
        .get_or("settings.graphAlwaysOnTop", json!(true))
        .as_bool()
        .unwrap_or(true)
}

// ---- history export --------------------------------------------------------

/// CSV columns are the UNION of keys across every record, because the shape
/// grows over time — a pool the account gained last week exists on later rows
/// only, and keying off the first row would silently drop it.
fn history_to_csv(history: &[Value]) -> String {
    let mut rows: Vec<serde_json::Map<String, Value>> = vec![];
    for entry in history {
        let Some(obj) = entry.as_object() else { continue };
        let mut row = serde_json::Map::new();
        for (k, v) in obj {
            if k == "scoped" {
                if let Some(scoped) = v.as_object() {
                    for (sk, sv) in scoped {
                        row.insert(format!("scoped_{}", sk), sv.clone());
                    }
                }
            } else {
                row.insert(k.clone(), v.clone());
            }
        }
        if let Some(ts) = obj.get("timestamp").and_then(|t| t.as_i64()) {
            if let Some(d) = chrono::DateTime::from_timestamp_millis(ts) {
                row.insert("timestamp_iso".into(), json!(d.to_rfc3339()));
            }
        }
        rows.push(row);
    }
    let mut cols: Vec<String> = vec![];
    for row in &rows {
        for k in row.keys() {
            if !cols.contains(k) {
                cols.push(k.clone());
            }
        }
    }
    let esc = |v: &Value| -> String {
        let s = match v {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    };
    let mut out = vec![cols.join(",")];
    for row in &rows {
        out.push(
            cols.iter()
                .map(|c| esc(row.get(c).unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    out.join("\n")
}

#[tauri::command]
async fn export_history(app: tauri::AppHandle, format: String) -> Value {
    use tauri_plugin_dialog::DialogExt;
    let history = history::read();
    if history.is_empty() {
        return json!({ "ok": false, "error": "No usage history recorded yet." });
    }
    let stamp = history
        .last()
        .and_then(|e| e.get("timestamp"))
        .and_then(|t| t.as_i64())
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "export".into());
    let json_format = format == "json";
    let ext = if json_format { "json" } else { "csv" };

    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title("Export I'm Burning! usage history")
        .set_file_name(&format!("burnwatch-usage-{}.{}", stamp, ext))
        .add_filter(if json_format { "JSON" } else { "CSV" }, &[ext])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Ok(Some(path)) = rx.recv() else {
        return json!({ "ok": false, "canceled": true });
    };
    let Some(path) = path.into_path().ok() else {
        return json!({ "ok": false, "error": "Unusable path" });
    };

    let body = if json_format {
        serde_json::to_string_pretty(&history).unwrap_or_default()
    } else {
        history_to_csv(&history)
    };
    match std::fs::write(&path, body) {
        Ok(_) => json!({ "ok": true, "path": path.to_string_lossy(), "count": history.len() }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
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
            // Verification runs must never take focus from the user. As an
            // Accessory app the process cannot become active, so neither the
            // main window nor the graph window can steal the front — which is
            // the whole point of the headless capture path. Opt-in via env so
            // the real app still behaves like a normal app.
            #[cfg(target_os = "macos")]
            if std::env::var("IMBURNING_NO_FOCUS").as_deref() == Ok("1") {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

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

            // Apply the stored window settings once at startup: the config's
            // alwaysOnTop is only the initial value, and the user's choice
            // lives in the settings store.
            if let Some(window) = app.get_webview_window("main") {
                let on_top = state
                    .store
                    .get_or("settings.alwaysOnTop", json!(true))
                    .as_bool()
                    .unwrap_or(true);
                let _ = window.set_always_on_top(on_top);
            }

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

            // Dev probe: exercise the Cloudflare-passing fetch path without a
            // login window. A 401 here is a PASS — it proves the webview
            // reached the real API and got a real status, rather than a
            // challenge page or a timeout.
            // Its own flag: this reaches out to claude.ai twice, which should
            // not happen on every ordinary dev launch.
            if std::env::var("IMBURNING_PROBE").as_deref() == Ok("1") {
                let probe = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                    // An unauthenticated endpoint first: it distinguishes "we
                    // passed Cloudflare and the API answered" from "Cloudflare
                    // answered instead", which a bare 403 cannot.
                    let anon =
                        anthropic::fetch_via_webview(&probe, "https://claude.ai/robots.txt").await;
                    dev_log(&format!(
                        "claude probe (anon): {}",
                        match &anon {
                            Ok(v) => format!("OK {}", v),
                            Err(e) => e.clone(),
                        }
                    ));
                    let result =
                        anthropic::fetch_via_webview(&probe, "https://claude.ai/api/organizations").await;
                    dev_log(&format!(
                        "claude probe (api): {}",
                        match &result {
                            Ok(v) => format!("OK {} bytes", v.to_string().len()),
                            Err(e) => e.clone(),
                        }
                    ));
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
                    let data = usage::fetch_all(&state.http, &state.store, &state.cache, true, Some(&handle)).await;
                    tray::sync(&handle, &data, &state.store);
                    emit_burn_alerts(&handle, &state).await;
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
            anthropic_login,
            delete_credentials,
            set_organization,
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
            apply_window_preset,
            send_alert_webhook,
            open_graph_window,
            close_graph_window,
            is_graph_window_open,
            graph_set_always_on_top,
            graph_get_always_on_top,
            export_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running I'm Burning!");
}
