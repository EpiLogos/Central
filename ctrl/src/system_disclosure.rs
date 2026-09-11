//! Wave 5 System contribution: Central's native `oi.product-settings-disclosure/v2`
//! reading. One read-only Action (`central.system`) composes the descriptor from
//! Central's own live seams and returns it as a single document.
//!
//! The whole point of this surface is that authored intent and effective/observed
//! state stay distinguishable:
//!
//! - `declared`   — what the human authored (Control/user, governance, machine
//!                  declarations, skill manifests, accepted source relations).
//! - `effective`  — what Central actually resolved (the active root, recognised
//!                  source standing, resolved skills).
//! - `active`     — what is observed/materialised right now (doctor output, NOW
//!                  field state, wiki presence, disk proposal counts). These are
//!                  observations; they never become `declared`.
//! - `staged`     — proposals awaiting a human decision (source returns, generated
//!                  proposals). Never authored.
//! - `expected_effect` — what applying a stage would do, disclosed before invoke.
//!
//! Central is deliberately *not* turned into a preferences schema here: no
//! settings database, no duplicate Action catalogue, no product-config semantics.
//! The Action field is referenced through `action.list`; the disclosed `actions`
//! array carries only the engagement seam (authority/exposure/explain/history).

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::AGENT_RETRIEVAL_DENY_MARKER;
use crate::control_skills::inspect_control_skills;
use crate::machine::read_machine_declaration;
use crate::projectcentral_ground::inspect_project_ground;
use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, resolve_central_root, CentralHealth, RootOptions};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SYSTEM_DISCLOSURE_SCHEMA: &str = "oi.product-settings-disclosure/v2";
pub const SYSTEM_CONTRACT_REVISION: &str = "wave-5/system.1";
pub const SYSTEM_PRODUCT_ID: &str = "central";
/// The stable ref into Central's own namespace for the root register.
pub const SYSTEM_OWNER_REF: &str = "control:root";
/// The argv the O:I composition kernel runs to obtain this reading.
pub const SYSTEM_READING_COMMAND: [&str; 3] = ["ctrl", "system", "--json"];
pub const SYSTEM_ACTION_ID: &str = "central.system";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// SHA-256 (pure Rust; no external hash dependency is accepted in ctrl).
// Verified against the FIPS 180-4 test vectors in the unit tests below.
// ---------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    // Padding: 0x80, then zeros, then 64-bit big-endian bit length.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// Recursively zero every `*_unix_ms` field so the digest is time-independent.
fn zero_timestamps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key.ends_with("_unix_ms") {
                    *child = json!(0);
                } else {
                    zero_timestamps(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                zero_timestamps(item);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Provenance / axis helpers. `observed` is the single reading's observation
// moment, captured once so every axis shares one freshness stamp.
// ---------------------------------------------------------------------------

fn prov(owner_ref: &str, path: &str, observed: u64) -> Value {
    json!({
        "owner_ref": owner_ref,
        "path": path,
        "observed_at_unix_ms": observed,
    })
}

fn declared_axis(value: Value, path: &str, observed: u64) -> Value {
    json!({"value": value, "provenance": prov(SYSTEM_OWNER_REF, path, observed)})
}

fn effective_axis(value: Value, path: &str, observed: u64) -> Value {
    json!({"value": value, "provenance": prov(SYSTEM_OWNER_REF, path, observed)})
}

fn active_axis(value: Value, path: &str, materialisation_ref: &str, observed: u64) -> Value {
    json!({
        "value": value,
        "provenance": prov(SYSTEM_OWNER_REF, path, observed),
        "materialisation_ref": materialisation_ref,
    })
}

fn staged_axis(value: Value, stage_ref: Option<&str>, stage_state: &str, observed: u64) -> Value {
    json!({
        "value": value,
        "provenance": prov(SYSTEM_OWNER_REF, ".central/source-returns", observed),
        "stage_ref": stage_ref,
        "stage_state": stage_state,
    })
}

fn expected_effect(summary: &str, ref_: &str) -> Value {
    json!({"summary": summary, "ref": ref_})
}

fn drift(state: &str, between: &[&str], remediation: Option<&str>) -> Value {
    json!({"state": state, "between": between, "remediation_action_ref": remediation})
}

// ---------------------------------------------------------------------------
// Bounded filesystem observations (read-only, never authored).
// ---------------------------------------------------------------------------

/// Bounded recursive count of regular files under a directory (never follows symlinks).
fn count_files(root: &Path, max: usize) -> usize {
    let mut total = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if total >= max {
            return max;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                total += 1;
                if total >= max {
                    return max;
                }
            }
        }
    }
    total
}

/// Bounded scan for `.no-agent-retrieval` markers under a directory.
fn find_deny_markers(root: &Path, max_depth: usize) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(dir: &Path, depth: usize, max_depth: usize, found: &mut Vec<String>) {
        if depth > max_depth {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if entry.file_name() == AGENT_RETRIEVAL_DENY_MARKER && ft.is_file() {
                found.push(entry.path().to_string_lossy().into_owned());
            } else if ft.is_dir() {
                walk(&entry.path(), depth + 1, max_depth, found);
            }
        }
    }
    walk(root, 0, max_depth, &mut found);
    found.sort();
    found
}

fn list_machine_roles(root: &Path) -> Vec<(String, Value)> {
    let dir = root.join("Control/machines");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut roles = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        let role = name.trim_end_matches(".json").to_owned();
        let declaration = read_machine_declaration(root, &role)
            .map(|d| serde_json::to_value(&d.declaration).unwrap_or(Value::Null))
            .unwrap_or_else(|e| json!({"error": e.code}));
        roles.push((role, declaration));
    }
    roles.sort_by(|a, b| a.0.cmp(&b.0));
    roles
}

fn list_projects(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("Work")) else {
        return Vec::new();
    };
    let mut projects = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| e.path().join("ProjectCentral").is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    projects.sort();
    projects
}

/// Scan every project's `.central/source-returns` store for proposal status counts.
fn scan_proposals(root: &Path) -> (usize, usize, usize, usize) {
    let mut pending = 0usize;
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut total = 0usize;
    for project in list_projects(root) {
        let dir = root
            .join("Work")
            .join(&project)
            .join(".central/source-returns");
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("return-") || !name.ends_with(".json") {
                continue;
            }
            total += 1;
            if let Ok(text) = fs::read_to_string(entry.path()) {
                let status = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v.get("status").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_else(|| "unknown".to_owned());
                match status.as_str() {
                    "pending" | "applying" => pending += 1,
                    "accepted" => accepted += 1,
                    "rejected" => rejected += 1,
                    _ => {}
                }
            }
        }
    }
    (total, pending, accepted, rejected)
}

// ---------------------------------------------------------------------------
// Descriptor assembly
// ---------------------------------------------------------------------------

fn settings_axes_for_ground(
    root: &Path,
    health: &crate::root::CentralHealth,
    resolved_source: &str,
    projects: &[String],
    observed: u64,
) -> Value {
    let control_user_files = count_files(&root.join("Control/user"), 500);
    let governance_files = count_files(&root.join("Control/agents/governance"), 500);
    let wiki_present = root.join("Control/agents/wiki/wiki.json").is_file();

    let declared = json!({
        "register": "control:root",
        "authored_apertures": {
            "control_user_files": control_user_files,
            "governance_files": governance_files,
            "wiki_source_present": wiki_present,
        },
        "project_registers": projects,
    });
    let effective = json!({
        "resolved_root": health.root.to_string_lossy(),
        "source": resolved_source,
        "projectcentral_ready_projects": projects.len(),
    });
    let active = json!({
        "root_state": health.root_state,
        "valid": health.valid,
        "checks": health.checks.iter().map(|c| json!({"path": c.path, "valid": c.valid})).collect::<Vec<_>>(),
        "mixed_root_detected": health.mixed_root.detected,
    });

    json!({
        "declared": declared_axis(declared, "Control/", observed),
        "effective": effective_axis(effective, &health.root.to_string_lossy(), observed),
        "active": active_axis(active, &health.root.to_string_lossy(), "central.doctor", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("No staged ground change is prepared.", "central.system"),
    })
}

fn settings_axes_for_self_description(root: &Path, observed: u64) -> Value {
    let user_files = count_files(&root.join("Control/user"), 500);
    let wiki_present = root.join("Control/agents/wiki/wiki.json").is_file();
    let declared = json!({
        "authored_self_description_present": user_files > 0,
        "source": "Control/user (human-authored prose; body not disclosed here)",
        "file_count": user_files,
    });
    let effective = json!({
        "recognised_standing": if user_files > 0 { "human-authored aperture present" } else { "empty" },
        "disclosed_here": false,
    });
    let active = json!({
        "agent_maintained_wiki_present": wiki_present,
        "note": "wiki.json is agent-maintained knowledge, never authored self-description; it is observed, not declared.",
    });
    json!({
        "declared": declared_axis(declared, "Control/user/", observed),
        "effective": effective_axis(effective, "Control/user/", observed),
        "active": active_axis(active, "Control/agents/wiki/wiki.json", "central.wiki.read", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("No staged self-description change is prepared.", "central.system"),
    })
}

fn settings_axes_for_machine_intent(root: &Path, observed: u64) -> Value {
    let roles = list_machine_roles(root);
    let declared = json!({
        "roles": roles.iter().map(|(r, _)| r).collect::<Vec<_>>(),
        "declaration_count": roles.len(),
    });
    let effective = json!({
        "resolved_declarations": roles
            .iter()
            .map(|(r, d)| json!({"role": r, "capabilities": d.get("capabilities").cloned().unwrap_or_else(|| json!([]))}))
            .collect::<Vec<_>>(),
    });
    let active = json!({
        "observed_via": "machine.inspect (native Action; observation is not folded into declared intent)",
        "observed_here": false,
    });
    json!({
        "declared": declared_axis(declared, "Control/machines/", observed),
        "effective": effective_axis(effective, "Control/machines/", observed),
        "active": active_axis(active, "Control/machines/", "machine.inspect", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("machine.plan/machine.apply reconcile declared intent with fresh observation.", "machine.plan"),
    })
}

fn settings_axes_for_skills(root: &Path, observed: u64) -> Value {
    let inspection = match inspect_control_skills(root) {
        Ok(i) => i,
        Err(e) => {
            return json!({
                "declared": declared_axis(json!(null), "Control/user/skills", observed),
                "effective": effective_axis(json!({"error": e.to_string()}), "Control/user/skills", observed),
                "active": active_axis(json!({"error": e.to_string()}), "Control/user/skills", "control.skills.inspect", observed),
                "staged": staged_axis(json!(null), None, "none", observed),
                "expected_effect": expected_effect("No staged skill change is prepared.", "central.system"),
            })
        }
    };
    let authored = inspection
        .skills
        .iter()
        .filter(|s| matches!(s.provenance.as_str(), "human-authored" | "adopted"))
        .map(|s| json!({"name": s.name, "scope": s.scope, "standing": s.standing, "provenance": s.provenance}))
        .collect::<Vec<_>>();
    let unresolved = inspection
        .skills
        .iter()
        .filter(|s| s.standing == "unresolved")
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    let declared = json!({
        "authored_skills": authored,
        "authored_count": authored.len(),
    });
    let effective = json!({
        "active_skills": inspection.active_skills,
        "retired_skills": inspection.retired_skills,
        "unresolved_skills": inspection.unresolved_skills,
    });
    let active = json!({
        "scopes": inspection.scopes.iter().map(|s| json!({"scope": s.scope, "path": s.path, "exists": s.exists})).collect::<Vec<_>>(),
        "unresolved_names": unresolved,
    });
    json!({
        "declared": declared_axis(declared, "Control/user/skills", observed),
        "effective": effective_axis(effective, "Control/user/skills", observed),
        "active": active_axis(active, "Control/user/skills", "control.skills.inspect", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("control.skills.retire / control.skills.restore change a skill's standing in its manifest.", "control.skills.retire"),
    })
}

fn settings_axes_for_privacy(root: &Path, observed: u64) -> Value {
    let control_markers = find_deny_markers(&root.join("Control"), 6);
    let project_markers = find_deny_markers(&root.join("Work"), 8)
        .into_iter()
        .filter(|p| p.contains("ProjectCentral") || p.contains("/.no-agent-retrieval"))
        .collect::<Vec<_>>();
    let declared = json!({
        "marker": AGENT_RETRIEVAL_DENY_MARKER,
        "control_markers": control_markers.len(),
        "project_markers": project_markers.len(),
        "note": "deny markers are authored retrieval exclusions; their presence is disclosed, never the excluded content.",
    });
    let effective = json!({
        "retrieval_rule": "subtree denied by .no-agent-retrieval marker (role-based, not magic-path)",
        "markers_enforced": control_markers.len() + project_markers.len(),
    });
    let active = json!({
        "markers_observed": control_markers.len() + project_markers.len(),
    });
    json!({
        "declared": declared_axis(declared, "Control/", observed),
        "effective": effective_axis(effective, "Control/", observed),
        "active": active_axis(active, "Control/", "control.search", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("No staged privacy/disclosure change is prepared.", "central.system"),
    })
}

fn settings_axes_for_proposals(root: &Path, observed: u64) -> Value {
    let (total, pending, accepted, rejected) = scan_proposals(root);
    // Proposals are never authored: the declared axis is honestly empty.
    json!({
        "declared": declared_axis(json!({"authored": false, "note": "proposals are staged material, never authored ground"}), ".central/source-returns", observed),
        "effective": effective_axis(json!({"proposal_store": ".central/source-returns", "total": total}), ".central/source-returns", observed),
        "active": active_axis(json!({"total": total, "pending": pending, "accepted": accepted, "rejected": rejected}), ".central/source-returns", "projectcentral.source.returns", observed),
        "staged": staged_axis(
            json!({"pending_or_applying": pending, "accepted": accepted, "rejected": rejected}),
            if pending > 0 { Some(".central/source-returns") } else { None },
            if pending > 0 { "prepared" } else { "none" },
            observed,
        ),
        "expected_effect": expected_effect("Accepting a pending proposal applies it through the native source/flow writer and records the result revision.", "projectcentral.source.return_accept"),
    })
}

fn settings_axes_for_accepted_mutation(root: &Path, projects: &[String], observed: u64) -> Value {
    let mut accepted_relations = 0usize;
    let mut human_sources = Vec::new();
    for project in projects {
        let project_root = root.join("Work").join(project);
        if let Ok(inspection) = inspect_project_ground(&project_root) {
            for record in &inspection.recognised_sources {
                let provenance = serde_json::to_value(&record.provenance)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "unresolved".to_owned());
                let standing = serde_json::to_value(&record.standing)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "unspecified".to_owned());
                if matches!(provenance.as_str(), "human-authored" | "human-adopted") {
                    human_sources.push(json!({
                        "ref": record.source_ref,
                        "path": record.path,
                        "standing": standing,
                    }));
                }
                accepted_relations += 1;
            }
        }
    }
    json!({
        "declared": declared_axis(json!({"accepted_human_sources": human_sources}), "ProjectCentral/relations/source-relations.json", observed),
        "effective": effective_axis(json!({"recognised_relations": accepted_relations, "recognised_human_sources": human_sources.len()}), "ProjectCentral/relations/source-relations.json", observed),
        "active": active_axis(json!({"relations_observed": accepted_relations}), "ProjectCentral/relations/source-relations.json", "projectcentral.ground.inspect", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("projectcentral.ground.apply records one explicitly human-accepted source relation; projectcentral.source.write revises a participating source under CAS.", "projectcentral.ground.apply"),
    })
}

fn settings_axes_for_actions(action_count: usize, observed: u64) -> Value {
    json!({
        "declared": declared_axis(json!({"native_seam": "action.list", "count": action_count, "note": "this is a reference to Central's native Action registry, not a duplicate catalogue"}), "ctrl/src/action.rs", observed),
        "effective": effective_axis(json!({"registry_count": action_count, "invocation_seam": "ctrl action run <id> [<json>] [--json]"}), "action.list", observed),
        "active": active_axis(json!({"count": action_count}), "action.list", "action.list", observed),
        "staged": staged_axis(json!(null), None, "none", observed),
        "expected_effect": expected_effect("Disclosed actions are invoked through the native `action run` seam; receipts return through the same seam.", "action.list"),
    })
}

/// §4.7: owner-level availability is derived from the doctor probe Central
/// already runs, never a literal. A failing or mixed root degrades the reading
/// with a real reason; a missing or non-directory root makes the owner
/// unavailable. `available` with an empty degradations list is only produced
/// when the probe reports a valid, non-mixed root — so the failure branch is
/// reachable and observable, not papered over.
fn derive_availability(health: &CentralHealth) -> (String, Option<String>, Vec<Value>) {
    let mut degradations = Vec::new();

    // A missing or non-directory root is the ground failing to exist at all:
    // that is unavailability, not a mere degradation.
    match health.root_state.as_str() {
        "missing" => {
            let reason = "Central root does not exist".to_owned();
            degradations.push(json!({
                "subject_ref": "central:root",
                "state": "unavailable",
                "reason": reason,
                "native_error": null,
            }));
            return ("unavailable".to_owned(), Some(reason), degradations);
        }
        "not_directory" => {
            let reason = "Central root path is not a directory".to_owned();
            degradations.push(json!({
                "subject_ref": "central:root",
                "state": "unavailable",
                "reason": reason,
                "native_error": null,
            }));
            return ("unavailable".to_owned(), Some(reason), degradations);
        }
        _ => {}
    }

    // A directory root that fails any required-directory check is degraded,
    // naming the specific missing directory.
    for check in &health.checks {
        if !check.valid {
            degradations.push(json!({
                "subject_ref": format!("central:root:{}", check.path),
                "state": "degraded",
                "reason": format!("required directory missing: {}", check.path),
                "native_error": null,
            }));
        }
    }

    // A mixed root (personal ground that is also the product source checkout)
    // is a real, observed degradation, not a silent success.
    if health.mixed_root.detected {
        degradations.push(json!({
            "subject_ref": "central:root:composition",
            "state": "degraded",
            "reason": health.mixed_root.message.clone().unwrap_or_else(|| {
                "Central personal root is also the Central product source checkout".to_owned()
            }),
            "native_error": null,
        }));
    }

    if degradations.is_empty() {
        ("available".to_owned(), None, degradations)
    } else {
        let reason = degradations
            .iter()
            .filter_map(|d| d.get("reason").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("; ");
        ("degraded".to_owned(), Some(reason), degradations)
    }
}

fn build_descriptor(
    registry: &ActionRegistry,
    root_options: &RootOptions,
) -> Result<Value, String> {
    let observed = now_ms();
    let resolved = resolve_central_root(root_options).map_err(|e| e)?;
    let health = inspect_central(&resolved.path).map_err(|e| e.to_string())?;
    let resolved_source = match resolved.source {
        crate::root::RootSource::Explicit => "explicit",
        crate::root::RootSource::Environment => "environment",
        crate::root::RootSource::Default => "default",
    };
    let projects = list_projects(&resolved.path);
    let action_count = registry.list().len();
    let (availability_state, availability_reason, degradations) = derive_availability(&health);

    let sections = vec![
        json!({
            "id": "ground",
            "title": "Personal and Project ground",
            "settings": [
                json!({
                    "key": "central.ground.register",
                    "title": "Ground register binding",
                    "kind": "reference",
                    "axes": settings_axes_for_ground(&resolved.path, &health, resolved_source, &projects, observed),
                    "mutable": false,
                    "native_path": "central.root / central.doctor",
                    "bootstrap": true,
                    "drift": drift(
                        if health.mixed_root.detected { "diverged" } else { "none" },
                        &["declared", "active"],
                        if health.mixed_root.detected { Some("central.recover") } else { None },
                    ),
                }),
                json!({
                    "key": "central.ground.projects",
                    "title": "Project registers",
                    "kind": "table",
                    "axes": json!({
                        "declared": declared_axis(json!({"projects": projects}), "Work/", observed),
                        "effective": effective_axis(json!({"projectcentral_ready": projects.len()}), "Work/", observed),
                        "active": active_axis(json!({"observed_projects": projects.len()}), "Work/", "central.world", observed),
                        "staged": staged_axis(json!(null), None, "none", observed),
                        "expected_effect": expected_effect("projectcentral.init / adopt establish a project register without migrating existing source.", "projectcentral.init"),
                    }),
                    "mutable": false,
                    "native_path": "projectcentral.init / projectcentral.adopt",
                    "bootstrap": false,
                    "drift": drift("none", &["declared", "effective"], None),
                }),
            ],
        }),
        json!({
            "id": "self-description",
            "title": "Self-description",
            "settings": [ json!({
                "key": "central.self-description",
                "title": "Authored self-description vs agent-maintained self-knowledge",
                "kind": "presence",
                "axes": settings_axes_for_self_description(&resolved.path, observed),
                "mutable": false,
                "native_path": "central.wiki.read",
                "bootstrap": false,
                "drift": drift("none", &["declared", "active"], None),
            }) ],
        }),
        json!({
            "id": "machine-intent",
            "title": "Machine intent",
            "settings": [ json!({
                "key": "central.machine-intent",
                "title": "Authored machine roles vs observed machine state",
                "kind": "table",
                "axes": settings_axes_for_machine_intent(&resolved.path, observed),
                "mutable": false,
                "native_path": "machine.declaration / machine.inspect",
                "bootstrap": false,
                "drift": drift("unknown", &["declared", "active"], None),
            }) ],
        }),
        json!({
            "id": "skills",
            "title": "Skills and Methods source",
            "settings": [ json!({
                "key": "central.skills",
                "title": "Authored skill ground and standing",
                "kind": "table",
                "axes": settings_axes_for_skills(&resolved.path, observed),
                "mutable": true,
                "native_path": "control.skills.retire / control.skills.restore",
                "bootstrap": false,
                "drift": drift("none", &["declared", "effective"], None),
            }) ],
        }),
        json!({
            "id": "privacy",
            "title": "Privacy and disclosure policy",
            "settings": [ json!({
                "key": "central.privacy.policy",
                "title": "Retrieval exclusions",
                "kind": "presence",
                "axes": settings_axes_for_privacy(&resolved.path, observed),
                "mutable": false,
                "native_path": "control.search (respects .no-agent-retrieval)",
                "bootstrap": false,
                "drift": drift("none", &["declared", "active"], None),
            }) ],
        }),
        json!({
            "id": "proposals",
            "title": "Proposals",
            "settings": [ json!({
                "key": "central.proposals",
                "title": "Pending source-return proposals",
                "kind": "table",
                "axes": settings_axes_for_proposals(&resolved.path, observed),
                "mutable": true,
                "native_path": "projectcentral.source.return / projectcentral.source.return_accept",
                "bootstrap": false,
                "drift": drift("none", &["staged", "active"], None),
            }) ],
        }),
        json!({
            "id": "accepted-source-mutation",
            "title": "Accepted source mutation",
            "settings": [ json!({
                "key": "central.accepted-source-mutation",
                "title": "Accepted source relations",
                "kind": "table",
                "axes": settings_axes_for_accepted_mutation(&resolved.path, &projects, observed),
                "mutable": true,
                "native_path": "projectcentral.ground.apply / projectcentral.source.write",
                "bootstrap": false,
                "drift": drift("none", &["declared", "effective"], None),
            }) ],
        }),
        json!({
            "id": "actions",
            "title": "Current Central owner Actions",
            "settings": [ json!({
                "key": "central.actions",
                "title": "Native Action field",
                "kind": "reference",
                "axes": settings_axes_for_actions(action_count, observed),
                "mutable": false,
                "native_path": "action.list",
                "bootstrap": false,
                "drift": drift("none", &["declared", "effective"], None),
            }) ],
        }),
    ];

    let actions = disclosed_actions(observed);

    let owner = json!({
        "owner_id": SYSTEM_PRODUCT_ID,
        "owner_ref": SYSTEM_OWNER_REF,
        "owner_version": env!("CARGO_PKG_VERSION"),
        "reading_command": SYSTEM_READING_COMMAND,
        "reading_digest": Value::Null,
        "reading_digest_covers": "whole descriptor, every *_unix_ms field zeroed, owner.reading_digest null",
        "observed_at_unix_ms": observed,
    });

    let mut descriptor = json!({
        "schema": SYSTEM_DISCLOSURE_SCHEMA,
        "product_id": SYSTEM_PRODUCT_ID,
        "contract_revision": SYSTEM_CONTRACT_REVISION,
        "disclosed_at_unix_ms": observed,
        "owner": owner,
        "about": "Central is the personal ground: durable authored Control source (self-description, governance, machine intent, skills), the recursive ProjectCentral register, and stable canonical Actions. It keeps authored intent distinct from observed state, and every legitimate source change returns through an explicit proposal-then-human-acceptance path.",
        "sections": sections,
        "actions": actions,
        "availability": json!({"state": availability_state, "reason": availability_reason}),
        "degradations": json!(degradations),
        "obligations": json!([
            "Attested human principal: ActionExecutionContext carries no attested human identity, so `human-accepted` acceptance strings are caller declarations, not grants of human source authority; acceptance of an authored-source return currently reports unavailable_capability (SOURCE-RETURN.md).",
            "UI invoke seam: this descriptor renders read-only in the O:I System surface; Central has not yet disclosed central_intent/central_invoke kernel ops (Factory pattern), so engagement crosses only the native `ctrl action run` seam, not the surface.",
            "reading_digest is computed by a locally-implemented SHA-256 (no external hash dependency is accepted in ctrl), over the descriptor with all *_unix_ms fields zeroed; verified against FIPS 180-4 vectors."
        ]),
    });

    // Compute the time-independent digest and stamp it into the owner block.
    let mut canonical = descriptor.clone();
    zero_timestamps(&mut canonical);
    let digest = sha256_hex(
        serde_json::to_string(&canonical)
            .unwrap_or_default()
            .as_bytes(),
    );
    descriptor["owner"]["reading_digest"] = json!(digest);

    Ok(descriptor)
}

fn disclosed_actions(observed: u64) -> Value {
    let args = |fields: &[(&str, &str)]| -> Value {
        fields
            .iter()
            .map(|(name, kind)| json!({"name": name, "kind": kind}))
            .collect::<Vec<_>>()
            .into()
    };
    let expose = |ui: bool| -> Value { json!({"ui": ui, "agent": true, "headless": true}) };
    // The human-accepted authority that Central's native layer cannot yet attest.
    let human_accept_authority = || -> Value {
        json!({
            "requires": ["acceptance=human-accepted", "attested human principal"],
            "granted_by": "caller declaration (no attested principal in native ActionExecutionContext)",
            "evidence_ref": null,
        })
    };

    json!([
        json!({
            "action_ref": "projectcentral.source.return",
            "title": "Propose a source change",
            "args": args(&[("project", "string"), ("source_ref", "string"), ("expected_revision", "string"), ("proposed_content", "string"), ("reason", "string"), ("agent_session_ref", "string"), ("evidence_refs", "array")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.source-return"],
            "authority": json!({"requires": ["source authority for the target source"], "granted_by": "native source binding", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "central.source-returns", "command": ["ctrl", "action", "run", "projectcentral.source.returns", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "projectcentral.source.return_accept",
            "title": "Accept a proposal",
            "args": args(&[("project", "string"), ("return_ref", "string"), ("expected_revision", "string"), ("acceptance", "string"), ("accepted_by_ref", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.source-return"],
            "authority": human_accept_authority(),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "central.source-returns", "command": ["ctrl", "action", "run", "projectcentral.source.returns", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "projectcentral.source.return_reject",
            "title": "Reject a proposal",
            "args": args(&[("project", "string"), ("return_ref", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.source-return"],
            "authority": json!({"requires": [], "granted_by": "proposal owner", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "central.source-returns", "command": ["ctrl", "action", "run", "projectcentral.source.returns", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "projectcentral.ground.apply",
            "title": "Accept a source-ground relation",
            "args": args(&[("project", "string"), ("source", "string"), ("provenance", "string"), ("standing", "string"), ("treatment", "string"), ("acceptance", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.source-relation"],
            "authority": json!({"requires": ["acceptance=human-accepted"], "granted_by": "caller declaration (human-accepted is the semantic boundary; Central cannot infer identity)", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "projectcentral.ground.inspect", "command": ["ctrl", "action", "run", "projectcentral.ground.inspect", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "projectcentral.source.write",
            "title": "Revise a participating source",
            "args": args(&[("project", "string"), ("source_ref", "string"), ("expected_revision", "string"), ("content", "string"), ("actor", "string"), ("actor_kind", "string"), ("agent_session_ref", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.participating-source"],
            "authority": json!({"requires": ["source authority for the target source"], "granted_by": "native source binding", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "projectcentral.change.horizon", "command": ["ctrl", "action", "run", "projectcentral.change.horizon", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "projectcentral.now.return",
            "title": "Write a bounded return into the NOW field",
            "args": args(&[("project", "string"), ("actor", "string"), ("kind", "string"), ("subject", "string"), ("result", "string"), ("status", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.now-return"],
            "authority": json!({"requires": [], "granted_by": "actor attribution (declared)", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "projectcentral.now.inspect", "command": ["ctrl", "action", "run", "projectcentral.now.inspect", "{\"project\":\"<project>\"}", "--json"]}),
        }),
        json!({
            "action_ref": "central.remember",
            "title": "Remember a selection as a generated proposal",
            "args": args(&[("selection", "string"), ("source_ref", "string"), ("actor", "string"), ("actor_kind", "string"), ("agent_session_ref", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.remembered-note"],
            "authority": json!({"requires": [], "granted_by": "generated-proposal (recognition is the human owner's separate act)", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "central.remembered", "command": ["ctrl", "action", "run", "action.list", "{}", "--json"]}),
        }),
        json!({
            "action_ref": "control.skills.retire",
            "title": "Retire a skill's standing",
            "args": args(&[("scope", "string"), ("name", "string"), ("retired_by", "string"), ("retirement_reason", "string")]),
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["central.skill"],
            "authority": json!({"requires": [], "granted_by": "declared caller (recorded verbatim)", "evidence_ref": null}),
            "exposure": expose(false),
            "explain": json!({"ref": "action.list", "command": ["ctrl", "action", "list", "--json"]}),
            "history": json!({"ref": "control.skills.inspect", "command": ["ctrl", "action", "run", "control.skills.inspect", "{}", "--json"]}),
        }),
    ])
}

fn system_action(
    registry: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match build_descriptor(registry, context.root_options) {
        Ok(descriptor) => ActionResult::success(SYSTEM_ACTION_ID, descriptor),
        Err(message) => ActionResult::failure(
            Some(SYSTEM_ACTION_ID),
            ResultStatus::InvalidInput,
            message,
            None,
        ),
    }
}

pub fn register_system_disclosure_action(registry: &mut ActionRegistry) {
    registry
        .register(
            ActionDescriptor {
                id: SYSTEM_ACTION_ID.to_owned(),
                title: "Central System disclosure".to_owned(),
                description: "Compose Central's native oi.product-settings-disclosure/v2 reading: authored vs effective vs observed state across ground, self-description, machine intent, skills, privacy, proposals, accepted source mutation and the native Action field.".to_owned(),
                inputs: Vec::new(),
                output: ActionOutputDefinition {
                    output_type: SYSTEM_DISCLOSURE_SCHEMA.to_owned(),
                },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![],
                availability: ActionAvailability {
                    available: true,
                    reason: None,
                },
            },
            system_action,
        )
        .expect("central.system Action id is valid");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_fips_180_4_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn zero_timestamps_recurses_into_nested_values() {
        let mut v = json!({
            "disclosed_at_unix_ms": 123,
            "owner": {"observed_at_unix_ms": 456},
            "sections": [{"settings": [{"axes": {"declared": {"provenance": {"observed_at_unix_ms": 789}}}}]}],
            "name": "kept",
        });
        zero_timestamps(&mut v);
        assert_eq!(v["disclosed_at_unix_ms"], 0);
        assert_eq!(v["owner"]["observed_at_unix_ms"], 0);
        assert_eq!(
            v["sections"][0]["settings"][0]["axes"]["declared"]["provenance"]
                ["observed_at_unix_ms"],
            0
        );
        assert_eq!(v["name"], "kept");
    }

    #[test]
    fn availability_is_derived_from_the_doctor_probe_not_a_literal() {
        use crate::root::{CentralHealth, DirectoryCheck, MixedRootDiagnostic, MixedRootSignal};

        // A valid, non-mixed root is available with an empty degradations list.
        let healthy = CentralHealth {
            root: PathBuf::from("/central"),
            root_state: "directory".to_owned(),
            valid: true,
            checks: vec![DirectoryCheck {
                path: "Control/user".to_owned(),
                valid: true,
            }],
            mixed_root: MixedRootDiagnostic {
                detected: false,
                signals: vec![],
                message: None,
            },
        };
        let (state, reason, degradations) = derive_availability(&healthy);
        assert_eq!(state, "available");
        assert_eq!(reason, None);
        assert!(degradations.is_empty());

        // A missing root makes the owner unavailable with a real reason.
        let missing = CentralHealth {
            root: PathBuf::from("/central"),
            root_state: "missing".to_owned(),
            valid: false,
            checks: vec![],
            mixed_root: MixedRootDiagnostic {
                detected: false,
                signals: vec![],
                message: None,
            },
        };
        let (state, reason, degradations) = derive_availability(&missing);
        assert_eq!(state, "unavailable");
        assert_eq!(reason.as_deref(), Some("Central root does not exist"));
        assert_eq!(degradations.len(), 1);
        assert_eq!(degradations[0]["state"], "unavailable");

        // A non-directory root is also unavailability, with its own reason.
        let not_directory = CentralHealth {
            root: PathBuf::from("/central"),
            root_state: "not_directory".to_owned(),
            valid: false,
            checks: vec![],
            mixed_root: MixedRootDiagnostic {
                detected: false,
                signals: vec![],
                message: None,
            },
        };
        let (state, _, degradations) = derive_availability(&not_directory);
        assert_eq!(state, "unavailable");
        assert_eq!(
            degradations[0]["reason"],
            "Central root path is not a directory"
        );

        // A mixed root degrades the reading with the mixed-root reason.
        let mixed = CentralHealth {
            root: PathBuf::from("/central"),
            root_state: "directory".to_owned(),
            valid: true,
            checks: vec![],
            mixed_root: MixedRootDiagnostic {
                detected: true,
                signals: vec![
                    MixedRootSignal::CargoManifest,
                    MixedRootSignal::CtrlSourceDirectory,
                ],
                message: Some("mixed root message".to_owned()),
            },
        };
        let (state, reason, degradations) = derive_availability(&mixed);
        assert_eq!(state, "degraded");
        assert_eq!(reason.as_deref(), Some("mixed root message"));
        assert_eq!(degradations[0]["subject_ref"], "central:root:composition");

        // A directory root with a failing required-directory check degrades,
        // naming the specific missing directory.
        let invalid = CentralHealth {
            root: PathBuf::from("/central"),
            root_state: "directory".to_owned(),
            valid: false,
            checks: vec![DirectoryCheck {
                path: "Work".to_owned(),
                valid: false,
            }],
            mixed_root: MixedRootDiagnostic {
                detected: false,
                signals: vec![],
                message: None,
            },
        };
        let (state, reason, degradations) = derive_availability(&invalid);
        assert_eq!(state, "degraded");
        assert_eq!(reason.as_deref(), Some("required directory missing: Work"));
        assert_eq!(degradations[0]["subject_ref"], "central:root:Work");
    }
}
