// Disk format shared with Electron's src/alert-events.js. No account identifiers
// are written: event identities are SHA-256 hashes, and diagnostics are numeric.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, time::Duration};
const COOLDOWN_MS: i64 = 600_000;

pub fn record(root: &Path, app: &str, request: &Value, now: i64) -> std::io::Result<Value> {
    let kind = request["kind"].as_str().unwrap_or("");
    let phase = request["phase"].as_str().unwrap_or("");
    if !["reset", "banked", "wall", "burn"].contains(&kind)
        || !["claim", "played", "failed", "disabled", "preview"].contains(&phase) {
        return Ok(json!({"play": false}));
    }
    let mut events = Vec::new();
    for e in request["events"].as_array().into_iter().flatten().take(32) {
        let Some(key) = e["key"].as_str().filter(|s| s.chars().count() <= 2048) else { continue };
        let scoped_key = if e["shared"] == false { format!("{app}:{key}") } else { key.to_string() };
        let reason = e["reason"].as_str().unwrap_or("");
        let mut event = json!({
            "id": format!("{:x}", Sha256::digest(scoped_key.as_bytes())),
            "pool": e["pool"].as_str().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(100).collect::<String>(),
            "reason": if ["scheduled", "early", "credit-increase"].contains(&reason) { reason } else { "unknown" },
        });
        for field in ["from", "to", "previousResetAt", "resetAt", "observedAt"] {
            event[field] = if e[field].is_number() { e[field].clone() } else { Value::Null };
        }
        events.push(event);
    }
    fs::create_dir_all(root)?;
    let lock = root.join("lock");
    let mut acquired = false;
    for _ in 0..100 {
        match fs::create_dir(&lock) {
            Ok(()) => { acquired = true; break; }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::metadata(&lock).and_then(|m| m.modified()).ok()
                    .and_then(|t| t.elapsed().ok()).is_some_and(|d| d > Duration::from_secs(60)) {
                    let _ = fs::remove_dir(&lock);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
    if !acquired { return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "Sound event ledger is busy")); }
    let result = (|| {
        let file = root.join("claims.json");
        let mut ledger = fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.as_object().cloned()).unwrap_or_default();
        ledger.retain(|_, v| v["at"].as_i64().is_some_and(|at| now - at < COOLDOWN_MS && at <= now + COOLDOWN_MS));
        let mut play = false;
        for event in &mut events {
            let id = event["id"].as_str().unwrap().to_string();
            if phase == "claim" {
                event["decision"] = json!(if ledger.contains_key(&id) { "duplicate" } else { "claimed" });
                if !ledger.contains_key(&id) { ledger.insert(id, json!({"at": now, "app": app})); play = true; }
            } else if phase == "failed" && ledger.get(&id).and_then(|v| v["app"].as_str()) == Some(app) {
                ledger.remove(&id);
            }
        }
        let tmp = root.join("claims.json.tmp");
        fs::write(&tmp, serde_json::to_vec(&ledger)?)?;
        fs::rename(tmp, file)?;
        let log = root.join("events.jsonl");
        if fs::metadata(&log).is_ok_and(|m| m.len() >= 1024 * 1024) {
            let backup = root.join("events.jsonl.1");
            if backup.exists() { fs::remove_file(&backup)?; }
            fs::rename(&log, backup)?;
        }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(log)?;
        writeln!(f, "{}", json!({"at": now, "app": app, "kind": kind, "phase": phase, "events": events}))?;
        Ok(json!({"play": play}))
    })();
    let _ = fs::remove_dir(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_apps_claim_once_and_failed_playback_releases_its_claim() {
        let root = std::env::temp_dir().join(format!("imburning-alert-test-{}", rand::random::<u64>()));
        let request = json!({"kind":"reset","phase":"claim","events":[{"key":"private@example.test","pool":"codex_weekly","reason":"scheduled","from":90,"to":0}]});
        let handles: Vec<_> = ["electron", "tauri"].into_iter().map(|app| {
            let root = root.clone(); let request = request.clone();
            std::thread::spawn(move || (app, record(&root, app, &request, 1000000).unwrap()["play"] == true))
        }).collect();
        let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|(_, play)| *play).count(), 1);
        let owner = outcomes.iter().find(|(_, play)| *play).unwrap().0;
        let mut failed = request.clone(); failed["phase"] = json!("failed");
        record(&root, owner, &failed, 1000001).unwrap();
        assert_eq!(record(&root, "tauri", &request, 1000002).unwrap()["play"], true);
        assert_eq!(record(&root, "electron", &request, 1600003).unwrap()["play"], true);
        assert!(!fs::read_to_string(root.join("events.jsonl")).unwrap().contains("private@example.test"));
        fs::remove_dir_all(root).unwrap();
    }
}
