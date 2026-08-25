#![recursion_limit = "512"]
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
mod oauth;
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

#[tauri::command]
async fn oauth_connect(
    app: tauri::AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    provider: String,
) -> Result<Value, String> {
    let result = oauth::connect(&app, &state.http, &provider).await;
    if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        state.cache.clear(); // show the new account on the next tick
    }
    Ok(result)
}

#[tauri::command]
fn oauth_disconnect(state: State<'_, std::sync::Arc<AppState>>, provider: String) -> Value {
    oauth::clear_tokens(&provider);
    state.cache.clear();
    json!({ "ok": true })
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
        || providers::antigravity::available()
        || oauth::load_tokens("openai").is_some()
        || oauth::load_tokens("google").is_some();
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

/// Errors a user can actually hit — a failed sign-in, a rejected token
/// exchange — always go to a log file. Gating those behind a debug env var
/// means the one moment you need the reason is the moment it was not recorded.
pub fn log_error(text: &str) {
    let path = dirs::home_dir()
        .map(|h| h.join("Library/Logs/imburning-tauri.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/imburning-tauri.log"));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), text);
    }
    dev_log(text);
}

pub fn dev_log(text: &str) {
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
    let traced = window_is_user_sized(&window);
    dev_log(&format!(
        "  resize(h={:.0} force={} fitPreset={} userAction={}) userSized={}",
        height, force.unwrap_or(false), fit_preset.unwrap_or(false),
        user_action.unwrap_or(false), traced
    ));
    if force.unwrap_or(false) && !deliberate && window_is_user_sized(&window) {
        dev_log("    rejected: forced background refit on a user-sized window");
        return;
    }
    if !force.unwrap_or(false) && window_is_user_sized(&window) {
        dev_log("    rejected: unforced fit on a user-sized window");
        return;
    }
    // Damp the settle jitter. While a preset is active two renderer passes
    // disagree by a few pixels — _fitPresetHeight wants one height,
    // _fitWidePresetWithGraph another, and the latter calls through with no
    // delta guard of its own — so they alternate for about 700ms and the
    // window visibly buzzes. Anything under the renderer's own 12px shrink
    // tolerance is not a real layout change.
    if ACTIVE_PRESET.lock().map(|p| p.is_some()).unwrap_or(false) {
        if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
            let current = size.height as f64 / scale;
            if (height - current).abs() <= 12.0 {
                dev_log("  (damped: sub-12px preset jitter)");
                return;
            }
        }
    }
    if let Ok(size) = window.inner_size() {
        let scale = window.scale_factor().unwrap_or(1.0);
        let logical_width = size.width as f64 / scale;
        let floor = MIN_HEIGHT.lock().map(|h| *h).unwrap_or(180.0);
        let target = height.max(if COMPACT.load(std::sync::atomic::Ordering::Relaxed) { 90.0 } else { floor });
        if (target - height).abs() > 0.5 {
            dev_log(&format!("    NOTE: floor {:.0} raised requested {:.0} to {:.0}", floor, height, target));
        }
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

/// Whether the window's geometry belongs to the user (or to a preset) rather
/// than to the auto-fit loop. Mirrors Electron's windowIsUserSized, including
/// its 24px tolerance — 4px was tight enough to trip on scale-factor rounding.
fn window_is_user_sized(window: &tauri::Window) -> bool {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return false;
    };
    geometry_is_user_sized(size.width as f64 / scale, size.height as f64 / scale)
}

/// The predicate itself, over plain logical dimensions — the resize guard has a
/// Window and the resize listener a WebviewWindow, and both must agree.
fn geometry_is_user_sized(logical_w: f64, logical_h: f64) -> bool {
    // A named preset owns its geometry until switched off. Especially matters
    // for tall: on a shorter display its clamped height can resemble the last
    // auto-fit height, and the renderer would fight every vertical resize.
    if ACTIVE_PRESET.lock().map(|p| p.is_some()).unwrap_or(false) {
        return true;
    }
    let expected_w = EXPECTED_WIDTH.lock().map(|w| *w).unwrap_or(590.0);
    if (logical_w - expected_w).abs() > 24.0 {
        return true;
    }
    let expected_h = LAST_SET_HEIGHT.lock().map(|h| *h).unwrap_or(0.0);
    expected_h > 0.0 && (logical_h - expected_h).abs() > 24.0
}

/// Widen the columns in landscape. Guarded exactly as the Electron handler is,
/// because an unguarded version is actively harmful: it fired while compact
/// was on and produced a huge window full of compact rows, and it could grow
/// the window past the display.
#[tauri::command]
fn fit_landscape_width(window: tauri::Window, width: f64) {
    const WIDE_PRESET_WIDTH: f64 = 900.0;
    const MIN_WIDGET_WIDTH: f64 = 290.0;

    // Only the wide preset manages width, and only while it still owns it.
    if ACTIVE_PRESET.lock().map(|p| p.as_deref() != Some("wide")).unwrap_or(true) {
        return;
    }
    let Some(managed) = MANAGED_PRESET_WIDTH.lock().ok().and_then(|m| *m) else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let current = size.width as f64 / scale;

    // Drifted from what we set: the user has taken the width over, so stop
    // managing it rather than yanking it back under them.
    if (current - managed).abs() > PRESET_WIDTH_TOLERANCE {
        if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
            *slot = None;
        }
        return;
    }

    let requested = if width > 0.0 { width.round().max(MIN_WIDGET_WIDTH) } else { WIDE_PRESET_WIDTH };
    let available = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.size().width as f64 / m.scale_factor())
        .unwrap_or(requested);
    let target = requested.min(available);

    if (target - current).abs() < 2.0 {
        if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
            *slot = Some(target);
        }
        return;
    }
    dev_log(&format!("  fitLandscapeWidth {:.0} -> {:.0}", current, target));
    set_expected_width(target);
    let _ = window.set_size(tauri::LogicalSize::new(target, size.height as f64 / scale));
    if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
        *slot = Some(target);
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

/// Compact mode. Ported from the Electron handler rather than improvised,
/// because the improvised version got four things wrong at once — most
/// visibly, leaving compact kept whatever height was current, so coming back
/// from the wide preset produced a narrow window still at wide's height with
/// the Google section cut off.
///
/// Entering or leaving compact CLEARS any active preset: the two are different
/// answers to the same question, and letting a preset survive means its
/// geometry ownership fights whatever compact just set.
#[tauri::command]
fn set_compact_mode(window: tauri::Window, state: State<'_, std::sync::Arc<AppState>>, compact: bool) {
    const WIDGET_WIDTH: f64 = 590.0;
    const WIDGET_HEIGHT: f64 = 155.0;
    const MIN_WIDGET_WIDTH: f64 = 290.0;

    set_active_preset(None);
    if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
        *slot = None;
    }

    let width = if compact { 320.0 } else { WIDGET_WIDTH };
    set_expected_width(width);

    // A lower floor for compact, or the 180px portrait minimum clamps its
    // short window and leaves empty space. The renderer then fits the exact
    // pool count on top of this.
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(
        MIN_WIDGET_WIDTH,
        if compact { 80.0 } else { 180.0 },
    )));

    // Compact grows by one slim row per scoped weekly limit (e.g. Fable).
    let scoped = if compact { scoped_weekly_count(&state.store) } else { 0 };
    let height = if compact { 105.0 + (scoped as f64 * 26.0) } else { WIDGET_HEIGHT };

    dev_log(&format!("  compact({}) -> {:.0}x{:.0}", compact, width, height));
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if let Ok(mut slot) = LAST_SET_HEIGHT.lock() {
        *slot = height; // trackers line up, so auto-fit takes it from here
    }
    COMPACT.store(compact, std::sync::atomic::Ordering::Relaxed);
    let _ = window.emit("window-user-sized", false);
}

/// Scoped weekly pools (Fable and friends) in the last payload.
fn scoped_weekly_count(store: &Store) -> usize {
    store
        .get_or("latestUsageData", json!({}))
        .get("limits")
        .and_then(|l| l.as_array())
        .map(|limits| {
            limits
                .iter()
                .filter(|l| {
                    l.get("kind").and_then(|k| k.as_str()) == Some("weekly_scoped")
                        && l.get("percent").map(|p| !p.is_null()).unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
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

/// Wide / tall window presets.
///
/// A named preset OWNS the window geometry until the user turns it off. That
/// is what `window_is_user_sized` reports while one is active, and without it
/// the renderer's auto-fit immediately measures its content and resizes back —
/// the window visibly snaps to the preset and returns, which reads as a
/// flicker, and the wide preset never survives long enough to reflow into
/// columns (it just looks like a warped tall).
#[tauri::command]
fn apply_window_preset(window: tauri::Window, preset: String) {
    const WIDGET_WIDTH: f64 = 590.0;
    const WIDGET_HEIGHT: f64 = 155.0;
    const WIDE_PRESET_WIDTH: f64 = 900.0;

    let (mut width, mut height) = match preset.as_str() {
        "wide" => (WIDE_PRESET_WIDTH, 600.0),
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

    if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
        *slot = if preset == "wide" { Some(width) } else { None };
    }
    if preset == "reset" {
        set_active_preset(None);
        set_expected_width(WIDGET_WIDTH);
        if let Ok(mut slot) = LAST_SET_HEIGHT.lock() {
            *slot = height; // trackers match again, so auto-fit resumes
        }
    } else {
        set_active_preset(Some(preset.clone()));
        set_expected_width(width);
    }
    dev_log(&format!("  preset({}) -> {:.0}x{:.0}", preset, width, height));
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if preset != "reset" {
        settle_preset_height(&window);
    }
    // Announce the new ownership at once. The renderer keys `landscape` off
    // this, and waiting for the debounced resize event to say the same thing
    // is a race the layout should not depend on.
    let _ = window.emit("window-user-sized", preset != "reset");
}

/// Size a preset down to the content it actually has.
///
/// The preset height is deliberately generous — tall asks for 1150 — and the
/// renderer is supposed to measure the content and shrink to it. But that fit
/// runs inside requestAnimationFrame, and macOS throttles rAF to a standstill
/// whenever the window is not being rendered. When it does not fire the blind
/// preset height simply stays, which is the 362px of dead space under the
/// graph: window 1150, content 788.
///
/// So the measurement is driven from here instead. A host-initiated eval is
/// never throttled, and the reply is a single number.
fn settle_preset_height(window: &tauri::Window) {
    // eval lives on the WebviewWindow, not the Window.
    let Some(w) = window.app_handle().get_webview_window(window.label()) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        // Two passes: a preset switch triggers a large reflow, and the first
        // measurement can land mid-flight.
        for delay in [350u64, 800] {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            let (tx, rx) = std::sync::mpsc::channel();
            // mainContent's top IS the chrome height, and its childrens'
            // furthest bottom is the real content extent — the same
            // measurement _intrinsicMainContentHeight makes, minus the rAF.
            // Use the renderer's OWN measurement functions, not a
            // reimplementation of them. A hand-rolled version disagreed by
            // 15px (it missed the bottom bar), and the two then took turns
            // resizing the window to their own answer.
            let js = r#"(function(){
                try {
                    if (typeof _chromeHeight === 'function' && typeof _intrinsicMainContentHeight === 'function') {
                        return Math.ceil(_chromeHeight() + _intrinsicMainContentHeight() + 10);
                    }
                } catch (e) {}
                return 0;
            })()"#;
            if w.eval_with_callback(js, move |r| { let _ = tx.send(r); }).is_err() {
                return;
            }
            let Ok(raw) = rx.recv_timeout(std::time::Duration::from_secs(3)) else { return };
            let Ok(target) = raw.trim().parse::<f64>() else { return };
            if target < 120.0 {
                continue; // measured mid-reflow
            }
            let (Ok(size), Ok(scale)) = (w.inner_size(), w.scale_factor()) else { return };
            let current = size.height as f64 / scale;
            if (target - current).abs() <= 12.0 {
                continue;
            }
            dev_log(&format!("  settle: content {:.0} vs window {:.0}", target, current));
            if let Ok(mut slot) = LAST_SET_HEIGHT.lock() {
                *slot = target;
            }
            let _ = w.set_size(tauri::LogicalSize::new(size.width as f64 / scale, target));
        }
    });
}

/// The preset currently owning the geometry, if any.
static ACTIVE_PRESET: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// The width the backend intends the window to have.
static EXPECTED_WIDTH: std::sync::Mutex<f64> = std::sync::Mutex::new(590.0);
/// The width the WIDE preset is currently managing, if it still owns it. None
/// means nothing may adjust the width — which is the whole point: without this
/// gate the renderer's landscape fit fired in compact mode and in tall, giving
/// a huge window full of compact rows.
static MANAGED_PRESET_WIDTH: std::sync::Mutex<Option<f64>> = std::sync::Mutex::new(None);
/// How far the width may drift before the preset concludes the user has taken
/// over and stops managing it.
const PRESET_WIDTH_TOLERANCE: f64 = 12.0;

fn set_active_preset(preset: Option<String>) {
    let clearing = preset.is_none();
    if let Ok(mut slot) = ACTIVE_PRESET.lock() {
        *slot = preset;
    }
    // A preset's geometry claims live and die WITH it. Keeping them as
    // separate flags each caller has to remember is how the tall floor
    // survived into compact mode and blocked its resize — the same shape of
    // bug as every other one in this file's history.
    if clearing {
        if let Ok(mut slot) = MANAGED_PRESET_WIDTH.lock() {
            *slot = None;
        }
    }
}

fn set_expected_width(width: f64) {
    if let Ok(mut slot) = EXPECTED_WIDTH.lock() {
        *slot = width;
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
                        // MUST be the same predicate the resize guard uses.
                        // The renderer gates its whole responsive layer on this
                        // value — `landscape` is `_windowUserSized && w > h &&
                        // w >= 760` — so a second, subtly different definition
                        // here meant the renderer never learned a preset was
                        // active: wide never reflowed into columns and tall's
                        // content never adapted, so both were fitted straight
                        // back to the default height.
                        let user_sized = match (emitter.inner_size(), emitter.scale_factor()) {
                            (Ok(size), Ok(scale)) => geometry_is_user_sized(
                                size.width as f64 / scale,
                                size.height as f64 / scale,
                            ),
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
            // Geometry tracer for hand-testing. Logs every size change the
            // window actually undergoes, sampled from the HOST side. It has to
            // be run with the window VISIBLE: the renderer's auto-fit only runs
            // while the window is rendered, so a trace taken from an off-Space
            // window says nothing about the behaviour under investigation.
            if std::env::var("IMBURNING_DEV_TRACE").as_deref() == Ok("1") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    let Some(w) = handle.get_webview_window("main") else { return };
                    let start = std::time::Instant::now();
                    let mut last = String::new();
                    for _ in 0..1800 {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        if let (Ok(size), Ok(scale)) = (w.inner_size(), w.scale_factor()) {
                            let now = format!(
                                "{}x{}",
                                (size.width as f64 / scale).round() as i64,
                                (size.height as f64 / scale).round() as i64
                            );
                            if now != last {
                                dev_log(&format!("trace t={:>6}ms  {}", start.elapsed().as_millis(), now));
                                last = now;
                            }
                        }
                    }
                    dev_log("trace: finished");
                });
            }

            // Pull the rendered DOM from the host side rather than having the
            // page push it on a timer. A window on another Space is not
            // rendered, and WebKit throttles its timers to a standstill — so a
            // page-driven report simply never arrives, which looked exactly
            // like a broken frontend. A host-initiated eval still runs.
            if std::env::var("IMBURNING_DEV_REPORT").as_deref() == Ok("1") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    let Some(window) = handle.get_webview_window("main") else { return };
                    if let Ok(expr) = std::env::var("IMBURNING_DEV_EVAL") {
                        let _ = window.eval_with_callback(expr, |r| dev_log(&format!("eval: {}", r)));
                        // Always read back whatever the expression parked on
                        // window.__st once it has had time to settle. The expr
                        // can kick off an async change (e.g. click a preset and
                        // measure after the resize reflows), which the eval's
                        // own synchronous return value cannot capture.
                        let w = window.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                            let _ = w.eval_with_callback("window.__st||''", |r| {
                                dev_log(&format!("state: {}", r))
                            });
                        });
                        return;
                    }
                    let js = r#"JSON.stringify({
                        title: document.title,
                        google: [...document.querySelectorAll('#googleRows > *')].map(r => r.textContent.replace(/\s+/g,' ').trim()).filter(Boolean),
                        openai: [...document.querySelectorAll('#openaiRows > *')].map(r => r.textContent.replace(/\s+/g,' ').trim()).filter(Boolean),
                        anthropic: [...document.querySelectorAll('#scopedRows > *')].map(r => r.textContent.replace(/\s+/g,' ').trim()).filter(Boolean),
                        chipGoogle: (document.getElementById('chipGoogle')||{}).textContent || null,
                        planner: [...document.querySelectorAll('.planner-hint, .section-footer')].map(r => r.textContent.replace(/\s+/g,' ').trim()).filter(t => t.startsWith('Planner')),
                        errors: window.__shimErrors || []
                    })"#;
                    let _ = window.eval_with_callback(js, |result| {
                        dev_log(&format!("dom pull: {}", result));
                    });
                });
            }

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
            oauth_connect,
            oauth_disconnect,
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
