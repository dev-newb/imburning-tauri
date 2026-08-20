// Widget-owned sign-in for OpenAI and Google — the PKCE authorization-code
// flow with a loopback redirect.
//
// This is the login path for most people. Reading the codex/gemini CLI's
// credentials off disk only helps someone who already installed and signed
// into those CLIs; everyone else needs to click Connect and sign in with a
// browser, and never learns a CLI exists.
//
// The tokens live in the keychain, never in config.json.

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a parked sign-in waits for the browser round trip.
const FLOW_TIMEOUT: Duration = Duration::from_secs(300);

struct OAuthConfig {
    authorize_url: &'static str,
    token_url: &'static str,
    redirect_path: &'static str,
    scope: &'static str,
    /// OpenAI registered a fixed redirect port; Google's installed-app clients
    /// accept any loopback port, so 0 lets the OS choose a free one.
    port: u16,
}

const OPENAI: OAuthConfig = OAuthConfig {
    authorize_url: "https://auth.openai.com/oauth/authorize",
    token_url: "https://auth.openai.com/oauth/token",
    redirect_path: "/auth/callback",
    scope: "openid profile email offline_access",
    port: 1455,
};

const GOOGLE: OAuthConfig = OAuthConfig {
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    redirect_path: "/oauth2callback",
    scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",
    port: 0,
};

const OPENAI_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random_b64(len: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    b64url(&buf)
}

/// Claims out of a JWT body — display identity only, never a trust decision.
fn jwt_claims(token: Option<&str>) -> Value {
    let Some(token) = token else { return json!({}) };
    let Some(payload) = token.split('.').nth(1) else { return json!({}) };
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}))
}

// ---- token storage ---------------------------------------------------------

fn account(provider: &str) -> String {
    format!("oauth-{}", provider)
}

pub fn store_tokens(provider: &str, tokens: &Value) -> Result<(), String> {
    let status = std::process::Command::new("security")
        .args([
            "add-generic-password", "-U",
            "-s", "imburning-tauri",
            "-a", &account(provider),
            "-w", &tokens.to_string(),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("keychain write failed".into()) }
}

pub fn load_tokens(provider: &str) -> Option<Value> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "imburning-tauri", "-a", &account(provider), "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).ok()
}

pub fn clear_tokens(provider: &str) {
    let _ = std::process::Command::new("security")
        .args(["delete-generic-password", "-s", "imburning-tauri", "-a", &account(provider)])
        .status();
}

// ---- loopback callback server ----------------------------------------------

struct Callback {
    code: String,
}

/// A single-shot loopback listener for the redirect. Hand-rolled rather than
/// pulling in a web framework: it serves exactly one request and needs to read
/// one query string.
async fn await_callback(
    listener: TcpListener,
    path: &str,
    expected_state: &str,
) -> Result<Callback, String> {
    let deadline = tokio::time::Instant::now() + FLOW_TIMEOUT;
    loop {
        let accept = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| "Sign-in timed out".to_string())?;
        let Ok((mut socket, _)) = accept else { continue };

        let mut buf = [0u8; 4096];
        let n = socket.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        let Some(line) = request.lines().next() else { continue };
        let Some(target) = line.split_whitespace().nth(1) else { continue };
        if !target.starts_with(path) {
            let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
            continue;
        }

        let mut code = None;
        let mut state = None;
        let mut error = None;
        if let Some(query) = target.split('?').nth(1) {
            for pair in query.split('&') {
                let mut kv = pair.splitn(2, '=');
                let key = kv.next().unwrap_or("");
                let value = urldecode(kv.next().unwrap_or(""));
                match key {
                    "code" => code = Some(value),
                    "state" => state = Some(value),
                    "error" => error = Some(value),
                    _ => {}
                }
            }
        }

        let outcome = if let Some(err) = error {
            Err(format!("Authorization denied: {}", err))
        } else if state.as_deref() != Some(expected_state) {
            // A mismatched state means this redirect did not come from the
            // request we started. Refuse it.
            Err("State mismatch — sign-in rejected".to_string())
        } else if let Some(code) = code {
            Ok(Callback { code })
        } else {
            Err("No authorization code in the redirect".to_string())
        };

        let body = match &outcome {
            Ok(_) => "<h2>Signed in.</h2><p>You can close this tab and return to I'm Burning!.</p>",
            Err(_) => "<h2>Sign-in failed.</h2><p>You can close this tab and try again.</p>",
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.flush().await;
        return outcome;
    }
}

fn urldecode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

// ---- the flow ---------------------------------------------------------------

fn config_for(provider: &str) -> Option<(&'static OAuthConfig, String, Option<String>)> {
    match provider {
        "openai" => Some((&OPENAI, OPENAI_CLIENT_ID.to_string(), None)),
        "google" => {
            // Google has no public client of our own; the widget borrows the
            // gemini CLI's, same as the Electron build.
            let (id, secret) = crate::providers::gemini::oauth_client()?;
            Some((&GOOGLE, id, Some(secret)))
        }
        _ => None,
    }
}

pub async fn connect(app: &tauri::AppHandle, client: &reqwest::Client, provider: &str) -> Value {
    let Some((cfg, client_id, client_secret)) = config_for(provider) else {
        return json!({ "ok": false, "error": format!("Unknown provider: {}", provider) });
    };

    let verifier = random_b64(32);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);

    let listener = match TcpListener::bind(("127.0.0.1", cfg.port)).await {
        Ok(l) => l,
        Err(e) => {
            // OpenAI's port is fixed, so a foreign holder is something the user
            // can actually act on — say so instead of parroting EADDRINUSE.
            let msg = if provider == "openai" {
                "Port 1455 is in use — close any pending \"codex login\" in a terminal or browser tab, then try again".to_string()
            } else {
                e.to_string()
            };
            return json!({ "ok": false, "error": msg });
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let redirect_uri = format!("http://localhost:{}{}", port, cfg.redirect_path);

    let mut params = vec![
        ("response_type", "code".to_string()),
        ("client_id", client_id.clone()),
        ("redirect_uri", redirect_uri.clone()),
        ("scope", cfg.scope.to_string()),
        ("state", state.clone()),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256".to_string()),
    ];
    if provider == "google" {
        // Without these Google returns no refresh token on a repeat consent,
        // and the connection would silently expire in an hour.
        params.push(("access_type", "offline".to_string()));
        params.push(("prompt", "consent".to_string()));
    }
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let auth_url = format!("{}?{}", cfg.authorize_url, query);

    // The sign-in happens in the user's own browser, not in an embedded
    // webview: it is their real session, their password manager, their 2FA.
    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        return json!({ "ok": false, "error": format!("Could not open the browser: {}", e) });
    }
    let _ = app; // handle kept for symmetry with the other flows

    let callback = match await_callback(listener, cfg.redirect_path, &state).await {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "error": e }),
    };

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", callback.code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = &client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let res = client
        .post(cfg.token_url)
        .timeout(HTTP_TIMEOUT)
        .form(&form)
        .send()
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            crate::log_error(&format!("oauth {}: transport error {}", provider, e));
            return json!({ "ok": false, "error": format!("Token exchange failed: {}", e) });
        }
    };
    let status = res.status();
    let raw = res.text().await.unwrap_or_default();
    let body: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let Some(access) = body.get("access_token").and_then(|v| v.as_str()) else {
        // The provider's own error text is the only thing that explains this,
        // so surface it instead of a bare status code.
        crate::log_error(&format!(
            "oauth {}: exchange HTTP {} body {}",
            provider,
            status,
            &raw[..raw.len().min(400)]
        ));
        let detail = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("HTTP {}", status));
        return json!({ "ok": false, "error": format!("Token exchange failed: {}", detail) });
    };

    let claims = jwt_claims(body.get("id_token").and_then(|v| v.as_str()));
    let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    let tokens = json!({
        "accessToken": access,
        "refreshToken": body.get("refresh_token").and_then(|v| v.as_str()),
        "idToken": body.get("id_token").and_then(|v| v.as_str()),
        "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
        "email": claims.get("email").and_then(|v| v.as_str()),
        "accountId": claims
            .get("https://api.openai.com/auth")
            .and_then(|a| a.get("chatgpt_account_id"))
            .or_else(|| claims.get("sub"))
            .and_then(|v| v.as_str()),
    });
    if let Err(e) = store_tokens(provider, &tokens) {
        return json!({ "ok": false, "error": e });
    }
    json!({
        "ok": true,
        "email": tokens.get("email").cloned().unwrap_or(Value::Null),
        "accountId": tokens.get("accountId").cloned().unwrap_or(Value::Null),
    })
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "%20".to_string(),
            other => format!("%{:02X}", other),
        })
        .collect()
}

/// A usable access token, refreshing when it is close to expiry. Returns the
/// whole token object so callers can read the account identity too.
pub async fn access_token(client: &reqwest::Client, provider: &str) -> Option<Value> {
    let tokens = load_tokens(provider)?;
    let expires_at = tokens.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
    if chrono::Utc::now().timestamp_millis() < expires_at - 60_000 {
        return Some(tokens);
    }
    let refresh = tokens.get("refreshToken").and_then(|v| v.as_str())?.to_string();
    let (cfg, client_id, client_secret) = config_for(provider)?;

    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh.clone()),
        ("client_id", client_id),
    ];
    if let Some(secret) = &client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let res = client.post(cfg.token_url).timeout(HTTP_TIMEOUT).form(&form).send().await.ok()?;
    let body = res.json::<Value>().await.ok()?;
    let access = body.get("access_token").and_then(|v| v.as_str())?;

    let mut updated = tokens.clone();
    updated["accessToken"] = json!(access);
    // Rotation-safe: a response without a new refresh token means keep ours.
    if let Some(new_refresh) = body.get("refresh_token").and_then(|v| v.as_str()) {
        updated["refreshToken"] = json!(new_refresh);
    }
    let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    updated["expiresAt"] = json!(chrono::Utc::now().timestamp_millis() + expires_in * 1000);
    let _ = store_tokens(provider, &updated);
    Some(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The callback server is the security-relevant half of the flow: it must
    /// extract the code, reject a redirect whose state does not match the
    /// request we started, and answer the browser either way.
    async fn round_trip(query: &str, expected_state: &str) -> Result<String, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let path = "/auth/callback";

        let expected = expected_state.to_string();
        let server = tokio::spawn(async move {
            await_callback(listener, "/auth/callback", &expected).await.map(|c| c.code)
        });

        // Pretend to be the browser following the redirect.
        let mut sock = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("GET {}?{} HTTP/1.1\r\nHost: localhost\r\n\r\n", path, query);
        sock.write_all(request.as_bytes()).await.unwrap();
        let mut reply = String::new();
        let _ = sock.read_to_string(&mut reply).await;
        assert!(reply.starts_with("HTTP/1.1 200"), "browser must get a page: {}", reply);

        server.await.unwrap()
    }

    #[tokio::test]
    async fn extracts_the_code_when_state_matches() {
        let got = round_trip("code=abc123&state=xyz", "xyz").await;
        assert_eq!(got.unwrap(), "abc123");
    }

    #[tokio::test]
    async fn rejects_a_mismatched_state() {
        let got = round_trip("code=abc123&state=forged", "xyz").await;
        assert!(got.unwrap_err().contains("State mismatch"));
    }

    #[tokio::test]
    async fn surfaces_a_denied_authorization() {
        let got = round_trip("error=access_denied&state=xyz", "xyz").await;
        assert!(got.unwrap_err().contains("access_denied"));
    }

    #[tokio::test]
    async fn urldecodes_the_code() {
        let got = round_trip("code=a%2Fb%2Bc&state=xyz", "xyz").await;
        assert_eq!(got.unwrap(), "a/b+c");
    }

    #[test]
    fn pkce_challenge_is_the_sha256_of_the_verifier() {
        // RFC 7636 appendix B's worked example.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
}
