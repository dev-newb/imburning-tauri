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
use std::path::PathBuf;

const CHART_DAYS: i64 = 7;
const RETENTION_DAYS: i64 = 8;

fn root() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("imburning-tauri").join("usage-history-v2")
}

/// The Electron build keys its scope directory by a hash we do not reproduce;
/// this build has no org login, so it writes one plain "default" scope.
fn scope_dir() -> PathBuf {
    root().join("default")
}

/// First run: adopt the Electron build's history so the chart opens with real
/// data instead of a blank week. Only the most recently written scope is
/// copied — merging every scope would double-count days an account appears in
/// twice. Any failure just means an empty chart, which is what a first run
/// would show anyway.
pub fn seed_from_electron() {
    let dir = scope_dir();
    if dir.exists() {
        return;
    }
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let source_root = base.join("claude-usage-widget").join("usage-history-v2");
    let Ok(entries) = fs::read_dir(&source_root) else { return };
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        let Some(modified) = modified else { continue };
        if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            newest = Some((modified, path));
        }
    }
    let Some((_, source)) = newest else { return };
    let Ok(files) = fs::read_dir(&source) else { return };
    for file in files.flatten() {
        let path = file.path();
        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
            if let Some(name) = path.file_name() {
                let _ = fs::copy(&path, dir.join(name));
            }
        }
    }
}

fn day_stamp(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Worst (highest) percent across a provider's pools — one series per
/// provider, matching what the Electron build records.
fn worst(limits: Option<&Value>) -> Option<f64> {
    let arr = limits?.as_array()?;
    arr.iter()
        .filter_map(|l| l.get("percent").and_then(|v| v.as_f64()))
        .fold(None, |acc: Option<f64>, p| Some(acc.map_or(p, |a| a.max(p))))
}

pub fn record(data: &Value) {
    let session = data.get("five_hour").and_then(|v| v.get("utilization")).and_then(|v| v.as_f64());
    let weekly = data.get("seven_day").and_then(|v| v.get("utilization")).and_then(|v| v.as_f64());
    let codex = worst(data.get("codex").and_then(|c| c.get("limits")));
    let gemini = worst(data.get("gemini").and_then(|g| g.get("limits")));

    // Nothing worth plotting — a dead session reports no windows and no pools.
    if session.is_none() && weekly.is_none() && codex.is_none() && gemini.is_none() {
        return;
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let entry = json!({
        "timestamp": timestamp,
        "session": session,
        "weekly": weekly,
        "sonnet": Value::Null,
        "opus": Value::Null,
        "cowork": Value::Null,
        "design": Value::Null,
        "oauthApps": Value::Null,
        "extraUsage": Value::Null,
        "scoped": {},
        "codex": codex,
        "gemini": gemini,
    });

    let dir = scope_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jsonl", day_stamp(timestamp)));
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", entry);
    }
    prune();
}

/// Drop day files past the retention window. Cheap because retention is
/// per-FILE: no rewriting, just unlinking whole days.
fn prune() {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS);
    let cutoff_name = cutoff.format("%Y-%m-%d").to_string();
    let Ok(entries) = fs::read_dir(scope_dir()) else { return };
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
pub fn read() -> Vec<Value> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(CHART_DAYS)).timestamp_millis();
    let mut out = vec![];
    let Ok(entries) = fs::read_dir(scope_dir()) else { return out };
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
