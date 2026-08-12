// Antigravity (agy CLI) usage — the Rust port of the Electron build's reader.
//
// Two hard-won details carried over verbatim, because the endpoint documents
// neither and both cost real debugging time:
//
//   1. cloudcode-pa answers 403 PERMISSION_DENIED to a request that sends no
//      User-Agent. Same token, same body — the header alone decides.
//   2. Every Gemini model reports ONE shared allowance (identical fraction and
//      reset time), with Claude/GPT-OSS on a second. Rows are grouped by quota
//      signature so a single pool draws a single bar.

use crate::providers::{Limit, ProviderData};
use serde_json::Value;
use std::process::Command;

const HOST: &str = "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";
const OAUTH: &str = "https://oauth2.googleapis.com/token";

fn user_agent() -> String {
    format!("antigravity-cli/1.0 ({}; {})", std::env::consts::OS, std::env::consts::ARCH)
}

fn client_metadata() -> String {
    let platform = match std::env::consts::OS {
        "macos" => "MACOS",
        "windows" => "WINDOWS",
        _ => "LINUX",
    };
    format!(
        r#"{{"ideType":"ANTIGRAVITY","platform":"{}","pluginType":"GEMINI"}}"#,
        platform
    )
}

pub struct Token {
    pub access: Option<String>,
    pub refresh: String,
    pub expiry_ms: i64,
}

/// Read the agy login out of the macOS keychain. Go's keyring library stores
/// the JSON base64-wrapped behind a `go-keyring-base64:` marker.
pub fn read_keychain_token() -> Option<Token> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let out = Command::new("security")
        .args(["find-generic-password", "-s", "gemini", "-a", "antigravity", "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let payload = match raw.strip_prefix("go-keyring-base64:") {
        Some(b64) => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
            String::from_utf8(bytes).ok()?
        }
        None => raw,
    };
    let json: Value = serde_json::from_str(&payload).ok()?;
    let tk = json.get("token").unwrap_or(&json);
    let refresh = tk.get("refresh_token")?.as_str()?.to_string();
    let expiry_ms = tk
        .get("expiry")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    Some(Token {
        access: tk.get("access_token").and_then(|v| v.as_str()).map(String::from),
        refresh,
        expiry_ms,
    })
}

pub fn agy_binary() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join(".local/bin/agy"),
        std::path::PathBuf::from("/opt/homebrew/bin/agy"),
        std::path::PathBuf::from("/usr/local/bin/agy"),
        home.join(".agy/bin/agy"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// The OAuth client lives inside the agy binary. Scan for it rather than
/// embedding a secret in this source tree — when agy rotates its client, a
/// reinstall of agy is the only thing needed to keep this working.
fn oauth_clients() -> Vec<(String, String)> {
    let Some(bin) = agy_binary() else { return vec![] };
    let Ok(bytes) = std::fs::read(&bin) else { return vec![] };
    let text = String::from_utf8_lossy(&bytes);
    let id_re = regex::Regex::new(r"[0-9]{10,}-[a-z0-9]+\.apps\.googleusercontent\.com").unwrap();
    let secret_re = regex::Regex::new(r"GOCSPX-[A-Za-z0-9_-]{28}").unwrap();
    let mut ids: Vec<String> = id_re.find_iter(&text).map(|m| m.as_str().to_string()).collect();
    let mut secrets: Vec<String> = secret_re.find_iter(&text).map(|m| m.as_str().to_string()).collect();
    ids.dedup();
    ids.sort();
    ids.dedup();
    secrets.sort();
    secrets.dedup();
    let mut pairs = vec![];
    for id in &ids {
        for secret in &secrets {
            pairs.push((id.clone(), secret.clone()));
        }
    }
    pairs
}

/// Remembers the one credential pair that actually works. Scanning the binary
/// yields every id crossed with every secret, and only one combination is
/// real — without this, each refresh replays the same rejected attempts
/// against Google's token endpoint.
static WORKING_CLIENT: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

async fn refresh_token(client: &reqwest::Client, refresh: &str) -> Option<String> {
    let known = WORKING_CLIENT.lock().ok().and_then(|g| g.clone());
    let mut candidates = oauth_clients();
    if let Some(known) = &known {
        candidates.sort_by_key(|c| c != known);
    }
    for (id, secret) in candidates {
        let params = [
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ];
        let res = client
            .post(OAUTH)
            .header("User-Agent", user_agent())
            .form(&params)
            .send()
            .await
            .ok()?;
        if res.status().is_success() {
            let json: Value = res.json().await.ok()?;
            if let Some(at) = json.get("access_token").and_then(|v| v.as_str()) {
                if let Ok(mut slot) = WORKING_CLIENT.lock() {
                    *slot = Some((id, secret));
                }
                return Some(at.to_string());
            }
        }
    }
    None
}

async fn fetch_models(client: &reqwest::Client, access: &str) -> Option<Value> {
    let res = client
        .post(HOST)
        .header("Authorization", format!("Bearer {}", access))
        .header("Content-Type", "application/json")
        .header("Client-Metadata", client_metadata())
        .header("User-Agent", user_agent()) // load-bearing: no UA ⇒ 403
        .body("{}")
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    res.json().await.ok()
}

/// True when there is an agy login to read — gates the "auto" source.
pub fn available() -> bool {
    agy_binary().is_some() && read_keychain_token().is_some()
}

struct Entry {
    label: String,
    percent: f64,
    resets_at: String,
}

fn strip_effort(name: &str) -> String {
    let re = regex::Regex::new(r"(?i)\s*\((high|low|medium|extra low|tiered|standard)\)\s*$").unwrap();
    re.replace(name, "").trim().to_string()
}

fn label_for(display_name: Option<&str>, model_id: &str) -> String {
    if let Some(dn) = display_name {
        let stripped = strip_effort(dn);
        if !stripped.is_empty() {
            return stripped;
        }
    }
    let stem = model_id
        .trim_start_matches("gemini-")
        .rsplit_once('-')
        .map(|(head, tail)| {
            if matches!(tail, "high" | "low" | "medium" | "agent" | "tiered") { head } else { model_id }
        })
        .unwrap_or(model_id)
        .split('-')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) if !f.is_ascii_digit() => f.to_uppercase().collect::<String>() + c.as_str(),
                _ => p.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if model_id.starts_with("gemini-") { format!("Gemini {}", stem) } else { stem }
}

fn priority(label: &str) -> i32 {
    let l = label.to_lowercase();
    if l.contains("pro") { 0 } else if l.contains("flash") && !l.contains("lite") { 10 }
    else if l.contains("lite") { 11 } else { 20 }
}

/// Group by the quota itself, not by model name — see the note at the top.
fn group(entries: Vec<Entry>, family_label: &str) -> Vec<Limit> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, (f64, String, Vec<String>)> = BTreeMap::new();
    for e in entries {
        let sig = format!("{}|{}", e.percent, e.resets_at);
        let slot = groups.entry(sig).or_insert((e.percent, e.resets_at.clone(), vec![]));
        if !slot.2.contains(&e.label) {
            slot.2.push(e.label);
        }
    }
    let only = groups.len() == 1;
    let mut out: Vec<Limit> = groups
        .into_values()
        .map(|(percent, resets_at, mut names)| {
            names.sort();
            let label = if names.len() == 1 {
                names[0].clone()
            } else if only {
                family_label.to_string()
            } else {
                format!("{} +{}", names[0], names.len() - 1)
            };
            let key = format!(
                "m_{}",
                label
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>()
                    .split('_')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            Limit { key, label, percent, resets_at: Some(resets_at) , default_hidden: false}
        })
        .collect();
    out.sort_by(|a, b| {
        priority(&a.label)
            .cmp(&priority(&b.label))
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

pub fn normalize(json: &Value) -> Option<ProviderData> {
    let models = json.get("models")?.as_object()?;
    let mut gemini = vec![];
    let mut foreign = vec![];
    for (model_id, m) in models {
        let Some(qi) = m.get("quotaInfo") else { continue };
        let Some(fraction) = qi.get("remainingFraction").and_then(|v| v.as_f64()) else { continue };
        if model_id.starts_with("chat_") || model_id.starts_with("tab_") {
            continue; // internal editor pseudo-models
        }
        let Some(reset) = qi.get("resetTime").and_then(|v| v.as_str()) else {
            continue; // unmetered helpers report a bare fraction of 1
        };
        let display = m.get("displayName").and_then(|v| v.as_str());
        let entry = Entry {
            label: label_for(display, model_id),
            percent: ((1.0 - fraction) * 1000.0).round() / 10.0,
            resets_at: reset.to_string(),
        };
        let is_gemini = model_id.starts_with("gemini")
            || display.map(|d| d.to_lowercase().contains("gemini")).unwrap_or(false);
        if is_gemini { gemini.push(entry) } else { foreign.push(entry) }
    }
    let limits = group(gemini, "Gemini (all models)");
    let foreign = group(foreign, "Non-Gemini models");
    if limits.is_empty() && foreign.is_empty() {
        return None;
    }
    Some(ProviderData {
        source: "antigravity".into(),
        connected: true,
        email: None,
        limits,
        foreign,
        cli: None,
    })
}

pub async fn fetch(client: &reqwest::Client) -> Option<ProviderData> {
    let token = read_keychain_token()?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut access = match &token.access {
        Some(a) if token.expiry_ms - now > 60_000 => Some(a.clone()),
        _ => None,
    };
    if access.is_none() {
        access = refresh_token(client, &token.refresh).await;
    }
    let access = access?;
    let mut json = fetch_models(client, &access).await;
    if json.is_none() {
        // A stored token can be rejected even before its stated expiry.
        if let Some(fresh) = refresh_token(client, &token.refresh).await {
            json = fetch_models(client, &fresh).await;
        }
    }
    normalize(&json?)
}
