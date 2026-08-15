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

/// Open the claude.ai login page and wait for the session cookie to appear.
/// The window is the user's to interact with — SSO, 2FA, whatever their
/// account needs — so this polls rather than automating anything.
pub async fn login(app: &AppHandle) -> Value {
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

    // Poll for the cookie until it appears or the user gives up and closes the
    // window. Five minutes is generous enough for an SSO round trip.
    let deadline = Instant::now() + Duration::from_secs(300);
    let key = loop {
        if app.get_webview_window(LOGIN_WEBVIEW).is_none() {
            return json!({ "success": false, "error": "Login window closed" });
        }
        if let Some(key) = session_key_from(&window) {
            break key;
        }
        if Instant::now() > deadline {
            let _ = window.close();
            return json!({ "success": false, "error": "Login timed out" });
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    };
    let _ = window.close();

    if let Err(e) = store_session_key(&key) {
        return json!({ "success": false, "error": e });
    }

    // Validate by listing organizations — also how the account's default org
    // is chosen, which every later usage call needs.
    match fetch_via_webview(app, "https://claude.ai/api/organizations").await {
        Ok(orgs) => match pick_organization(&orgs) {
            Some((id, list)) => json!({ "success": true, "organizationId": id, "organizations": list }),
            None => json!({ "success": false, "error": "No chat-enabled organizations found" }),
        },
        Err(e) => json!({ "success": false, "error": e }),
    }
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
