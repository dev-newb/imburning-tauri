// Parity guard: the Rust normalizer must produce the same rows as the JS one
// in the Electron build. The fixture is a real captured fetchAvailableModels
// response; the expectation is the JS output for that same fixture.
//
// If these ever disagree the two builds show the user different numbers, which
// is the one failure mode a shared renderer cannot paper over.

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

    assert_eq!(out.foreign.len(), 1, "Claude + GPT-OSS share one pool");
    assert_eq!(out.foreign[0].key, "m_non_gemini_models");
    assert_eq!(out.foreign[0].label, "Non-Gemini models");
    assert_eq!(out.foreign[0].percent, 0.0);
    assert_eq!(out.foreign[0].resets_at.as_deref(), Some("2026-08-12T16:15:36Z"));
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
