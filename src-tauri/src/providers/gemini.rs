// Classic Gemini Code Assist quota, read from the gemini CLI's own login.
//
// Kept as a first-class option even though Google now answers
// UNSUPPORTED_CLIENT for individual accounts on this surface ("migrate to the
// Antigravity suite") — paid/enterprise accounts still report real buckets
// here, and the owner asked that this path not be deprecated.

use crate::providers::{local_credential_files, Limit, ProviderData};
use serde_json::{json, Value};

const CODE_ASSIST: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const OAUTH: &str = "https://oauth2.googleapis.com/token";

/// The widget borrows the gemini CLI's own OAuth client rather than embedding
/// one. Discovering it from the local install (instead of hardcoding) means a
/// client rotation upstream is picked up by reinstalling the CLI. Returns None
/// when gemini-cli is not installed, which simply means no Gemini rows.
fn oauth_client() -> Option<(String, String)> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<(String, String)>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let home = dirs::home_dir()?;
            // Version-manager layouts (nvm/fnm/volta/standalone tarballs) put
            // node_modules under a directory named for the version, so no fixed
            // list can enumerate them — glob one level instead.
            let glob_roots = |base: std::path::PathBuf, tail: &[&str]| -> Vec<std::path::PathBuf> {
                let mut out = vec![];
                if let Ok(entries) = std::fs::read_dir(&base) {
                    for e in entries.flatten() {
                        let mut p = e.path();
                        for t in tail {
                            p = p.join(t);
                        }
                        if p.is_dir() {
                            out.push(p);
                        }
                    }
                }
                out
            };
            let mut prefixes = vec![
                std::path::PathBuf::from("/opt/homebrew/lib/node_modules"),
                std::path::PathBuf::from("/usr/local/lib/node_modules"),
                std::path::PathBuf::from("/usr/lib/node_modules"),
                home.join(".npm-global/lib/node_modules"),
                home.join(".local/lib/node_modules"),
                home.join(".local/opt/node/lib/node_modules"),
                home.join(".bun/install/global/node_modules"),
            ];
            prefixes.extend(glob_roots(home.join(".nvm/versions/node"), &["lib", "node_modules"]));
            prefixes.extend(glob_roots(home.join(".local/opt"), &["lib", "node_modules"]));
            prefixes.extend(glob_roots(
                home.join(".local/share/fnm/node-versions"),
                &["installation", "lib", "node_modules"],
            ));
            prefixes.extend(glob_roots(
                home.join(".volta/tools/image/node"),
                &["lib", "node_modules"],
            ));

            let re = regex::Regex::new(
                r"(?s)([0-9]{10,}-[a-z0-9]+\.apps\.googleusercontent\.com).{0,300}?(GOCSPX-[A-Za-z0-9_-]+)",
            )
            .ok()?;
            for prefix in prefixes {
                let root = prefix.join("@google").join("gemini-cli");
                if !root.exists() {
                    continue;
                }
                let mut stack = vec![root];
                while let Some(dir) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            if entry.file_name() != "node_modules" {
                                stack.push(p);
                            }
                            continue;
                        }
                        if p.extension().map(|e| e != "js").unwrap_or(true) {
                            continue;
                        }
                        if std::fs::metadata(&p).map(|m| m.len() > 30_000_000).unwrap_or(true) {
                            continue;
                        }
                        let Ok(text) = std::fs::read_to_string(&p) else { continue };
                        if let Some(m) = re.captures(&text) {
                            return Some((m[1].to_string(), m[2].to_string()));
                        }
                    }
                }
            }
            None
        })
        .clone()
}

struct Creds {
    access: Option<String>,
    refresh: Option<String>,
    expiry_ms: i64,
    id_token: Option<String>,
}

fn read_creds() -> Option<Creds> {
    for path in local_credential_files(".gemini", "oauth_creds.json") {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
        let access = json.get("access_token").and_then(|v| v.as_str()).map(String::from);
        let refresh = json.get("refresh_token").and_then(|v| v.as_str()).map(String::from);
        if access.is_none() && refresh.is_none() {
            continue;
        }
        return Some(Creds {
            access,
            refresh,
            expiry_ms: json.get("expiry_date").and_then(|v| v.as_i64()).unwrap_or(0),
            id_token: json.get("id_token").and_then(|v| v.as_str()).map(String::from),
        });
    }
    None
}

pub fn has_credentials() -> bool {
    read_creds().is_some()
}

/// Email from the stored id_token's claims — display only, never verified here.
fn email_from_id_token(id_token: &str) -> Option<String> {
    use base64::Engine;
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("email").and_then(|v| v.as_str()).map(String::from)
}

async fn access_token(client: &reqwest::Client, creds: &Creds) -> Option<String> {
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(at) = &creds.access {
        if creds.expiry_ms - now > 60_000 {
            return Some(at.clone());
        }
    }
    let refresh = creds.refresh.as_ref()?;
    let (client_id, client_secret) = oauth_client()?;
    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("refresh_token", refresh.as_str()),
        ("grant_type", "refresh_token"),
    ];
    let res = client.post(OAUTH).form(&params).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    let json: Value = res.json().await.ok()?;
    json.get("access_token").and_then(|v| v.as_str()).map(String::from)
}

async fn post(client: &reqwest::Client, token: &str, method: &str, payload: Value) -> Option<Value> {
    let res = client
        .post(format!("{}:{}", CODE_ASSIST, method))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    res.json().await.ok()
}

fn model_label(model_id: &str) -> String {
    // "(1D)" not "(daily)" — Google meters many model versions at once and the
    // long suffix crowds the labels off the bars.
    let parts = model_id
        .trim_start_matches("gemini-")
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
    format!("{} (1D)", parts)
}

fn model_priority(model_id: &str) -> i32 {
    let id = model_id.to_lowercase();
    if id.contains("pro") { 0 }
    else if id == "gemini-3.6-flash" { 10 }
    else if id == "gemini-3.5-flash-lite" { 11 }
    else { 20 }
}

pub fn normalize(quota: &Value) -> Option<Vec<Limit>> {
    let buckets = quota.get("buckets")?.as_array()?;
    let mut limits: Vec<(i32, usize, Limit)> = vec![];
    for (index, bucket) in buckets.iter().enumerate() {
        let Some(fraction) = bucket.get("remainingFraction").and_then(|v| v.as_f64()) else { continue };
        let Some(model_id) = bucket.get("modelId").and_then(|v| v.as_str()) else { continue };
        limits.push((
            model_priority(model_id),
            index,
            Limit {
                key: format!(
                    "m_{}",
                    model_id
                        .to_lowercase()
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ),
                label: model_label(model_id),
                percent: ((1.0 - fraction) * 1000.0).round() / 10.0,
                resets_at: bucket.get("resetTime").and_then(|v| v.as_str()).map(String::from),
                default_hidden: false,
            },
        ));
    }
    if limits.is_empty() {
        return None;
    }
    limits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Some(limits.into_iter().map(|(_, _, l)| l).collect())
}

pub async fn fetch(client: &reqwest::Client) -> Option<ProviderData> {
    let creds = read_creds()?;
    let token = access_token(client, &creds).await?;
    // Resolve the account's Code Assist project first: sending an empty object
    // succeeds but returns only a partial bucket set for some accounts.
    let load = post(
        client,
        &token,
        "loadCodeAssist",
        json!({"metadata":{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}}),
    )
    .await;
    let project = load
        .as_ref()
        .and_then(|l| l.get("cloudaicompanionProject"))
        .and_then(|v| v.as_str());
    let payload = match project {
        Some(p) => json!({ "project": p }),
        None => json!({}),
    };
    let quota = post(client, &token, "retrieveUserQuota", payload).await?;
    let limits = normalize(&quota)?;
    Some(ProviderData {
        source: "live".into(),
        connected: false, // via CLI login, not a widget-owned connection
        email: creds.id_token.as_deref().and_then(email_from_id_token),
        limits,
        foreign: vec![],
        cli: None,
    })
}
