// Usage history: one JSON object per line, in per-day files under
// usage-history-v2/<scope>/<YYYY-MM-DD>.jsonl — the same layout and the same
// record shape the Electron build writes, so the chart code (carried over
// unmodified) reads it without translation.
//
// Append-per-sample is deliberate: the widget writes every few minutes for
// weeks, and rewriting one growing document each time would eventually cost a
// multi-megabyte write per sample and risk truncating history on a crash.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

const CHART_DAYS: i64 = 7;
const RETENTION_DAYS: i64 = 8;

fn root() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("imburning-tauri").join("usage-history-v2")
}

/// Capture this before a fetch awaits: a later org switch must not redirect
/// an in-flight sample into another organisation's history.
pub fn scope(store: &crate::store::Store) -> String {
    store.get("organizationId")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(String::from))
        .unwrap_or_else(|| "default".into())
}

fn scope_name(scope: &str) -> String {
    let scope = if scope.is_empty() { "default" } else { scope };
    format!("{:x}", Sha256::digest(scope.as_bytes()))[..32].to_string()
}

fn scope_dir(base: &Path, scope: &str) -> PathBuf {
    // Older Tauri data has no reliable organisation identity. Keep it
    // readable only as default history; never merge it into a named org.
    let legacy = base.join("default");
    if (scope.is_empty() || scope == "default") && legacy.is_dir() {
        legacy
    } else {
        base.join(scope_name(scope))
    }
}

/// Import only the Electron directory for this exact identity. Publish a
/// completed copy so an interrupted import remains retryable on next launch.
pub fn seed_from_electron(scope: &str) {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let source_root = base.join("claude-usage-widget").join("usage-history-v2");
    if let Err(error) = seed_at(&root(), &source_root, scope) {
        crate::log_error(&format!("history import failed: {}", error));
    }
}

fn seed_at(base: &Path, source_root: &Path, scope: &str) -> std::io::Result<()> {
    let dir = scope_dir(base, scope);
    let source = source_root.join(scope_name(scope));
    if dir.exists() || !source.is_dir() { return Ok(()); }
    fs::create_dir_all(base)?;
    let staging = base.join(format!(".import-{}-{:x}", scope_name(scope), rand::random::<u64>()));
    fs::create_dir(&staging)?;
    let result = (|| {
        for file in fs::read_dir(source)? {
            let path = file?.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                if let Some(name) = path.file_name() {
                    fs::copy(&path, staging.join(name))?;
                }
            }
        }
        fs::rename(&staging, &dir)
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staging); }
    result
}

fn day_stamp(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Gemini records its highest pool; Codex records its first pool in Electron.
fn worst(limits: Option<&Value>) -> Option<f64> {
    let arr = limits?.as_array()?;
    arr.iter()
        .filter_map(|l| l.get("percent").and_then(|v| v.as_f64()))
        .fold(None, |acc: Option<f64>, p| Some(acc.map_or(p, |a| a.max(p))))
}

fn first(limits: Option<&Value>) -> Option<f64> {
    limits?.as_array()?.first()?.get("percent")?.as_f64()
}

fn provider_samples(data: &Value) -> [(&'static str, Option<f64>); 4] {
    [
        ("codex", first(data.pointer("/codex/limits"))),
        ("gemini", worst(data.pointer("/gemini/limits"))),
        ("codexCli", first(data.pointer("/codex/cli/limits"))),
        ("geminiCli", worst(data.pointer("/gemini/cli/limits"))),
    ]
}

/// Match Electron's gate: absent reset timestamps and absent primary/secondary
/// provider readings mean this is a dead-session document, not a real zero.
pub fn would_record(data: &Value) -> bool {
    let has_reset = |field: &str| match data.get(field).and_then(|v| v.get("resets_at")) {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::Null) | None => false,
        _ => true,
    };
    has_reset("five_hour") || has_reset("seven_day")
        || provider_samples(data).iter().any(|(_, value)| value.is_some())
}

fn sample(data: &Value, timestamp: i64) -> Option<Value> {
    if !would_record(data) { return None; }
    let mut entry = json!({"timestamp": timestamp, "accountIdentities": account_identities(data)});
    for (key, field) in [
        ("session", "five_hour"), ("weekly", "seven_day"),
        ("sonnet", "seven_day_sonnet"), ("opus", "seven_day_opus"),
        ("cowork", "seven_day_cowork"), ("design", "seven_day_omelette"),
        ("oauthApps", "seven_day_oauth_apps"), ("extraUsage", "extra_usage"),
    ] {
        entry[key] = json!(data.get(field).and_then(|v| v.get("utilization")).and_then(Value::as_f64));
    }
    let mut scoped = serde_json::Map::new();
    if let Some(limits) = data.get("limits").and_then(Value::as_array) {
        let slug_chars = regex::Regex::new("[^a-z0-9]+").expect("literal regex");
        for limit in limits {
            if limit.get("kind").and_then(Value::as_str) != Some("weekly_scoped") { continue; }
            let Some(percent) = limit.get("percent").and_then(Value::as_f64) else { continue };
            let name = limit.pointer("/scope/model/display_name").and_then(Value::as_str).filter(|s| !s.is_empty())
                .or_else(|| limit.pointer("/scope/surface").and_then(Value::as_str).filter(|s| !s.is_empty()))
                .unwrap_or("Scoped");
            let slug = slug_chars.replace_all(&name.to_lowercase(), "_").into_owned();
            scoped.insert(slug, json!(percent));
        }
    }
    if !scoped.is_empty() { entry["scoped"] = Value::Object(scoped); }
    for (key, value) in provider_samples(data) {
        if let Some(value) = value { entry[key] = json!(value); }
    }
    if data.get("claude_code_same_account").and_then(Value::as_bool) == Some(false) {
        if let Some(value) = data.pointer("/claude_code/seven_day/utilization").and_then(Value::as_f64) {
            entry["claudeCli"] = json!(value);
        }
    }
    Some(entry)
}

// Match Electron's account boundaries without storing another copy of emails.
fn account_identities(data: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (key, path) in [("codex", "/codex"), ("codexCli", "/codex/cli"),
        ("gemini", "/gemini"), ("geminiCli", "/gemini/cli")] {
        let Some(account) = data.pointer(path) else { continue };
        if account.get("limits").and_then(Value::as_array).map_or(true, |l| l.is_empty()) { continue; }
        let id = account.get("accountId").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from)
            .or_else(|| account.get("email").and_then(Value::as_str).map(|s| s.trim().to_lowercase()));
        let Some(id) = id.filter(|s| !s.is_empty()) else { continue };
        let identity = json!([id, account.get("connected").and_then(Value::as_bool).unwrap_or(false),
            account.get("source").and_then(Value::as_str).unwrap_or("")]);
        out.insert(key.into(), json!(format!("{:x}", Sha256::digest(identity.to_string().as_bytes()))));
    }
    Value::Object(out)
}

pub fn record(scope: &str, data: &Value) {
    record_at(&root(), scope, data);
}

fn record_at(base: &Path, scope: &str, data: &Value) {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let Some(entry) = sample(data, timestamp) else { return };

    let dir = scope_dir(base, scope);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jsonl", day_stamp(timestamp)));
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", entry);
    }
    prune(&dir);
}

/// Drop day files past the retention window. Cheap because retention is
/// per-FILE: no rewriting, just unlinking whole days.
fn prune(dir: &Path) {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS);
    let cutoff_name = cutoff.format("%Y-%m-%d").to_string();
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        // Filenames are ISO dates, so a lexical compare is a date compare.
        if stem < cutoff_name.as_str() {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Every sample inside the chart window, oldest first.
pub fn read(scope: &str) -> Vec<Value> {
    read_at(&root(), scope)
}

fn read_at(base: &Path, scope: &str) -> Vec<Value> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(CHART_DAYS)).timestamp_millis();
    let mut out = vec![];
    let Ok(entries) = fs::read_dir(scope_dir(base, scope)) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
            if value.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0) > cutoff {
                out.push(value);
            }
        }
    }
    out.sort_by_key(|v| v.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0));
    out
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("codex-history-{:x}", rand::random::<u64>()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    fn sample(dir: &Path, percent: i64) {
        fs::create_dir_all(dir).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        fs::write(dir.join(format!("{}.jsonl", day_stamp(now))),
            format!("{}\n", json!({"timestamp": now, "session": percent}))).unwrap();
    }

    #[test]
    fn scope_names_match_electron_and_org_switches_keep_samples_separate() {
        assert_eq!(scope_name("default"), "37a8eec1ce19687d132fe29051dca629");
        assert_eq!(scope_name("org-a"), "527a4c0a7e943ca74bcc0baba99d5592");
        assert_eq!(scope_name("org-b"), "cc179cf1859e6bfbbeed0194a8a62892");
        let dir = Scratch::new();
        let store = crate::store::Store::in_memory(json!({"organizationId": "org-a"}));
        let started_scope = scope(&store);
        store.set("organizationId", json!("org-b"));
        record_at(&dir.0, &started_scope, &json!({"five_hour": {"utilization": 10, "resets_at": "later"}}));
        record_at(&dir.0, &scope(&store), &json!({"five_hour": {"utilization": 80, "resets_at": "later"}}));
        assert_eq!(read_at(&dir.0, "org-a")[0]["session"], 10.0);
        assert_eq!(read_at(&dir.0, "org-b")[0]["session"], 80.0);
        assert!(read_at(&dir.0, "default").is_empty());
    }

    #[test]
    fn legacy_default_stays_readable_without_entering_a_named_org() {
        let dir = Scratch::new();
        sample(&dir.0.join("default"), 42);
        assert_eq!(read_at(&dir.0, "default")[0]["session"], 42);
        assert_eq!(read_at(&dir.0, "")[0]["session"], 42);
        assert!(read_at(&dir.0, "org-a").is_empty());
        assert_eq!(scope(&crate::store::Store::in_memory(json!({}))), "default");
    }

    #[test]
    fn imports_match_identity_and_do_not_replace_local_history() {
        let dir = Scratch::new();
        let electron = dir.0.join("electron");
        let tauri = dir.0.join("tauri");
        sample(&electron.join(scope_name("org-a")), 10);
        sample(&electron.join(scope_name("org-b")), 90);
        seed_at(&tauri, &electron, "org-a").unwrap();
        assert_eq!(read_at(&tauri, "org-a")[0]["session"], 10);
        assert!(read_at(&tauri, "org-b").is_empty());
        sample(&electron.join(scope_name("org-a")), 99);
        seed_at(&tauri, &electron, "org-a").unwrap();
        assert_eq!(read_at(&tauri, "org-a")[0]["session"], 10);
        seed_at(&tauri, &electron, "missing-org").unwrap();
        assert!(!scope_dir(&tauri, "missing-org").exists());
    }

    #[test]
    fn failed_import_does_not_publish_a_partial_scope_and_can_retry() {
        let dir = Scratch::new();
        let electron = dir.0.join("electron");
        let tauri = dir.0.join("tauri");
        let source = electron.join(scope_name("org-a"));
        sample(&source, 10);
        let broken = source.join("bad.jsonl");
        fs::create_dir(&broken).unwrap();
        assert!(seed_at(&tauri, &electron, "org-a").is_err());
        assert!(!scope_dir(&tauri, "org-a").exists());
        fs::remove_dir(&broken).unwrap();
        seed_at(&tauri, &electron, "org-a").unwrap();
        assert_eq!(read_at(&tauri, "org-a").len(), 1);
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    // JSON represents one numeric type; serde_json distinguishes 1 and 1.0.
    fn numeric_values(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64()),
            Value::Array(a) => Value::Array(a.iter().map(numeric_values).collect()),
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), numeric_values(v))).collect()),
            _ => value.clone(),
        }
    }

    #[test]
    fn records_match_electrons_actual_history_recorder() {
        // Expectations captured by executing storeUsageHistory and its scoped/
        // Gemini helpers from burnwatch 395197b with a stub append and clock.
        let cases: Value = serde_json::from_str(include_str!("../tests/fixtures/history-parity.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let actual = sample(&case["input"], 1788609600000).unwrap_or(Value::Null);
            assert_eq!(numeric_values(&actual), numeric_values(&case["expected"]), "{}", case["name"]);
        }
    }
}
