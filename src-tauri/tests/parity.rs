// Parity guard: the Rust normalizer must produce the same rows as the JS one
// in the Electron build. The fixture is a real captured fetchAvailableModels
// response; the expectation is the JS output for that same fixture.
//
// If these ever disagree the two builds show the user different numbers, which
// is the one failure mode a shared renderer cannot paper over.

// The test binary is its own crate root, so the modules the code under test
// refers to as `crate::…` must be declared here too.
#[path = "../src/store.rs"]
mod store;
#[path = "../src/providers/mod.rs"]
mod providers;

#[test]
fn matches_js_normalizer() {
    let raw = include_str!("fixtures/ag-models.json");
    let json: serde_json::Value = serde_json::from_str(raw).expect("fixture parses");
    let out = providers::antigravity::normalize(&json).expect("normalizes");

    assert_eq!(out.limits.len(), 1, "Gemini pools collapse to one row");
    assert_eq!(out.limits[0].key, "m_gemini_all_models");
    assert_eq!(out.limits[0].label, "Gemini (all models)");
    assert_eq!(out.limits[0].percent, 1.5);
    assert_eq!(out.limits[0].resets_at.as_deref(), Some("2026-08-12T12:09:53Z"));

    // Claude and GPT-OSS are metered by the same login but are not Google's
    // models: they must not reach the Google section in any form.
    assert!(
        !out.limits.iter().any(|l| {
            let l = l.label.to_lowercase();
            l.contains("claude") || l.contains("gpt") || l.contains("non-gemini")
        }),
        "no non-Google pool may appear in the Google rows: {:?}",
        out.limits.iter().map(|l| &l.label).collect::<Vec<_>>()
    );
}

#[test]
fn diverging_quotas_split_back_into_rows() {
    let raw = include_str!("fixtures/ag-models.json");
    let mut json: serde_json::Value = serde_json::from_str(raw).unwrap();
    json["models"]["gemini-3.1-pro-high"]["quotaInfo"]["remainingFraction"] =
        serde_json::json!(0.5);
    let out = providers::antigravity::normalize(&json).unwrap();
    assert_eq!(out.limits.len(), 2, "a split allowance must draw two bars");
    // Pro sorts first, and carries the diverged figure.
    assert!(out.limits.iter().any(|l| l.percent == 50.0 && l.label.contains("Pro")));
}

// --- history write gate -----------------------------------------------------
// Electron refuses to record a sample when the Anthropic windows carry no
// reset timestamps and no provider reported a figure: that combination means
// the API returned zeroed data (dead session, removed device), and recording
// it plots a phantom drop to zero that every forecast then builds on.

#[path = "../src/history.rs"]
mod history;

#[test]
fn dead_session_with_no_provider_sample_is_not_recorded() {
    // utilization present but resets_at absent = the zeroed-data case.
    let dead = serde_json::json!({
        "five_hour": { "utilization": 0 },
        "seven_day": { "utilization": 0 }
    });
    assert!(!history::would_record(&dead), "a dead session must not be written");
}

#[test]
fn a_provider_sample_alone_is_enough_to_record() {
    let google_only = serde_json::json!({
        "five_hour": serde_json::Value::Null,
        "gemini": { "limits": [{ "key": "m_x", "label": "X", "percent": 12.0 }] }
    });
    assert!(history::would_record(&google_only), "Google-only usage is still history");
}

#[test]
fn a_live_session_records() {
    let live = serde_json::json!({
        "five_hour": { "utilization": 65, "resets_at": "2026-08-12T12:10:00Z" }
    });
    assert!(history::would_record(&live));
}
