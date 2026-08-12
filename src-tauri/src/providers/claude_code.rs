// Anthropic usage read from the Claude Code CLI's own login.

use crate::providers::local_credential_files;
use serde_json::Value;

const USAGE: &str = "https://api.anthropic.com/api/oauth/usage";

/// Deliberately NO refresh-token flow: consuming Claude Code's rotating
/// refresh token would invalidate the user's CLI login. An expired token is
/// skipped instead, exactly as the Electron build does.
pub fn read_token() -> Option<String> {
    for path in local_credential_files(".claude", ".credentials.json") {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
        let oauth = json.get("claudeAiOauth")?;
        let access = oauth.get("accessToken").and_then(|v| v.as_str())?;
        if let Some(expires) = oauth.get("expiresAt").and_then(|v| v.as_i64()) {
            if chrono::Utc::now().timestamp_millis() >= expires {
                continue;
            }
        }
        return Some(access.to_string());
    }
    None
}

pub fn available() -> bool {
    read_token().is_some()
}

/// Returns the raw usage document. Its shape (five_hour / seven_day /
/// extra-usage blocks) is consumed directly by the renderer, so it is passed
/// through rather than reshaped here.
pub async fn fetch(client: &reqwest::Client) -> Option<Value> {
    let token = read_token()?;
    let res = client
        .get(USAGE)
        .header("Authorization", format!("Bearer {}", token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    res.json().await.ok()
}
