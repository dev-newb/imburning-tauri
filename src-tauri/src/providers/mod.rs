pub mod antigravity;
pub mod claude_code;
pub mod codex;
pub mod gemini;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One metered pool, shaped exactly like the object the renderer already
/// consumes — the frontend is unchanged from the Electron build, so these
/// field names are a contract, not a preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limit {
    pub key: String,
    pub label: String,
    pub percent: f64,
    #[serde(rename = "resetsAt", skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderData {
    pub source: String,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub limits: Vec<Limit>,
    /// A second account for the same provider (the CLI login), when it differs
    /// from the one the widget itself is signed into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<Box<ProviderData>>,
    /// Codex prepaid credits. The renderer draws its own summary row from
    /// this, so dropping it silently removes a row the Electron build shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<Value>,
    /// Banked weekly-limit resets, likewise its own row.
    #[serde(rename = "resetCredits", skip_serializing_if = "Option::is_none")]
    pub reset_credits: Option<Value>,
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

impl ProviderData {
    pub fn new(source: &str) -> Self {
        ProviderData {
            source: source.into(),
            connected: false,
            email: None,
            limits: vec![],
            cli: None,
            credits: None,
            reset_credits: None,
            account_id: None,
        }
    }
}

/// Candidate credential directories for a CLI that keeps its login under
/// `~/<dir>/<file>`. The Electron build also walks sandboxed copies; this
/// covers the plain install plus the common Homebrew/XDG placements.
/// Email claim out of a JWT payload — used to name detected CLI logins in
/// the offer chips without any network traffic.
pub fn jwt_email(token: &str) -> Option<String> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("email").and_then(|v| v.as_str()).map(String::from)
}

pub fn local_credential_files(dir: &str, file: &str) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(dir).join(file));
        out.push(home.join(".config").join(dir.trim_start_matches('.')).join(file));
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}
