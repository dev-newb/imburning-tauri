// OpenAI/Codex usage read from the Codex CLI's own login.

use crate::providers::{local_credential_files, Limit, ProviderData};
use serde_json::{json, Value};

const USAGE: &str = "https://chatgpt.com/backend-api/wham/usage";
const CHROME_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                         (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct Candidate {
    pub access_token: String,
    pub account_id: Option<String>,
}

/// Claims out of a JWT body. Display/expiry only — never a trust decision.
fn jwt_claims(token: &str) -> Option<Value> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn read_candidates() -> Vec<Candidate> {
    let mut out = vec![];
    for path in local_credential_files(".codex", "auth.json") {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
        let Some(tokens) = json.get("tokens") else { continue };
        let Some(access) = tokens.get("access_token").and_then(|v| v.as_str()) else { continue };
        // An expired access token means the session-log fallback should be
        // used instead — skip rather than sending a request that will 401.
        if let Some(exp) = jwt_claims(access).and_then(|c| c.get("exp").and_then(|v| v.as_i64())) {
            if chrono::Utc::now().timestamp() >= exp {
                continue;
            }
        }
        out.push(Candidate {
            access_token: access.to_string(),
            account_id: tokens.get("account_id").and_then(|v| v.as_str()).map(String::from),
        });
    }
    out
}

pub fn available() -> bool {
    !read_candidates().is_empty()
}

/// "5h" / "7d" style suffix from a window length in seconds.
fn window_suffix(seconds: Option<i64>) -> String {
    let hours = seconds.map(|s| (s as f64 / 3600.0).round() as i64).unwrap_or(0);
    if hours >= 24 {
        format!("{}d", (hours as f64 / 24.0).round() as i64)
    } else {
        format!("{}h", hours.max(1))
    }
}

fn window_key(seconds: Option<i64>, prefix: &str) -> String {
    let hours = seconds.map(|s| (s as f64 / 3600.0).round() as i64).unwrap_or(0);
    if hours >= 24 * 6 {
        format!("{}_seven_day", prefix)
    } else {
        format!("{}_five_hour", prefix)
    }
}

/// reset_at arrives as unix SECONDS, not milliseconds and not a string.
fn reset_iso(window: &Value) -> Option<String> {
    let secs = window.get("reset_at").and_then(|v| v.as_i64())?;
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
}

fn window_limit(window: &Value, key: String, label: String) -> Option<Limit> {
    let used = window.get("used_percent").and_then(|v| v.as_f64())?;
    Some(Limit { key, label, percent: used, resets_at: reset_iso(window)})
}

fn normalize(json: &Value) -> Option<ProviderData> {
    let mut limits = vec![];
    for (field, prefix) in [("primary_window", "primary"), ("secondary_window", "secondary")] {
        let Some(w) = json.get("rate_limit").and_then(|r| r.get(field)) else { continue };
        let seconds = w.get("limit_window_seconds").and_then(|v| v.as_i64());
        if let Some(limit) = window_limit(
            w,
            window_key(seconds, prefix),
            format!("Codex ({})", window_suffix(seconds)),
        ) {
            limits.push(limit);
        }
    }
    // Per-feature sub-pools (e.g. GPT-5.3-Codex-Spark) are genuinely separate
    // limits, so they show even at 0%, like every other tracked pool.
    for extra in json.get("additional_rate_limits").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
        let Some(w) = extra.get("rate_limit").and_then(|r| r.get("primary_window")) else { continue };
        let raw = extra.get("limit_name").and_then(|v| v.as_str()).unwrap_or("Extra");
        let name = regex::Regex::new(r"(?i)^gpt-[\d.]+-codex-")
            .map(|re| re.replace(raw, "").to_string())
            .unwrap_or_else(|_| raw.to_string());
        let seconds = w.get("limit_window_seconds").and_then(|v| v.as_i64());
        let key = format!(
            "extra_{}_seven_day",
            name.to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        );
        if let Some(limit) = window_limit(w, key, format!("{} ({})", name, window_suffix(seconds))) {
            limits.push(limit);
        }
    }
    // Code review gets its own pool once the account has used the feature.
    if let Some(w) = json.get("code_review_rate_limit").and_then(|r| r.get("primary_window")) {
        let seconds = w.get("limit_window_seconds").and_then(|v| v.as_i64());
        if let Some(limit) = window_limit(
            w,
            "code_review_seven_day".into(),
            format!("Code Review ({})", window_suffix(seconds)),
        ) {
            limits.push(limit);
        }
    }
    if limits.is_empty() {
        return None;
    }
    // Prepaid credits and banked resets each drive their own summary row in
    // the renderer, so they are passed through rather than dropped.
    let credits = json.get("credits").map(|c| {
        json!({
            "balance": c.get("balance").cloned().unwrap_or(Value::Null),
            "hasCredits": c.get("has_credits").and_then(|v| v.as_bool()).unwrap_or(false),
            "unlimited": c.get("unlimited").and_then(|v| v.as_bool()).unwrap_or(false),
            "approxLocal": c.get("approx_local_messages").cloned().unwrap_or(Value::Null),
            "approxCloud": c.get("approx_cloud_messages").cloned().unwrap_or(Value::Null),
        })
    });
    // OpenAI's weekly-limit reset feature: banked resets that can be spent to
    // clear a hit limit early (applicable = usable right now).
    let reset_credits = json.get("rate_limit_reset_credits").map(|r| {
        json!({
            "available": r.get("available_count").and_then(|v| v.as_i64()).unwrap_or(0),
            "applicable": r.get("applicable_available_count").and_then(|v| v.as_i64()).unwrap_or(0),
        })
    });

    Some(ProviderData {
        source: "live".into(),
        connected: false, // via CLI login
        email: json.get("email").and_then(|v| v.as_str()).map(String::from),
        limits,
        cli: None,
        credits,
        reset_credits,
        account_id: json
            .get("account_id")
            .or_else(|| json.get("user_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

pub async fn fetch(client: &reqwest::Client) -> Option<ProviderData> {
    for candidate in read_candidates() {
        let mut req = client
            .get(USAGE)
            .header("Authorization", format!("Bearer {}", candidate.access_token))
            .header("User-Agent", CHROME_UA);
        if let Some(account) = &candidate.account_id {
            req = req.header("chatgpt-account-id", account.clone());
        }
        let Ok(res) = req.send().await else { continue };
        if !res.status().is_success() {
            continue;
        }
        let Ok(json) = res.json::<Value>().await else { continue };
        if let Some(data) = normalize(&json) {
            return Some(data);
        }
    }
    None
}
