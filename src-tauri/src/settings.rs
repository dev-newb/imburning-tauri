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
            "banked": { "enabled": true, "path": Value::Null, "volume": 0.85 }
        },
        "hiddenRows": {},
        "hiddenRowsSeeded": {},
        "chartHiddenSeries": {}
    })
}

/// Nested objects merge one level deep; everything else is replaced outright.
const DEEP_MERGE_KEYS: [&str; 3] = ["trayColors", "trayOutline", "sounds"];

pub fn with_defaults(store: &Store) -> Value {
    let mut out = defaults();
    let stored = store.get_or("settings", json!({}));
    let Some(stored) = stored.as_object() else { return out };
    for (key, value) in stored {
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
