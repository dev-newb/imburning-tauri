// OpenAI/Codex usage read from the Codex CLI's own login.

use crate::providers::{local_credential_files, Limit, ProviderData};
use serde_json::{json, Value};

const USAGE: &str = "https://chatgpt.com/backend-api/wham/usage";
const CHROME_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                         (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct Candidate {
    pub access_token: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
}

/// Claims out of a JWT body. Display/expiry only — never a trust decision.
fn jwt_claims(token: &str) -> Option<Value> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn candidate_from_auth(json: &Value, now: i64) -> Option<Candidate> {
    let tokens = json.get("tokens")?;
    let access = tokens.get("access_token")?.as_str()?;
    // An expired access token means the session-log fallback should be
    // used instead — skip rather than sending a request that will 401.
    if let Some(exp) = jwt_claims(access).and_then(|c| c.get("exp").and_then(|v| v.as_i64())) {
        if now >= exp {
            return None;
        }
    }
    Some(Candidate {
        access_token: access.to_string(),
        account_id: tokens.get("account_id").and_then(|v| v.as_str()).map(String::from),
        email: tokens.get("id_token").and_then(Value::as_str).and_then(crate::providers::jwt_email),
    })
}

pub fn read_candidates() -> Vec<Candidate> {
    let mut out = vec![];
    for path in local_credential_files(".codex", "auth.json") {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
        if let Some(candidate) = candidate_from_auth(&json, chrono::Utc::now().timestamp()) {
            out.push(candidate);
        }
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

fn normalize(json: &Value, widget_login: bool) -> Option<ProviderData> {
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
        // TRUE when the user signed in through the app itself. The UI shows
        // "Not connected (using CLI login)" off this flag, so hardcoding it
        // false meant a successful sign-in still reported as not connected —
        // the token was in the keychain the whole time.
        connected: widget_login,
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

/// A detected-but-unadopted CLI login, for the offer chip: Some(email) when
/// ~/.codex/auth.json holds usable credentials. Local read only — no network.
pub fn cli_offer_email() -> Option<Option<String>> {
    let path = crate::providers::local_credential_files(".codex", "auth.json")
        .into_iter()
        .next()?;
    let auth: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    auth.get("tokens")?.get("access_token")?.as_str()?;
    let email = auth
        .get("tokens")
        .and_then(|t| t.get("id_token"))
        .and_then(|v| v.as_str())
        .and_then(crate::providers::jwt_email);
    Some(email)
}

pub async fn fetch(client: &reqwest::Client, cli_allowed: bool) -> Option<ProviderData> {
    // A widget-owned OAuth login comes first: the user signed in through the
    // app itself, so it is the account they expect to see. CLI credentials
    // remain a fallback for people who do have codex installed.
    // (candidate, came_from_widget_login)
    let mut candidates: Vec<(Candidate, bool)> = vec![];
    if let Some(tokens) = crate::oauth::access_token(client, "openai").await {
        if let Some(access) = tokens.get("accessToken").and_then(|v| v.as_str()) {
            candidates.push((
                Candidate {
                    access_token: access.to_string(),
                    account_id: tokens.get("accountId").and_then(|v| v.as_str()).map(String::from),
                    email: tokens.get("email").and_then(Value::as_str).map(String::from),
                },
                true,
            ));
        }
    }
    // Un-adopted CLI credentials stay invisible: no fallback account.
    if cli_allowed {
        candidates.extend(read_candidates().into_iter().map(|c| (c, false)));
    }

    let mut results = vec![];
    for (candidate, widget_login) in candidates {
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
        if let Some(mut data) = normalize(&json, widget_login) {
            // Prefer the identity from the login itself; the usage payload
            // does not always carry an email.
            apply_candidate_identity(&mut data, candidate);
            results.push(data);
        }
    }
    select_accounts(results)
}

fn apply_candidate_identity(data: &mut ProviderData, candidate: Candidate) {
    if data.email.is_none() { data.email = candidate.email; }
    if data.account_id.is_none() { data.account_id = candidate.account_id; }
}

fn select_accounts(mut results: Vec<ProviderData>) -> Option<ProviderData> {
    if results.is_empty() { return None; }
    let mut primary = results.remove(0);
    if primary.connected {
        primary.cli = results.into_iter().find(|cli| {
            matches!((&primary.account_id, &cli.account_id), (Some(a), Some(b)) if a != b)
        }).map(Box::new);
    }
    Some(primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn jwt(claims: Value) -> String {
        format!("header.{}.signature", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn candidate_keeps_its_own_id_token_email_and_rejects_expired_access() {
        let auth = json!({"tokens": {"access_token": jwt(json!({"exp": 200})),
            "id_token": jwt(json!({"email": "cli@example.test"})), "account_id": "cli-id"}});
        let candidate = candidate_from_auth(&auth, 100).unwrap();
        assert_eq!(candidate.email.as_deref(), Some("cli@example.test"));
        assert_eq!(candidate.account_id.as_deref(), Some("cli-id"));
        assert!(candidate_from_auth(&auth, 200).is_none());
        assert!(candidate_from_auth(&json!({"tokens": {"id_token": "bad"}}), 100).is_none());
    }

    fn usage(id: &str, email: &str, connected: bool) -> ProviderData {
        let mut data = ProviderData::new("live");
        data.connected = connected;
        data.account_id = Some(id.into());
        data.email = Some(email.into());
        data
    }

    #[test]
    fn missing_usage_identity_is_filled_from_the_matching_candidate() {
        let mut data = ProviderData::new("live");
        apply_candidate_identity(&mut data, Candidate { access_token: "unused".into(),
            account_id: Some("cli-id".into()), email: Some("cli@example.test".into()) });
        assert_eq!(data.email.as_deref(), Some("cli@example.test"));
        assert_eq!(data.account_id.as_deref(), Some("cli-id"));
        let mut known = usage("api-id", "api@example.test", false);
        apply_candidate_identity(&mut known, Candidate { access_token: "unused".into(),
            account_id: Some("fallback-id".into()), email: Some("fallback@example.test".into()) });
        assert_eq!(known.email.as_deref(), Some("api@example.test"));
        assert_eq!(known.account_id.as_deref(), Some("api-id"));
    }

    #[test]
    fn desktop_and_distinct_cli_accounts_keep_separate_emails() {
        let data = select_accounts(vec![usage("desk", "desk@example.test", true),
            usage("desk", "desk@example.test", false), usage("cli", "cli@example.test", false)]).unwrap();
        assert_eq!(data.email.as_deref(), Some("desk@example.test"));
        assert_eq!(data.cli.unwrap().email.as_deref(), Some("cli@example.test"));
        let same = select_accounts(vec![usage("a", "a@example.test", true),
            usage("a", "a@example.test", false)]).unwrap();
        assert!(same.cli.is_none());
        let only_cli = select_accounts(vec![usage("cli", "cli@example.test", false)]).unwrap();
        assert!(!only_cli.connected);
        assert_eq!(only_cli.email.as_deref(), Some("cli@example.test"));
        assert!(only_cli.cli.is_none());
    }
}
