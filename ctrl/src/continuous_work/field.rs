//! The composed activity field reading (`central.now.field`).
//!
//! One read that joins, per participating Workcell, the NOW plane this
//! register owns with the material and conversation readings of their native
//! owners — so the same live work is addressable from either machine without
//! a second dashboard that could disagree.
//!
//! Ownership law (docs/experience/WORKCELL-NOW-TEMPORAL-FIELD.md §1): Central
//! remains NOW/DAY source owner; Workcell remains material observation owner;
//! AIKit remains session/composition and Gateway owner. This reading never
//! mints identity: every block names its native owner, carries
//! `observed_at_unix_seconds`, and distinguishes an observed fact from an
//! unavailable one. A failed or missing census leaves the block explicitly
//! `available: false` — a disconnected cell must not look empty or healthy.
//!
//! Correlation law: the only joins asserted here are ones an authored source
//! declares. A Workcell-root NOW carries `workcell_ref`; the machine
//! declaration (`Control/machines/current.json`) binds this machine to its
//! Workcell. The local census/instances/status and the Gateway status attach
//! to the root whose `workcell_ref` that declaration names, and the census's
//! own native cell ref is retained verbatim beside the declared identity (a
//! renamed cell must not silently rewrite owner output). Census panes carry
//! no canonical NOW refs, so panes are reported as material of the horizon —
//! never as members of a child NOW.

use super::{
    placement,
    source::{invalid, Scope},
    temporal,
};
use serde_json::{json, Value};
use std::{
    io,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const FIELD_SCHEMA: &str = "central.now-field/v1";

/// Observation bounds. The field is a reading, not a warehouse: subprocess
/// calls are bounded in time, rows are capped, and every cap is disclosed so
/// a truncated reading cannot masquerade as a complete one.
const SUBPROCESS_TIMEOUT_MS: u64 = 12_000;
const CENSUS_ROW_MAX: usize = 160;
const INSTANCES_MAX: usize = 64;

/// Run one native owner command and parse its JSON stdout, bounded in time.
/// Kill on expiry; classify expiry and spawn absence as explicit unavailability
/// with the owner's own condition named — never as absence of work.
fn run_owner_json(program: &str, args: &[&str]) -> Result<(Value, u64), String> {
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program} did not start: {error}"))?;
    let deadline = Duration::from_millis(SUBPROCESS_TIMEOUT_MS);
    let mut waited = Duration::ZERO;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        use io::Read;
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    let detail = stderr.trim();
                    return Err(format!(
                        "{program} {args:?} exited unsuccessfully{}",
                        if detail.is_empty() {
                            String::new()
                        } else {
                            format!(": {detail}")
                        }
                    ));
                }
                break;
            }
            Ok(None) => {
                if waited >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{program} exceeded the {}ms field-reading bound and was stopped",
                        SUBPROCESS_TIMEOUT_MS
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
                waited += Duration::from_millis(20);
            }
            Err(error) => return Err(format!("{program} could not be waited on: {error}")),
        }
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    // The child was reaped or killed above; take its stdout without blocking.
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        use io::Read;
        let _ = pipe.read_to_string(&mut stdout);
    }
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!("{program} {args:?} wrote unparseable output: {error}")
    })?;
    Ok((value, elapsed_ms))
}

fn owner_reading(program: &str, args: &[&str], now: u64) -> Value {
    match run_owner_json(program, args) {
        Ok((value, elapsed_ms)) => json!({
            "available": true,
            "observed_at_unix_seconds": now,
            "duration_ms": elapsed_ms,
            "reading": value,
        }),
        Err(reason) => json!({
            "available": false,
            "observed_at_unix_seconds": now,
            "reason": reason,
            "native_owner_command": format!("{program} {}", args.join(" ")),
        }),
    }
}

/// The machine's declared Workcell binding, read from the authored machine
/// declaration. Absence is a finding, not a guess: without a declared binding
/// the local material stays explicitly unjoined.
fn declared_workcell(scope: &Scope) -> Option<(String, String)> {
    let path = scope.central_root.join("Control/machines/current.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let reference = value["bindings"]
        .as_array()?
        .iter()
        .filter(|binding| binding["kind"] == "workcell")
        .filter_map(|binding| binding["reference"].as_str())
        .next()?
        .to_string();
    Some((
        reference,
        scope.source_ref("Control/machines/current.json"),
    ))
}

/// Cap a JSON array with an honest truncation count.
fn capped(value: &Value, field: &str, max: usize) -> (Value, usize) {
    let empty = Vec::new();
    let rows = value[field].as_array().unwrap_or(&empty);
    if rows.len() <= max {
        return (value.clone(), 0);
    }
    let mut bounded = value.clone();
    bounded[field] = Value::Array(rows[..max].to_vec());
    (bounded, rows.len() - max)
}

/// One owner reading bundle for the local cell, run on its own thread so a
/// degraded owner cannot serialise its timeout across the whole field: the
/// composed reading costs roughly its slowest call, not the sum of them.
fn owner_reading_threaded(
    program: &str,
    args: &[&str],
    now: u64,
) -> std::sync::mpsc::Receiver<Value> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let program = program.to_owned();
    let args: Vec<String> = args.iter().map(|value| value.to_string()).collect();
    std::thread::spawn(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let _ = sender.send(owner_reading(&program, &args, now));
    });
    receiver
}

/// The full composed field over this scope's Workcell-root NOWs.
pub(crate) fn field(scope: &Scope, input: &Value, now: u64) -> io::Result<Value> {
    if now == 0 {
        return Err(invalid("field reading requires a current observation time"));
    }
    let filter: Vec<String> = match input.get("workcell_refs") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => {
            let mut refs = Vec::new();
            for value in values {
                let reference = value
                    .as_str()
                    .ok_or_else(|| invalid("workcell_refs must be strings"))?;
                placement::validate_workcell_ref(reference)?;
                refs.push(reference.to_string());
            }
            refs
        }
        Some(_) => return Err(invalid("workcell_refs must be an array of strings")),
    };
    let include_material = !matches!(input.get("include_material"), Some(Value::Bool(false)));
    let include_gateway = !matches!(input.get("include_gateway"), Some(Value::Bool(false)));

    // The NOW plane is read in this register; workcell roots are root-register
    // clearings by horizon law.
    let records = placement::scan_now(scope)?;
    let mut roots: Vec<_> = records
        .iter()
        .filter(|(record, _)| record.horizon.as_deref() == Some(placement::HORIZON_WORKCELL_ROOT))
        .filter(|(record, _)| {
            filter.is_empty()
                || record
                    .workcell_ref
                    .as_deref()
                    .is_some_and(|reference| filter.iter().any(|wanted| wanted == reference))
        })
        .collect();
    roots.sort_by(|a, b| a.0.now_ref.cmp(&b.0.now_ref));

    let declared = declared_workcell(scope);
    // Local-owner readings run once per field read, never once per root — and
    // only when a declared root can carry them: a field with no local root
    // observes nothing and spawns no owner process.
    let local_requested = declared.clone().is_some_and(|(reference, _)| {
        roots
            .iter()
            .any(|(record, _)| record.workcell_ref.as_deref() == Some(reference.as_str()))
    });
    let material_reading = if include_material && local_requested {
        let census = owner_reading_threaded("workcell", &["places", "--json"], now);
        let status = owner_reading_threaded("workcell", &["status", "--json"], now);
        let instances = owner_reading_threaded("workcell", &["instances", "list", "--json"], now);
        Some(json!({
            "census": census.recv().unwrap_or_else(|_| json!({"available": false, "reason": "census reading thread ended without an answer"})),
            "status": status.recv().unwrap_or_else(|_| json!({"available": false, "reason": "status reading thread ended without an answer"})),
            "instances": instances.recv().unwrap_or_else(|_| json!({"available": false, "reason": "instances reading thread ended without an answer"})),
        }))
    } else {
        None
    };
    let gateway_reading = if include_gateway && local_requested {
        Some(
            owner_reading_threaded("aikit", &["gateway", "status", "--json"], now)
                .recv()
                .unwrap_or_else(|_| json!({"available": false, "reason": "gateway reading thread ended without an answer"})),
        )
    } else {
        None
    };

    let mut workcells = Vec::new();
    let mut unjoined_local: Option<Value> = None;
    for (record, reading) in &roots {
        let children = placement::children(
            scope,
            &json!({"now_ref": record.now_ref}),
        )?;
        let is_declared = declared
            .as_ref()
            .is_some_and(|(reference, _)| {
                record.workcell_ref.as_deref() == Some(reference.as_str())
            });
        let material = if !include_material {
            json!({"available": false, "reason": "material reading not requested", "observation_scope": "omitted"})
        } else if !is_declared {
            json!({
                "available": false,
                "observation_scope": "remote",
                "reason": "this cell observes its own material only; this Workcell is observed on its own machine and is not declared to this machine",
                "note": "unavailable here is not absence: the Workcell may be live where it is hosted"
            })
        } else {
            let value = material_reading.clone().expect("material was requested");
            let (census, census_truncated) =
                capped(&value["census"]["reading"], "panes", CENSUS_ROW_MAX);
            let mut census_bounded = value["census"].clone();
            if census_truncated > 0 {
                census_bounded["reading"] = census;
                census_bounded["truncated_rows"] = json!(census_truncated);
            }
            let (instances, instances_truncated) = capped(
                &value["instances"]["reading"],
                "instances",
                INSTANCES_MAX,
            );
            let mut instances_bounded = value["instances"].clone();
            if instances_truncated > 0 {
                instances_bounded["reading"] = instances;
                instances_bounded["truncated_rows"] = json!(instances_truncated);
            }
            json!({
                "available": census_bounded["available"].as_bool() == Some(true),
                "observation_scope": "local",
                "declared_join": declared.as_ref().map(|(_, source_ref)| json!({
                    "source_ref": source_ref,
                    "relation": "the machine declaration binds this machine to this Workcell"
                })),
                "native_workcell_ref": value["census"]["reading"]["workcell_ref"]
                    .as_str()
                    .or(value["instances"]["reading"]["workcell_ref"].as_str()),
                "native_ref_note": "the census/instances owner reports the cell under its own ref; the declared binding above is the authored identity and both are retained",
                "census": census_bounded,
                "status": value["status"],
                "instances": instances_bounded,
            })
        };
        let gateway = if !include_gateway {
            json!({"available": false, "reason": "gateway reading not requested", "observation_scope": "omitted"})
        } else if !is_declared {
            json!({
                "available": false,
                "observation_scope": "remote",
                "reason": "the Gateway status of another machine is read on that machine; this reading carries only the local cell's Gateway",
            })
        } else {
            let value = gateway_reading.clone().expect("gateway was requested");
            json!({
                "available": value["available"].as_bool() == Some(true),
                "observation_scope": "local",
                "status": value,
            })
        };
        workcells.push(json!({
            "workcell_ref": record.workcell_ref,
            "root": placement::now_row(record, reading),
            "children": children["children"],
            "children_count": children["count"],
            "children_unscanned": children["unscanned"],
            "material": material,
            "gateway": gateway,
        }));
        if !is_declared && declared.is_none() {
            // No authored binding exists at all: the local readings would be
            // dropped silently. Retain them unjoined and name the finding.
            unjoined_local = Some(json!({
                "reason": "this machine declares no Workcell binding in Control/machines/current.json, so the local readings attach to no Workcell root",
                "material": material_reading,
                "gateway": gateway_reading,
            }));
        }
    }

    // Declared remote Workcells: AIKit's own remote-gateway declarations name
    // other machines this home can reach. The Gateway behind a declared
    // remote is observable from here through that route — the one piece of
    // the other machine that is genuinely addressable without copying it.
    // The remote ground's NOW plane and material census live there and are
    // never mirrored: a remote block is one availability reading plus its
    // routing provenance, not a copy of the other machine.
    let mut gateway_remotes_reading: Option<Value> = None;
    if include_gateway {
        let list = owner_reading("aikit", &["gateway", "remote", "list", "--json"], now);
        if list["available"].as_bool() == Some(true) {
            // Probe every declared remote concurrently: one slow machine
            // must not stretch the whole field by its own timeout.
            let mut pending: Vec<(String, Value, std::sync::mpsc::Receiver<Value>)> = Vec::new();
            for remote in list["reading"]["data"]["remotes"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                let reference = match remote["workcell_ref"].as_str() {
                    Some(value) if !value.is_empty() => value.to_string(),
                    _ => continue,
                };
                if placement::validate_workcell_ref(&reference).is_err() {
                    continue;
                }
                if roots.iter().any(|(record, _)| {
                    record.workcell_ref.as_deref() == Some(reference.as_str())
                }) {
                    continue;
                }
                if !filter.is_empty() && !filter.iter().any(|wanted| wanted == &reference) {
                    continue;
                }
                if workcells
                    .iter()
                    .any(|block| block["workcell_ref"] == reference)
                {
                    continue;
                }
                let status = owner_reading_threaded(
                    "aikit",
                    &["gateway", "status", "--at", reference.as_str(), "--json"],
                    now,
                );
                pending.push((reference, remote, status));
            }
            for (reference, remote, receiver) in pending {
                let status = receiver
                    .recv()
                    .unwrap_or_else(|_| json!({"available": false, "reason": "remote gateway reading thread ended without an answer"}));
                workcells.push(json!({
                    "workcell_ref": reference,
                    "root": Value::Null,
                    "children": [],
                    "children_count": 0,
                    "children_unscanned": [],
                    "remote_declaration": {
                        "token_location": remote["token_location"],
                        "websocket_bind": remote["websocket_bind"],
                        "route": "aikit gateway status --at — the answer names the gateway that produced it",
                    },
                    "material": {
                        "available": false,
                        "observation_scope": "remote-ground",
                        "reason": "this Workcell's NOW plane and material census live on their own ground and are not mirrored here; its Gateway is the addressable part",
                    },
                    "gateway": {
                        "available": status["available"].as_bool() == Some(true),
                        "observation_scope": "remote-declared",
                        "status": status,
                    },
                }));
            }
        } else {
            gateway_remotes_reading = Some(list);
        }
    }
    let missing: Vec<String> = filter
        .iter()
        .filter(|reference| {
            !roots
                .iter()
                .any(|(record, _)| record.workcell_ref.as_deref() == Some(reference.as_str()))
        })
        .cloned()
        .collect();

    // Temporal readings are native and honest: an unrecognised time policy or
    // a missing today pointer is reported, never substituted.
    let time_policy = temporal::time_policy(scope, now).ok();
    let day = temporal::day_read(scope, &json!({})).ok();

    Ok(json!({
        "schema": FIELD_SCHEMA,
        "scope_ref": scope.world_ref,
        "generated_at_unix_seconds": now,
        "machine": {
            "declared_workcell_ref": declared.as_ref().map(|(reference, _)| reference),
            "declaration_source_ref": declared.as_ref().map(|(_, source_ref)| source_ref),
        },
        "time": {
            "policy": time_policy,
            "day": day,
            "note": "absent readings are reported as absent; a disconnected or undeclared temporal source is not fabricated",
        },
        "workcells": workcells,
        "missing_workcell_roots": missing,
        "unjoined_local": unjoined_local,
        "gateway_remotes_unavailable": gateway_remotes_reading,
        "horizon": placement::horizon_reading(scope)?,
        "bounds": {
            "subprocess_timeout_ms": SUBPROCESS_TIMEOUT_MS,
            "census_rows_max": CENSUS_ROW_MAX,
            "instances_max": INSTANCES_MAX,
        },
        "automatic_agent_or_model_invocation": false,
        "pointer_note": "Records are pointers, not authority: every block names its native owner and observation time. Follow the governing sources before acting on any row.",
    }))
}
