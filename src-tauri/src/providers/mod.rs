pub mod antigravity;
pub mod claude_code;
pub mod codex;
pub mod gemini;

use serde::{Deserialize, Serialize};

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
    /// Ships hidden on first sight; the renderer records that once in
    /// hiddenRowsSeeded and thereafter treats the row as any other.
    /// Must be a real field: setting it on the serialized JSON and parsing
    /// back into this struct silently drops it.
    #[serde(rename = "defaultHidden", default, skip_serializing_if = "std::ops::Not::not")]
    pub default_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderData {
    pub source: String,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub limits: Vec<Limit>,
    /// Pools metered by the same login but belonging to another vendor
    /// (Claude / GPT-OSS via Antigravity). Fetched, not yet rendered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign: Vec<Limit>,
    /// A second account for the same provider (the CLI login), when it differs
    /// from the one the widget itself is signed into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<Box<ProviderData>>,
}

impl ProviderData {
    pub fn new(source: &str) -> Self {
        ProviderData {
            source: source.into(),
            connected: false,
            email: None,
            limits: vec![],
            foreign: vec![],
            cli: None,
        }
    }
}

/// Candidate credential directories for a CLI that keeps its login under
/// `~/<dir>/<file>`. The Electron build also walks sandboxed copies; this
/// covers the plain install plus the common Homebrew/XDG placements.
pub fn local_credential_files(dir: &str, file: &str) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(dir).join(file));
        out.push(home.join(".config").join(dir.trim_start_matches('.')).join(file));
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}
