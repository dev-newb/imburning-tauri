// claude.ai login and API access.
//
// Everything here goes through a WEBVIEW rather than an HTTP client, and that
// is not an accident: Cloudflare blocks plain client fingerprints on
// claude.ai, while a real page context riding the session cookies passes. The
// Electron build learned this the same way and parks a hidden BrowserWindow
// for exactly this reason.
//
// One hidden webview is parked on a cheap same-origin page and reused for
// in-page fetch() calls. A fresh webview per request would mean a whole
// renderer process per endpoint, several times per refresh.
//
// The session cookie is never injected by hand: the login window and the
// fetch webview share one WKWebsiteDataStore, so signing in leaves the cookie
// where later fetches will find it. Tauri has no cookie-set API on macOS
// anyway, and relying on the shared jar means the key itself never has to be
// handled outside the keychain.

use serde_json::{json, Value};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const ANCHOR_URL: &str = "https://claude.ai/robots.txt";
const LOGIN_URL: &str = "https://claude.ai/login";
const FETCH_WEBVIEW: &str = "cf-fetch";
const LOGIN_WEBVIEW: &str = "claude-login";

/// Signatures that mean Cloudflare answered instead of the API.
const BLOCKED: [(&str, &str); 3] = [
    ("Just a moment", "CloudflareBlocked"),
    ("Enable JavaScript and cookies to continue", "CloudflareChallenge"),
    ("<html", "UnexpectedHTML"),
];

/// The parked fetch webview, created on demand.
async fn ensure_fetch_webview(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(FETCH_WEBVIEW) {
        return Ok(existing);
    }
    let window = WebviewWindowBuilder::new(app, FETCH_WEBVIEW, WebviewUrl::External(
        ANCHOR_URL.parse().map_err(|_| "bad anchor url".to_string())?,
    ))
    .title("")
    .inner_size(800.0, 600.0)
    .visible(false) // never shown; it exists only to hold a page context
    .skip_taskbar(true)
    .build()
    .map_err(|e| format!("fetch webview: {}", e))?;

    // Give the anchor page a moment to load before the first fetch runs in it.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    Ok(window)
}

/// Discard the parked webview. A Cloudflare challenge means the page lost its
/// clearance, and a wedged webview is worthless — the next call rebuilds.
fn destroy_fetch_webview(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(FETCH_WEBVIEW) {
        let _ = window.close();
    }
}

/// Fetch JSON from claude.ai inside the parked page's context.
///
/// Two evals rather than one, because WKWebView's evaluateJavaScript returns
/// as soon as the expression produces a value — and an async function produces
/// a *pending Promise*, so the callback fires with nothing before the fetch has
/// even left. The first eval starts the request and parks the outcome on the
/// page; the second reads it with a plain synchronous expression, which is the
/// only kind this API can actually deliver.
pub async fn fetch_via_webview(app: &AppHandle, url: &str) -> Result<Value, String> {
    let window = ensure_fetch_webview(app).await?;
    let slot = format!("__imb_{}", now_tag());

    let kickoff = format!(
        r#"(function() {{
             window[{slot}] = null;
             fetch({url}, {{ credentials: 'include', headers: {{ 'Accept': 'application/json' }} }})
               .then(function(res) {{
                 return res.text().then(function(body) {{
                   window[{slot}] = JSON.stringify({{ status: res.status, bodyText: body }});
                 }});
               }})
               .catch(function(e) {{
                 window[{slot}] = JSON.stringify({{ status: 0, bodyText: 'FetchError: ' + e.message }});
               }});
             return true;
           }})()"#,
        slot = serde_json::to_string(&slot).unwrap_or_default(),
        url = serde_json::to_string(url).unwrap_or_default()
    );
    window.eval(kickoff).map_err(|e| format!("eval: {}", e))?;

    // Poll the parked slot. 30s matches the Electron build's request timeout.
    let deadline = Instant::now() + Duration::from_secs(30);
    let payload = loop {
        if Instant::now() > deadline {
            destroy_fetch_webview(app);
            return Err("Request timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;

        let (tx, rx) = mpsc::channel();
        let read = format!("window[{}]", serde_json::to_string(&slot).unwrap_or_default());
        if window
            .eval_with_callback(read, move |result| {
                let _ = tx.send(result);
            })
            .is_err()
        {
            continue;
        }
        let Ok(raw) = rx.recv_timeout(Duration::from_secs(5)) else { continue };
        // The callback JSON-encodes the value, so a JS string arrives quoted
        // and a JS null arrives as the four characters "null".
        if raw.trim().is_empty() || raw.trim() == "null" {
            continue;
        }
        match serde_json::from_str::<String>(&raw) {
            Ok(inner) => break inner,
            Err(_) => break raw,
        }
    };

    // Tidy up so a long-lived page does not accumulate slots.
    let _ = window.eval(format!(
        "delete window[{}]",
        serde_json::to_string(&slot).unwrap_or_default()
    ));

    let parsed: Value =
        serde_json::from_str(&payload).map_err(|e| format!("callback shape: {}", e))?;
    let status = parsed.get("status").and_then(|s| s.as_i64()).unwrap_or(0);
    let body = parsed.get("bodyText").and_then(|b| b.as_str()).unwrap_or("");

    // An explicit auth failure must be distinguishable from a shape problem,
    // so a dead session can prompt re-login instead of looking like a bug.
    if status == 401 || status == 403 {
        return Err(format!("AuthFailure: HTTP {}", status));
    }
    for (pattern, name) in BLOCKED {
        if body.contains(pattern) {
            if name.starts_with("Cloudflare") {
                destroy_fetch_webview(app);
            }
            return Err(format!("{}: {}", name, &body[..body.len().min(200)]));
        }
    }
    serde_json::from_str(body).map_err(|_| format!("InvalidJSON: {}", &body[..body.len().min(200)]))
}

/// A per-request slot name; two fetches in flight must not read each other's
/// result. Not security-sensitive, just uniqueness.
fn now_tag() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}_{}",
        chrono::Utc::now().timestamp_millis(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

// ---- login -----------------------------------------------------------------

fn session_key_from(window: &tauri::WebviewWindow) -> Option<String> {
    let url = "https://claude.ai".parse().ok()?;
    let cookies = window.cookies_for_url(url).ok()?;
    cookies
        .into_iter()
        .find(|c| c.name() == "sessionKey" && !c.value().is_empty())
        .map(|c| c.value().to_string())
}

/// Store the key in the login keychain. Electron used safeStorage, which is
/// keychain-backed too; this keeps the secret out of config.json either way.
fn store_session_key(key: &str) -> Result<(), String> {
    let status = std::process::Command::new("security")
        .args([
            "add-generic-password", "-U",
            "-s", "imburning-tauri", "-a", "claude-session",
            "-w", key,
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("keychain write failed".into()) }
}

pub fn read_session_key() -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "imburning-tauri", "-a", "claude-session", "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if key.is_empty() { None } else { Some(key) }
}

pub fn delete_session_key() {
    let _ = std::process::Command::new("security")
        .args(["delete-generic-password", "-s", "imburning-tauri", "-a", "claude-session"])
        .status();
}

/// Sign in to claude.ai.
///
/// The cookie jar is shared with the hidden fetch webview, so a session may
/// already be sitting there — from a previous run, or simply because the user
/// is signed in. The question that matters is not "did a cookie appear" but
/// "does the session WORK", so that is what this asks.
///
/// Both earlier attempts got this wrong from opposite directions: the first
/// accepted any cookie it found, so the window flashed open and shut using a
/// stale key; the second demanded the value CHANGE, so an already-signed-in
/// user waited forever at a logged-in Claude page for a sign-in that was
/// never going to happen.
pub async fn login(app: &AppHandle) -> Value {
    // Already signed in? Take it, and never show a window at all.
    if let Some(result) = try_existing_session(app).await {
        return result;
    }

    if app.get_webview_window(LOGIN_WEBVIEW).is_some() {
        return json!({ "success": false, "error": "A sign-in is already in progress" });
    }
    let Ok(url) = LOGIN_URL.parse() else {
        return json!({ "success": false, "error": "bad login url" });
    };
    let window = match WebviewWindowBuilder::new(app, LOGIN_WEBVIEW, WebviewUrl::External(url))
        .title("Claude Login — claude.ai")
        .inner_size(1000.0, 700.0)
        .build()
    {
        Ok(w) => w,
        Err(e) => return json!({ "success": false, "error": e.to_string() }),
    };

    // Poll until the session works. Five minutes is generous enough for an
    // SSO round trip; the user closing the window ends it sooner.
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if app.get_webview_window(LOGIN_WEBVIEW).is_none() {
            // They may well have signed in and then closed it themselves.
            return try_existing_session(app)
                .await
                .unwrap_or_else(|| json!({ "success": false, "error": "Login window closed" }));
        }
        if Instant::now() > deadline {
            let _ = window.close();
            return json!({ "success": false, "error": "Login timed out" });
        }
        tokio::time::sleep(Duration::from_secs(2)).await;

        if session_key_from(&window).is_none() {
            continue; // nothing to validate yet
        }
        if let Some(result) = try_existing_session(app).await {
            let _ = window.close();
            return result;
        }
    }
}

/// Ask the API whether the session in the shared cookie jar is usable. Some on
/// success (with the account's organizations), None if it is not signed in.
async fn try_existing_session(app: &AppHandle) -> Option<Value> {
    let orgs = fetch_via_webview(app, "https://claude.ai/api/organizations")
        .await
        .ok()?;
    let (id, list) = pick_organization(&orgs)?;

    // Keep the key so "are we logged in" can be answered without a round trip.
    if let Some(window) = app
        .get_webview_window(FETCH_WEBVIEW)
        .or_else(|| app.get_webview_window(LOGIN_WEBVIEW))
    {
        if let Some(key) = session_key_from(&window) {
            let _ = store_session_key(&key);
        }
    }
    Some(json!({ "success": true, "organizationId": id, "organizations": list }))
}

/// Cache the cookie jar's current session key, so "are we logged in" can be
/// answered without a network round trip. Called after any successful fetch:
/// the jar is the real source of truth, this is only a note about it.
pub fn remember_session(app: &AppHandle) {
    if let Some(window) = app
        .get_webview_window(FETCH_WEBVIEW)
        .or_else(|| app.get_webview_window("main"))
    {
        if let Some(key) = session_key_from(&window) {
            let _ = store_session_key(&key);
        }
    }
}

/// Is the stored session still usable? The organizations endpoint is the
/// cheapest thing that answers it, and it is the same call the login uses.
pub async fn session_is_valid(app: &AppHandle) -> bool {
    fetch_via_webview(app, "https://claude.ai/api/organizations")
        .await
        .ok()
        .and_then(|orgs| pick_organization(&orgs))
        .is_some()
}

/// Chat-capable orgs only (API-only orgs report no usage), preferring a team
/// org when the account has one — same rule as the Electron build.
pub fn pick_organization(orgs: &Value) -> Option<(String, Value)> {
    let list = orgs.as_array()?;
    let chat: Vec<&Value> = list
        .iter()
        .filter(|o| {
            o.get("capabilities")
                .and_then(|c| c.as_array())
                .map(|c| c.iter().any(|v| v.as_str() == Some("chat")))
                .unwrap_or(false)
        })
        .collect();
    if chat.is_empty() {
        return None;
    }
    let chosen = chat
        .iter()
        .find(|o| o.get("raven_type").and_then(|v| v.as_str()) == Some("team"))
        .copied()
        .unwrap_or(chat[0]);
    let id = chosen
        .get("uuid")
        .or_else(|| chosen.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    Some((id, json!(chat)))
}

/// Usage for an organization, plus the two optional extras the renderer draws
/// as its own rows. The extras are fetched separately and never allowed to
/// fail the whole call — a missing credits endpoint must not suppress valid
/// usage, which is how the Electron build orders it too.
/// The signed-in account's email, from claude.ai's own account endpoint.
/// Cached for the process; display only.
pub async fn account_email(app: &AppHandle) -> Option<String> {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<String>> = Mutex::new(None);
    if let Ok(c) = CACHE.lock() {
        if c.is_some() {
            return c.clone();
        }
    }
    let acct = fetch_via_webview(app, "https://claude.ai/api/account").await.ok()?;
    // The email sits under a couple of shapes across API versions; try both.
    let email = acct
        .get("email_address")
        .or_else(|| acct.get("email"))
        .or_else(|| acct.get("account").and_then(|a| a.get("email_address")))
        .and_then(|v| v.as_str())
        .map(String::from)?;
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(email.clone());
    }
    Some(email)
}

pub async fn fetch_usage(app: &AppHandle, org_id: &str) -> Result<Value, String> {
    let base = format!("https://claude.ai/api/organizations/{}", org_id);
    let mut usage = fetch_via_webview(app, &format!("{}/usage", base)).await?;

    if let Ok(overage) = fetch_via_webview(app, &format!("{}/overage_spend_limit", base)).await {
        let extra = usage.get("extra_usage").cloned().unwrap_or_else(|| json!({}));
        let mut extra = extra.as_object().cloned().unwrap_or_default();
        if let Some(enabled) = overage.get("is_enabled") {
            extra.insert("is_enabled".into(), enabled.clone());
        }
        if let Some(currency) = overage.get("currency") {
            extra.insert("currency".into(), currency.clone());
        }
        usage["extra_usage"] = Value::Object(extra);
    }
    if let Ok(prepaid) = fetch_via_webview(app, &format!("{}/prepaid/credits", base)).await {
        if let Some(amount) = prepaid.get("amount") {
            let extra = usage.get("extra_usage").cloned().unwrap_or_else(|| json!({}));
            let mut extra = extra.as_object().cloned().unwrap_or_default();
            extra.insert("balance_cents".into(), amount.clone());
            usage["extra_usage"] = Value::Object(extra);
        }
    }
    Ok(usage)
}
