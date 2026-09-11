//! Receipts for observed AIKit copies. A snapshot binding is not a live harness
//! or permission grant. Source identity remains in the existing source ground.
use super::file_map::*;
use crate::projectcentral_flow::content_revision_bytes;
use serde_json::{json, Value};
use std::{fs, io, path::Path};
const RECEIPTS: &str = ".central/bkmr/projections.json";
pub(crate) fn execute(all: &[Scope], input: &Value, write: bool) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let path = safe_member(&scope.root, RECEIPTS, false)?;
    let mut state = if path.exists() {
        read_json(&path)?
    } else {
        json!({"schema":"central.file-map-projections/v1","records":[]})
    };
    if state["schema"] != "central.file-map-projections/v1" || !state["records"].is_array() {
        return Err(invalid("Invalid projection receipt document"));
    }
    if write {
        let reference = text(input, "source_ref")?;
        let (_, source) = lookup(all, reference)?;
        if input["source_revision"] != source.revision {
            return Err(conflict("Projection source revision changed"));
        }
        let raw = text(input, "path")?;
        let target = if Path::new(raw).is_absolute() {
            safe_member(Path::new("/"), raw.trim_start_matches('/'), true)?
        } else {
            safe_member(&scope.root, raw, true)?
        };
        let external = !target.starts_with(&scope.root);
        if external && input["allow_external"] != true {
            return Err(invalid(
                "External projection receipt requires allow_external=true",
            ));
        }
        let owner = text(input, "owner")?;
        if fs::metadata(&target)?.len() > crate::source_safety::MAX_SOURCE as u64 {
            return Err(invalid("Projection exceeds bounded source size"));
        }
        let bytes = crate::file_map::payload(&source)?;
        let observed = fs::read(&target)?;
        if bytes != observed || content_revision_bytes(&observed) != source.revision {
            return Err(conflict(
                "Snapshot does not contain the current source bytes",
            ));
        }
        let relative = target
            .strip_prefix(&scope.root)
            .unwrap_or(&target)
            .to_string_lossy()
            .to_string();
        let row = json!({"source_ref":reference,"world_ref":source.world_ref,"source_revision":source.revision,"owner":owner,"path":relative,"external":external,"kind":"immutable-snapshot","live_harness_claim":false});
        let rows = state["records"].as_array_mut().unwrap();
        rows.retain(|r| r["source_ref"] != reference || r["owner"] != owner);
        if rows.len() >= 10000 {
            return Err(invalid("Projection receipts exceed 10000-entry bound"));
        }
        rows.push(row.clone());
        safe_directory(&scope.root, Path::new(".central/bkmr"))?;
        write_atomic(&path, &serde_json::to_vec_pretty(&state)?)?;
        return Ok(row);
    }
    let mut result = Vec::new();
    for row in state["records"].as_array().unwrap() {
        let Some(reference) = row["source_ref"].as_str() else {
            return Err(invalid("Projection receipt lacks SourceRef"));
        };
        // Revoked or missing sources do not disclose old projection locations.
        if let Ok((_, source)) = lookup(all, reference) {
            let mut row = row.clone();
            row["source_current"] = json!(row["source_revision"] == source.revision);
            result.push(row);
        }
    }
    Ok(json!({"records":result}))
}
