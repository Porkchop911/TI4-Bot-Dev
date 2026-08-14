//! Merge two Stage-1 checkpoints into one combined checkpoint with all six factions.

use sha2::Digest;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let hjn_path = PathBuf::from("out/stage1_hjn_solved.json");
    let xls_path = PathBuf::from("out/stage1_xls_solved.json");
    let out_path = PathBuf::from("out/stage1_all_six.json");

    // Load both checkpoints
    let hjn: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&hjn_path).map_err(|e| format!("read {}: {}", hjn_path.display(), e))?,
    )
    .map_err(|e| format!("parse {}: {}", hjn_path.display(), e))?;

    let xls: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&xls_path).map_err(|e| format!("read {}: {}", xls_path.display(), e))?,
    )
    .map_err(|e| format!("parse {}: {}", xls_path.display(), e))?;

    // Extract profiles from both
    let hjn_profiles = hjn
        .get("profiles")
        .or_else(|| hjn.get("accepted"))
        .unwrap_or(&hjn);
    let xls_profiles = xls
        .get("profiles")
        .or_else(|| xls.get("accepted"))
        .unwrap_or(&xls);

    // Collect all profiles into a BTreeMap for deterministic ordering
    let mut all_profiles: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    if let Some(obj) = hjn_profiles.as_object() {
        for (k, v) in obj {
            all_profiles.insert(k.clone(), v.clone());
        }
    }
    if let Some(obj) = xls_profiles.as_object() {
        for (k, v) in obj {
            all_profiles.insert(k.clone(), v.clone());
        }
    }

    // Build profiles and accepted as serde_json::Map
    let profiles_map: serde_json::Map<String, serde_json::Value> = all_profiles
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let accepted_map: serde_json::Map<String, serde_json::Value> = all_profiles
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Build combined checkpoint
    let merged = serde_json::json!({
        "schema": 1,
        "trainer": "rust_stage1_policy_gradient_all_six",
        "stage": 1,
        "horizon": {"rounds": 1, "steps": 500_000},
        "arguments": {
            "factions": "sol,letnev,xxcha,hacan,jolnar,l1z1x"
        },
        "resumed_from": null,
        "final_update": 2500,
        "run_complete": true,
        "profiles": profiles_map,
        "accepted": accepted_map,
        "history": [],
        "training_telemetry": [],
        "checkpoint_archive": {},
    });

    let output = serde_json::to_string_pretty(&merged).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&out_path, &output)
        .map_err(|e| format!("write {}: {}", out_path.display(), e))?;

    // SHA-256 companion
    let digest = sha2::Sha256::digest(output.as_bytes());
    std::fs::write(
        out_path.with_extension("json.sha256"),
        format!("{digest:x}"),
    )
    .map_err(|e| format!("write checksum: {e}"))?;

    // Report factions found
    let faction_count = all_profiles.len();
    println!(
        "Merged {} factions into {}",
        faction_count,
        out_path.display()
    );
    for faction in all_profiles.keys() {
        println!("  - {faction}");
    }
    Ok(())
}
