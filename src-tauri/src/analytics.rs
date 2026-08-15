// Forecasts, session plans, frozen-provider detection and burn-spike alerts —
// the analytics the Electron build computes in main.js and attaches to the
// usage payload as `forecasts`, `sessionPlans`, `frozenProviders` and
// `burningSeries`.
//
// Without these the renderer degrades quietly rather than breaking: the
// planner line reads "still learning this account's rhythm", no bar catches
// fire, and no burn-spike notification ever fires. Every constant and
// threshold below is carried over unchanged, because they were tuned against
// this user's real history and a "cleaner" number would change when the
// widget shouts at them.

use crate::store::Store;
use chrono::{Datelike, Local, TimeZone, Timelike};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Mutex;

const DEFAULT_REFRESH_SECONDS: i64 = 300;
const MIN_SAMPLE_GAP_MS: i64 = 3 * 60 * 1000;
const SAMPLE_GAP_MULTIPLIER: f64 = 2.5;

const FORECAST_WINDOW_MS: i64 = 6 * 60 * 60 * 1000;
const FORECAST_MIN_SPAN_MS: i64 = 30 * 60 * 1000;
const FORECAST_MAX_HORIZON_MS: i64 = 7 * 24 * 60 * 60 * 1000;

const FROZEN_QUIET_MS: i64 = 72 * 60 * 60 * 1000;
const FROZEN_MIN_COVERAGE_MS: i64 = 6 * 60 * 60 * 1000;

const BURN_WINDOW_MS: i64 = 10 * 60 * 1000;
const BURN_MIN_WINDOW_MS: i64 = 4 * 60 * 1000;
const BURN_COOLDOWN_MS: i64 = 30 * 60 * 1000;
const BURN_MIN_JUMP: f64 = 3.0;
const BURN_FALLBACK_JUMP: f64 = 8.0;
const BURN_MAD_K: f64 = 6.0;
const BURN_SETTLE_MS: i64 = 45 * 60 * 1000;
const BURN_COOLING_MS: i64 = 8 * 60 * 1000;

#[derive(Clone, Copy)]
pub struct Sample {
    pub t: i64,
    pub v: f64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Consecutive samples sit one refresh interval apart, so the largest gap that
/// still counts as "the same session" has to track that interval. A fixed
/// 3-minute gap rejected every pair at the 5-minute default.
fn sample_gap_limit_ms(store: &Store) -> i64 {
    let raw = store.get_or("settings.refreshInterval", json!(DEFAULT_REFRESH_SECONDS));
    let seconds = match &raw {
        Value::String(s) => s.parse::<i64>().unwrap_or(DEFAULT_REFRESH_SECONDS),
        Value::Number(n) => n.as_i64().unwrap_or(DEFAULT_REFRESH_SECONDS),
        _ => DEFAULT_REFRESH_SECONDS,
    };
    let seconds = if seconds > 0 { seconds } else { DEFAULT_REFRESH_SECONDS };
    MIN_SAMPLE_GAP_MS.max((seconds as f64 * 1000.0 * SAMPLE_GAP_MULTIPLIER).round() as i64)
}

/// Pull one named series out of the history records.
fn series(history: &[Value], key: &str) -> Vec<Sample> {
    history
        .iter()
        .filter_map(|e| {
            let t = e.get("timestamp")?.as_i64()?;
            let v = e.get(key)?.as_f64()?;
            if v.is_finite() { Some(Sample { t, v }) } else { None }
        })
        .collect()
}

fn scoped_series(history: &[Value], slug: &str) -> Vec<Sample> {
    history
        .iter()
        .filter_map(|e| {
            let t = e.get("timestamp")?.as_i64()?;
            let v = e.get("scoped")?.get(slug)?.as_f64()?;
            if v.is_finite() { Some(Sample { t, v }) } else { None }
        })
        .collect()
}

fn scoped_slugs(history: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for entry in history {
        if let Some(map) = entry.get("scoped").and_then(|s| s.as_object()) {
            for slug in map.keys() {
                if !out.contains(slug) {
                    out.push(slug.clone());
                }
            }
        }
    }
    out.sort();
    out
}

/// The tail of the series with no gap too large and no value drop. A drop is a
/// window reset, and projecting a slope across one would forecast from a cliff.
fn latest_contiguous_run(samples: &[Sample], max_gap_ms: i64) -> &[Sample] {
    if samples.len() < 2 {
        return samples;
    }
    let mut start = 0;
    for i in 1..samples.len() {
        let dt = samples[i].t - samples[i - 1].t;
        if dt <= 0 || dt > max_gap_ms || samples[i].v < samples[i - 1].v {
            start = i;
        }
    }
    &samples[start..]
}

/// Least-squares projection to 100%, as an ISO timestamp.
fn forecast_series(samples: &[Sample], max_gap_ms: i64) -> Option<String> {
    if samples.len() < 3 {
        return None;
    }
    let win = latest_contiguous_run(samples, max_gap_ms);
    if win.len() < 3 {
        return None;
    }
    let last = win[win.len() - 1];
    if last.v >= 100.0 || last.t - win[0].t < FORECAST_MIN_SPAN_MS {
        return None;
    }
    let t0 = win[0].t;
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
    for s in win {
        let x = (s.t - t0) as f64 / 3_600_000.0; // hours
        sx += x;
        sy += s.v;
        sxx += x * x;
        sxy += x * s.v;
    }
    let n = win.len() as f64;
    let denom = n * sxx - sx * sx;
    if denom == 0.0 {
        return None;
    }
    let slope = (n * sxy - sx * sy) / denom; // percent per hour
    if slope < 0.1 {
        return None; // flat or falling — no meaningful forecast
    }
    let eta_ms = last.t + (((100.0 - last.v) / slope) * 3_600_000.0) as i64;
    if eta_ms - now_ms() > FORECAST_MAX_HORIZON_MS {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(eta_ms).map(|d| d.to_rfc3339())
}

pub fn compute_forecasts(history: &[Value], store: &Store) -> Value {
    let cutoff = now_ms() - FORECAST_WINDOW_MS;
    let recent: Vec<Value> = history
        .iter()
        .filter(|e| e.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0) > cutoff)
        .cloned()
        .collect();
    let gap = sample_gap_limit_ms(store);
    let f = |key: &str| forecast_series(&series(&recent, key), gap).map(Value::String).unwrap_or(Value::Null);

    let mut scoped = Map::new();
    for slug in scoped_slugs(&recent) {
        let value = forecast_series(&scoped_series(&recent, &slug), gap)
            .map(Value::String)
            .unwrap_or(Value::Null);
        scoped.insert(slug, value);
    }

    json!({
        "session": f("session"),
        "weekly": f("weekly"),
        "sonnet": f("sonnet"),
        "opus": f("opus"),
        "cowork": f("cowork"),
        "design": f("design"),
        "oauthApps": f("oauthApps"),
        "scoped": Value::Object(scoped),
        "codex": f("codex"),
        "gemini": f("gemini"),
        "codexCli": f("codexCli"),
        "geminiCli": f("geminiCli"),
        "claudeCli": f("claudeCli"),
    })
}

// ---- session plans ---------------------------------------------------------

fn fmt_plan_hour(h: i64, time_format: &str) -> String {
    let h = ((h % 24) + 24) % 24;
    if time_format == "24h" {
        return format!("{:02}:00", h);
    }
    let ampm = if h >= 12 { "pm" } else { "am" };
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{}{}", h12, ampm)
}

/// The heaviest 5-hour stretch of the day, learned from a week of history.
fn compute_plan_from_series(
    history: &[Value],
    samples: &[Sample],
    store: &Store,
    min_total: f64,
    session_advice: bool,
) -> Value {
    if history.len() < 100 {
        return Value::Null;
    }
    let max_gap = sample_gap_limit_ms(store);
    let mut hourly = [0.0f64; 24];
    for i in 1..samples.len() {
        let dt = samples[i].t - samples[i - 1].t;
        if dt <= 0 || dt > max_gap {
            continue;
        }
        let dv = samples[i].v - samples[i - 1].v;
        if dv <= 0.0 {
            continue;
        }
        // Local hour: the whole point is aligning advice to the user's day.
        let Some(ts) = Local.timestamp_millis_opt(samples[i].t).single() else { continue };
        hourly[ts.hour() as usize] += dv;
    }
    let total: f64 = hourly.iter().sum();
    if total < min_total {
        return Value::Null; // not enough burn to find a pattern
    }
    let (mut best_start, mut best_sum) = (0usize, -1.0f64);
    for s in 0..24 {
        let mut sum = 0.0;
        for k in 0..5 {
            sum += hourly[(s + k) % 24];
        }
        if sum > best_sum {
            best_sum = sum;
            best_start = s;
        }
    }
    if best_sum < total * 0.35 {
        return Value::Null; // usage too evenly spread — no useful peak
    }
    let time_format = store
        .get_or("settings.timeFormat", json!("12h"))
        .as_str()
        .unwrap_or("12h")
        .to_string();
    let share = ((best_sum / total) * 100.0).round() as i64;
    let range = format!(
        "{}–{}",
        fmt_plan_hour(best_start as i64, &time_format),
        fmt_plan_hour(best_start as i64 + 5, &time_format)
    );
    let text = if session_advice {
        format!(
            "Planner: your heaviest hours are {} ({}% of burn) — start a fresh session just before {} to cover them in one 5h window.",
            range, share, fmt_plan_hour(best_start as i64, &time_format)
        )
    } else {
        format!("Planner: your heaviest hours here are {} ({}% of burn).", range, share)
    };
    json!({ "startHour": best_start, "text": text })
}

pub fn compute_session_plans(history: &[Value], store: &Store) -> Value {
    let cutoff = now_ms() - 7 * 24 * 60 * 60 * 1000;
    let recent: Vec<Value> = history
        .iter()
        .filter(|e| e.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0) > cutoff)
        .cloned()
        .collect();
    json!({
        "anthropic": compute_plan_from_series(&recent, &series(&recent, "session"), store, 20.0, true),
        "openai": compute_plan_from_series(&recent, &series(&recent, "codex"), store, 10.0, false),
        "google": compute_plan_from_series(&recent, &series(&recent, "gemini"), store, 10.0, false),
    })
}

// ---- frozen ("on ice") providers -------------------------------------------

fn is_provider_frozen(limits: Option<&Value>, samples: &[Sample]) -> bool {
    let Some(limits) = limits.and_then(|l| l.as_array()) else { return false };
    if limits.is_empty() {
        return false;
    }
    if limits
        .iter()
        .any(|l| l.get("percent").and_then(|p| p.as_f64()).unwrap_or(0.0) > 0.0)
    {
        return false;
    }
    if samples.is_empty() {
        return false;
    }
    if now_ms() - samples[0].t < FROZEN_MIN_COVERAGE_MS {
        return false; // too little history to judge
    }
    let quiet_cutoff = now_ms() - FROZEN_QUIET_MS;
    for i in 1..samples.len() {
        if samples[i].t < quiet_cutoff {
            continue;
        }
        if samples[i].v > samples[i - 1].v {
            return false; // burned recently
        }
    }
    true
}

/// Anthropic is exempt by design — the widget's own login is in active use, and
/// 0% right after a reset would be a false positive.
pub fn compute_frozen_providers(data: &Value, history: &[Value]) -> Value {
    json!({
        "anthropic": false,
        "openai": is_provider_frozen(data.get("codex").and_then(|c| c.get("limits")), &series(history, "codex")),
        "google": is_provider_frozen(data.get("gemini").and_then(|g| g.get("limits")), &series(history, "gemini")),
    })
}

// ---- burn-spike detection --------------------------------------------------

/// seriesKey -> the moment its flames go out.
static BURNING: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);
/// seriesKey -> when it last raised a notification (throttling is separate
/// from the flames, which are live state).
static ALERTED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

pub fn burning_series_map() -> Value {
    let now = now_ms();
    let mut out = Map::new();
    if let Ok(mut guard) = BURNING.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.retain(|_, until| *until > now);
        for key in map.keys() {
            out.insert(key.clone(), json!(true));
        }
    }
    Value::Object(out)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    }
}

fn local_date_string() -> String {
    let now = Local::now();
    format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day())
}

pub struct BurnAlert {
    pub key: String,
    pub body: String,
}

/// Alerts raised but not yet shown. The detector runs deep inside the fetch,
/// which has no AppHandle; the callers that do (the command and the refresh
/// loop) drain this and emit. Keeps the detector pure and testable.
static PENDING: Mutex<Vec<BurnAlert>> = Mutex::new(Vec::new());

pub fn drain_alerts() -> Vec<BurnAlert> {
    PENDING.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// Returns the alerts to raise. Emitting them (notification, sound, webhook) is
/// the caller's job — this stays pure enough to test.
pub fn check_burn_anomalies(history: &[Value], store: &Store) -> Vec<BurnAlert> {
    let mut alerts = vec![];
    if !store.get_or("settings.burnAlerts", json!(true)).as_bool().unwrap_or(true) {
        return alerts;
    }
    if history.len() < 5 {
        return alerts;
    }
    let now = history[history.len() - 1]
        .get("timestamp")
        .and_then(|t| t.as_i64())
        .unwrap_or_else(now_ms);
    let pair_max_gap = sample_gap_limit_ms(store);

    let mut series_list: Vec<(String, String, Vec<Sample>)> = vec![
        ("session", "Anthropic — Session"),
        ("weekly", "Anthropic — Weekly (all models)"),
        ("codex", "OpenAI — Codex weekly"),
        ("gemini", "Google — Gemini daily"),
        ("codexCli", "OpenAI — Codex weekly (CLI account)"),
        ("geminiCli", "Google — Gemini daily (CLI account)"),
        ("claudeCli", "Anthropic — Claude Models 7d (CLI account)"),
    ]
    .into_iter()
    .map(|(k, l)| (k.to_string(), l.to_string(), series(history, k)))
    .collect();

    for slug in scoped_slugs(history) {
        let label = slug
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let samples = scoped_series(history, &slug);
        series_list.push((format!("scoped_{}", slug), format!("Anthropic — {} weekly", label), samples));
    }

    for (key, label, samples) in series_list {
        if samples.len() < 5 {
            continue;
        }
        let window: Vec<&Sample> = samples.iter().filter(|s| s.t >= now - BURN_WINDOW_MS).collect();
        if window.len() < 2 {
            continue;
        }
        let first = window[0];
        let last = window[window.len() - 1];
        let span_ms = last.t - first.t;
        if span_ms < BURN_MIN_WINDOW_MS {
            continue;
        }
        let jump = last.v - first.v;
        if jump < BURN_MIN_JUMP {
            // Clearly settled — start cooling any flames rather than snuffing
            // them, since a pause between prompts is normal mid-session.
            if jump < BURN_MIN_JUMP / 2.0 {
                cool(&key, now + BURN_COOLING_MS);
            }
            continue;
        }

        // Baseline: per-minute rates from pairs OLDER than the window.
        let mut rates = vec![];
        for i in 1..samples.len() {
            if samples[i].t >= now - BURN_WINDOW_MS {
                break;
            }
            let dt = samples[i].t - samples[i - 1].t;
            if dt <= 0 || dt > pair_max_gap {
                continue;
            }
            let dv = samples[i].v - samples[i - 1].v;
            if dv < 0.0 {
                continue; // window reset
            }
            rates.push(dv / (dt as f64 / 60000.0));
        }

        let jump_rate = jump / (span_ms as f64 / 60000.0);
        let (is_anomaly, typical_jump, adaptive_threshold) = if rates.len() >= 50 {
            let med = median(&mut rates.clone());
            let mut deviations: Vec<f64> = rates.iter().map(|r| (r - med).abs()).collect();
            let mad = median(&mut deviations) * 1.4826;
            let threshold = med + BURN_MAD_K * mad.max(0.01);
            (
                jump_rate > threshold,
                Some((med * (BURN_WINDOW_MS as f64 / 60000.0) * 10.0).round() / 10.0),
                Some(threshold),
            )
        } else {
            // Not enough learned baseline yet — conservative absolute floor.
            (jump >= BURN_FALLBACK_JUMP, None, None)
        };

        if is_anomaly {
            ignite(&key, now + BURN_SETTLE_MS);
        } else {
            let settled = match adaptive_threshold {
                Some(t) => jump_rate <= t / 2.0,
                None => jump < BURN_FALLBACK_JUMP / 2.0,
            };
            if settled {
                cool(&key, now + BURN_COOLING_MS);
            }
        }
        if !is_anomaly {
            continue;
        }

        // Notifications are throttled even though the flames are not.
        if let Ok(mut guard) = ALERTED.lock() {
            let map = guard.get_or_insert_with(HashMap::new);
            if let Some(at) = map.get(&key) {
                if now - at < BURN_COOLDOWN_MS {
                    continue;
                }
            }
            map.insert(key.clone(), now);
        }

        let minutes = (span_ms as f64 / 60000.0).round() as i64;
        let typical = typical_jump
            .map(|t| format!(" (typical: ~{}% per 10 min)", t))
            .unwrap_or_default();
        let body = format!(
            "{} jumped {}% in {} min{}. Something may be eating tokens.",
            label,
            jump.round() as i64,
            minutes,
            typical
        );
        let date_key = format!("burnAlerts_{}", local_date_string());
        let count = store.get_or(&date_key, json!(0)).as_i64().unwrap_or(0);
        store.set(&date_key, json!(count + 1));
        alerts.push(BurnAlert { key: key.clone(), body });
    }
    if let Ok(mut queue) = PENDING.lock() {
        for a in &alerts {
            queue.push(BurnAlert { key: a.key.clone(), body: a.body.clone() });
        }
    }
    alerts
}

fn ignite(key: &str, until: i64) {
    if let Ok(mut guard) = BURNING.lock() {
        guard.get_or_insert_with(HashMap::new).insert(key.to_string(), until);
    }
}

fn cool(key: &str, until: i64) {
    if let Ok(mut guard) = BURNING.lock() {
        if let Some(entry) = guard.get_or_insert_with(HashMap::new).get_mut(key) {
            *entry = (*entry).min(until);
        }
    }
}
