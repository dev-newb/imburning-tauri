// The renderer expects a FULLY DEFAULTED settings object, not whatever
// happens to be on disk. It reads fields straight out of the reply
// (settings.sounds.reset.volume, settings.trayColors.session.bg) without
// guarding, so on a profile that has never saved a given key an undefined
// would propagate into the UI. Electron's get-settings materialises every
// default on the way out; this is the same list, kept in the same order so
// the two are diffable.
//
// Defaults are merged ONE LEVEL DEEP for the nested objects (trayColors,
// trayOutline, sounds), matching Electron's `{...DEFAULTS, ...stored}` — a
// stored partial must not erase the sibling defaults.

use crate::store::Store;
use serde_json::{json, Value};

fn defaults() -> Value {
    json!({
        "autoStart": false,
        "minimizeToTray": false,
        "alwaysOnTop": true,
        "theme": "dark",
        "warnThreshold": 75,
        "dangerThreshold": 90,
        "timeFormat": "12h",
        "weeklyDateFormat": "date",
        "usageAlerts": true,
        "compactMode": false,
        "refreshInterval": "300",
        "graphVisible": false,
        "expandedOpen": true,
        "openaiExtrasOpen": true,
        "projectionsOn": true,
        "showTrayStats": false,
        "showClaudeCode": true,
        "trayColors": {
            "session": { "bg": "#3b82f6", "text": "#000000" },
            "weekly":  { "bg": "#3b82f6", "text": "#ffffff" },
            "fable":   { "bg": "#ef4444", "text": "#000000" },
            "codex":   { "bg": "#10a37f", "text": "#ffffff" },
            "gemini":  { "bg": "#f4b400", "text": "#000000" }
        },
        "trayOutline": { "enabled": true, "color": "#facc15" },
        "burnAlerts": true,
        "fontColor": { "enabled": false, "color": "#e0e0e0" },
        "webhook": { "enabled": false, "url": "" },
        "dailyDigest": true,
        "showCodex": true,
        "showCodexCli": true,
        "showGemini": true,
        "showGeminiCli": true,
        "googleSource": "auto",
        "trayOpenai": false,
        "trayGoogle": false,
        "sectionCollapsed": {},
        "subgroupHidden": {},
        "pizazz": true,
        "sortByUsage": false,
        "hideAccountEmails": false,
        "flameStyle": "classic",
        "sounds": {
            "reset":  { "enabled": true, "path": Value::Null, "volume": 0.85 },
            "burn":   { "enabled": true, "path": Value::Null, "volume": 0.85 },
            "banked": { "enabled": true, "path": Value::Null, "volume": 0.85 },
            "wall":   { "enabled": true, "path": Value::Null, "volume": 0.85 }
        },
        "hiddenProviders": {},
        "hiddenRows": {},
        "hiddenRowsSeeded": {},
        "chartHiddenSeries": {}
    })
}

/// Nested objects merge one level deep; everything else is replaced outright.
const DEEP_MERGE_KEYS: [&str; 3] = ["trayColors", "trayOutline", "sounds"];

pub fn with_defaults(store: &Store) -> Value {
    let mut out = defaults();
    out["cliAdopted"] = cli_adopted(store);
    let stored = store.get_or("settings", json!({}));
    let Some(stored) = stored.as_object() else { return out };
    for (key, value) in stored {
        if key == "cliAdopted" { continue; }
        if DEEP_MERGE_KEYS.contains(&key.as_str()) {
            if let (Some(base), Some(over)) = (
                out.get_mut(key).and_then(|v| v.as_object_mut()),
                value.as_object(),
            ) {
                for (k, v) in over {
                    base.insert(k.clone(), v.clone());
                }
                continue;
            }
        }
        out[key] = value.clone();
    }
    out
}

/// Adoption belongs to its dedicated command, never a stale settings form.
pub fn merge_saved_settings(stored: Value, updates: &Value) -> Value {
    let mut merged = stored.as_object().cloned().unwrap_or_default();
    if let Some(updates) = updates.as_object() {
        for (key, value) in updates {
            if key != "cliAdopted" {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(merged)
}

pub fn cli_adopted(store: &Store) -> Value {
    let saved = store.get("settings.cliAdopted").filter(Value::is_object);
    let initialized = store.get("cliAdoptionInitialized").and_then(|v| v.as_bool()) == Some(true);
    if !initialized {
        let existing = store.get("latestUsageData").is_some();
        let adopted = saved.unwrap_or_else(|| json!({
            "anthropic": existing, "openai": existing, "google": existing
        }));
        store.set("settings.cliAdopted", adopted.clone());
        store.set("cliAdoptionInitialized", json!(true));
        return adopted;
    }
    saved.unwrap_or_else(|| json!({ "anthropic": false, "openai": false, "google": false }))
}

pub fn refresh_interval_seconds(value: &Value) -> u64 {
    value.as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(300)
        .clamp(15, 3600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_intervals_accept_ui_strings_and_numeric_seconds() {
        for seconds in [15, 30, 60, 120, 300] {
            assert_eq!(refresh_interval_seconds(&json!(seconds)), seconds);
            assert_eq!(refresh_interval_seconds(&json!(seconds.to_string())), seconds);
        }
        assert_eq!(refresh_interval_seconds(&json!(0)), 15);
        assert_eq!(refresh_interval_seconds(&json!(9000)), 3600);
        for invalid in [Value::Null, json!("bad"), json!(-1), json!(1.5)] {
            assert_eq!(refresh_interval_seconds(&invalid), 300);
        }
    }

    #[test]
    fn stale_and_partial_settings_never_overwrite_adoption() {
        let store = Store::in_memory(json!({"settings": {
            "cliAdopted": {"openai": true}, "theme": "dark", "hiddenRows": {"x": true}
        }}));
        let stale = with_defaults(&store);
        store.set("settings.cliAdopted.openai", json!(false));
        store.set("settings", merge_saved_settings(store.get_or("settings", json!({})), &stale));
        assert_eq!(cli_adopted(&store)["openai"], false);
        store.set("settings", merge_saved_settings(store.get_or("settings", json!({})), &json!({"theme":"light"})));
        assert_eq!(with_defaults(&store)["cliAdopted"]["openai"], false);
        assert_eq!(store.get("settings.hiddenRows.x"), Some(json!(true)));
    }

    #[test]
    fn grandfathering_runs_once_and_preserves_existing_choices() {
        let old = Store::in_memory(json!({"latestUsageData": {}}));
        assert_eq!(cli_adopted(&old)["openai"], true);
        old.set("settings.cliAdopted", Value::Null);
        assert_eq!(cli_adopted(&old)["openai"], false);
        assert_eq!(with_defaults(&old)["cliAdopted"]["openai"], false);
        let fresh = Store::in_memory(json!({}));
        assert_eq!(cli_adopted(&fresh)["openai"], false);
        fresh.set("latestUsageData", json!({}));
        fresh.set("settings.cliAdopted", Value::Null);
        assert_eq!(cli_adopted(&fresh)["openai"], false);
        let declined = Store::in_memory(json!({"latestUsageData":{},"settings":{"cliAdopted":{"openai":false}}}));
        assert_eq!(cli_adopted(&declined)["openai"], false);
    }
}
