//! Configuration plane owner lane (O:I #299 C3A): Central's native
//! `oi.configuration-contribution/v1` disclosure and the four-verb
//! owner-native mutation transport (`validate` / `plan` / `apply` / `reset`)
//! frozen by docs/cradle/09-CONFIGURATION-PLANE.md (C0).
//!
//! The Central-specific law this lane implements:
//!
//! - `Control/user` is authored human ground. Human-adopted policy facts
//!   (civil-time policy, work-placement policy, native-action authority) are
//!   contributed as **non-writable** settings: truthfully disclosed identities,
//!   never mutated through this transport. Central's proposal/acceptance
//!   source-return path stays the only door to authored ground.
//! - The one **writable** setting is state ctrl itself owns and already
//!   mutates through its own native Actions: skill standing in authored skill
//!   manifests (`control.skills.retire` / `control.skills.restore`). Apply and
//!   reset execute through exactly those native verbs — no second writer.
//! - Receipts, the idempotency index and the plan history live under derived
//!   local state (`.central/config/`), never under authored Control material.
//!
//! Structural compatibility with the v2 disclosure plane (C0 §17): the
//! writable setting keeps the exact v2 section id (`skills`) and v2 setting
//! key (`central.skills`), so `setting_ref` ↔ section/key resolution is
//! exact. The non-writable policy settings have no v2 counterpart; per C0 §17
//! neither plane infers the other's content.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control_skills::{
    read_skill_manifest, resolve_skill_location, restore_skill, retire_skill, SkillScope,
    SkillStanding,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::{inspect_central, resolve_central_root};
use crate::system_disclosure::{derive_availability, now_ms, sha256_hex, zero_timestamps};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CONFIG_CONTRIBUTION_SCHEMA: &str = "oi.configuration-contribution/v1";
pub const CONFIG_CONTRACT_REVISION: &str = "configuration-plane/contribution.1";
pub const CONFIG_VALIDATION_SCHEMA: &str = "oi.config-validation/v1";
pub const CONFIG_PLAN_SCHEMA: &str = "oi.config-plan/v1";
pub const CONFIG_RECEIPT_SCHEMA: &str = "oi.config-receipt/v1";
pub const CONFIG_ERROR_SCHEMA: &str = "oi.config-error/v1";
pub const CONFIG_HISTORY_SCHEMA: &str = "central.config-history/v1";
pub const CONFIG_RECEIPT_INDEX_SCHEMA: &str = "central.config-receipt-index/v1";

pub const CONFIG_CONTRIBUTION_ACTION: &str = "central.config.contribution";
pub const CONFIG_VALIDATE_ACTION: &str = "central.config.validate";
pub const CONFIG_PLAN_ACTION: &str = "central.config.plan";
pub const CONFIG_APPLY_ACTION: &str = "central.config.apply";
pub const CONFIG_RESET_ACTION: &str = "central.config.reset";

/// The product_id this owner contributes under (C0 §3: the existing
/// canonical id set; no second alias).
pub const CONFIG_OWNER_REF: &str = "central";

/// Derived local state (never authored Control material, never a commit
/// target): the owner-side receipt history and the idempotency index.
pub const CONFIG_HISTORY_DIR: &str = ".central/config/history";
pub const CONFIG_RECEIPTS_INDEX: &str = ".central/config/receipts.json";

/// Plans expire after this long; `apply` refuses older plans (`plan_expired`).
const PLAN_TTL_MS: u64 = 15 * 60 * 1000;
/// Bounded value/plan documents; the transport is for setting values, not files.
const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

/// The frozen scope kinds (C0 §5). Singular kinds may appear without a
/// `:ref`; every other kind requires one.
const SCOPE_KINDS: [&str; 12] = [
    "world",
    "ground",
    "project",
    "machine",
    "workcell",
    "agency",
    "agent",
    "session-space",
    "agent-session",
    "provider",
    "connector-relation",
    "invocation",
];
const SINGULAR_SCOPE_KINDS: [&str; 3] = ["world", "ground", "machine"];

// ---------------------------------------------------------------------------
// The setting catalogue. One stable identity per setting (C0 §3); the
// contribution document is assembled from this table so the disclosure and
// the verbs can never disagree.
// ---------------------------------------------------------------------------

struct SettingSpec {
    setting_ref: &'static str,
    section_ref: &'static str,
    section_title: &'static str,
    title: &'static str,
    description: &'static str,
    value_schema: Value,
    allowed_scopes: Vec<ScopeAllowance>,
    writable: bool,
    profileable: bool,
    sensitive: bool,
    effect: Value,
    operations: (bool, bool, bool, bool),
    native_ref: &'static str,
}

struct ScopeAllowance {
    kind: &'static str,
    /// `None` = any instance of this kind (the contribution convention).
    instance: Option<&'static str>,
}

const fn scope(kind: &'static str) -> ScopeAllowance {
    ScopeAllowance {
        kind,
        instance: None,
    }
}

/// The setting catalogue is built once at runtime (`json!` is not
/// const-callable); the disclosure and the verbs both read this one table so
/// they can never disagree.
fn settings() -> &'static [SettingSpec] {
    static SETTINGS: std::sync::OnceLock<Vec<SettingSpec>> = std::sync::OnceLock::new();
    SETTINGS.get_or_init(|| vec![
    // -- The one writable setting: state ctrl already mutates natively. ----
    SettingSpec {
        setting_ref: "central:skills:central.skills",
        section_ref: "skills",
        section_title: "Skills and Methods source",
        title: "Skill standing",
        description: "Requested standing (active | retired) for named skills that carry an authored manifest, per scope. Applied through the native control.skills.retire / control.skills.restore verbs; skills without a manifest are unresolved and are never touched.",
        value_schema: json!({
            "type": "table",
            "columns": [
                { "name": "skill", "type": "scalar" },
                { "name": "standing", "type": "enum" }
            ]
        }),
        allowed_scopes: vec![scope("ground"), scope("project")],
        writable: true,
        profileable: true,
        sensitive: false,
        effect: json!({
            "kind": "value-change",
            "summary": "Standing changes take effect at the next skill projection; no running process has to restart.",
            "ref": null
        }),
        operations: (true, true, true, true),
        native_ref: "central:skills:<scope>:<skill>:standing",
    },
    // -- Authored policy ground: disclosed read-only, never mutated here. --
    SettingSpec {
        setting_ref: "central:policy:civil-time.timezone",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Civil-time timezone",
        description: "The IANA timezone of Control/user/civil-time-policy.json, which Central's day lifecycle reads. Authored human ground: never mutated through the configuration verbs.",
        value_schema: json!({ "type": "scalar", "format": "iana-timezone" }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/civil-time-policy.json:timezone",
    },
    SettingSpec {
        setting_ref: "central:policy:civil-time.day-boundary-minutes",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Day boundary offset",
        description: "Minutes after local midnight at which the civil day closes (0-1439), from the authored civil-time policy. Never mutated through the configuration verbs.",
        value_schema: json!({ "type": "integer", "minimum": 0, "maximum": 1439 }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/civil-time-policy.json:day_boundary_minutes",
    },
    SettingSpec {
        setting_ref: "central:policy:civil-time.automatic-day-rollover",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Automatic day rollover",
        description: "Whether the day may roll over automatically (authored civil-time policy). Never mutated through the configuration verbs.",
        value_schema: json!({ "type": "boolean" }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/civil-time-policy.json:automatic_day_rollover",
    },
    SettingSpec {
        setting_ref: "central:policy:placement.enforcement",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Work placement enforcement",
        description: "How the authored work-placement policy is enforced (native-actions today). Never mutated through the configuration verbs.",
        value_schema: json!({ "type": "enum", "options": [{ "value": "native-actions" }] }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/placement.json:enforcement",
    },
    SettingSpec {
        setting_ref: "central:policy:placement.writable-paths",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Writable placement grants",
        description: "The paths the authored work-placement policy marks writable, with their class. Never mutated through the configuration verbs.",
        value_schema: json!({
            "type": "table",
            "columns": [
                { "name": "path", "type": "scalar" },
                { "name": "class", "type": "scalar" }
            ]
        }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/placement.json:writable",
    },
    SettingSpec {
        setting_ref: "central:policy:placement.protected-paths",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Protected placement paths",
        description: "The paths the authored work-placement policy protects from session writes (Control/user among them). Never mutated through the configuration verbs.",
        value_schema: json!({ "type": "list", "items": { "type": "scalar" } }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: false,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/placement.json:protected",
    },
    SettingSpec {
        setting_ref: "central:policy:native-action.authority",
        section_ref: "policy",
        section_title: "Authored policy ground (read-only)",
        title: "Native action authority",
        description: "Presence of the authored native-action-authority grants (Control/user/native-action-authority.json). Credential material never appears here: presence and reference only. Never mutated through the configuration verbs.",
        value_schema: json!({ "type": "reference", "subject_kind": "central.native-action-authority" }),
        allowed_scopes: vec![scope("ground")],
        writable: false,
        profileable: false,
        sensitive: true,
        effect: json!({ "kind": "none", "summary": null, "ref": null }),
        operations: (true, false, false, false),
        native_ref: "central:Control/user/native-action-authority.json",
    },
    ])
}

fn setting_by_ref(setting_ref: &str) -> Option<&'static SettingSpec> {
    settings().iter().find(|s| s.setting_ref == setting_ref)
}

// ---------------------------------------------------------------------------
// Structured error / receipt helpers (bare oi.config-* documents).
// ---------------------------------------------------------------------------

fn config_error(
    error_code: &str,
    message: impl Into<String>,
    setting_ref: Option<&str>,
    scope_kind: Option<&str>,
    retryable: bool,
) -> Value {
    json!({
        "schema": CONFIG_ERROR_SCHEMA,
        "error_code": error_code,
        "message": message.into(),
        "setting_ref": setting_ref,
        "scope_kind": scope_kind,
        "retryable": retryable,
        "detail_ref": null,
    })
}

/// Lift a resolved `oi.config-error/v1` document into a failure result for
/// the given action; the bare document stays in `data` so `--json` output is
/// the contract document, not an Action envelope.
fn config_failure(action: &str, document: Value) -> ActionResult {
    ActionResult {
        ok: false,
        status: ResultStatus::InvalidInput,
        action: Some(action.to_owned()),
        data: Some(document),
        error: None,
    }
}

/// A config Action failure carries the bare config-plane document in `data`
/// so `--json` output is the contract document, not an Action envelope.
fn error_result(
    action: &str,
    status: ResultStatus,
    error_code: &str,
    message: impl Into<String>,
    setting_ref: Option<&str>,
    scope_kind: Option<&str>,
    retryable: bool,
) -> ActionResult {
    ActionResult {
        ok: false,
        status,
        action: Some(action.to_owned()),
        data: Some(config_error(
            error_code,
            message,
            setting_ref,
            scope_kind,
            retryable,
        )),
        error: None,
    }
}

fn invalid_request(
    action: &str,
    message: impl Into<String>,
    setting_ref: Option<&str>,
    scope_kind: Option<&str>,
) -> ActionResult {
    error_result(
        action,
        ResultStatus::InvalidInput,
        "validation_failed",
        message,
        setting_ref,
        scope_kind,
        false,
    )
}

fn scope_document(kind: &str, scope_ref: Option<&str>) -> Value {
    json!({ "scope_kind": kind, "scope_ref": scope_ref })
}

/// Parse the compact CLI scope form `"<scope_kind>:<scope_ref>"`, with the
/// ref omitted for singular kinds (C0 §5). A scope kind outside the frozen
/// registry is `unknown_scope_kind`; a non-singular kind without a ref or a
/// singular kind with one is a malformed request.
fn parse_scope(compact: &str) -> Result<(String, Option<String>, Value), Value> {
    let compact = compact.trim();
    if compact.is_empty() {
        return Err(config_error(
            "validation_failed",
            "scope must not be empty",
            None,
            None,
            false,
        ));
    }
    let (kind, scope_ref) = match compact.split_once(':') {
        Some((kind, reference)) => (kind, Some(reference.to_owned())),
        None => (compact, None),
    };
    if !SCOPE_KINDS.contains(&kind) {
        return Err(config_error(
            "unknown_scope_kind",
            format!("unknown scope kind: {kind}"),
            None,
            Some(kind),
            false,
        ));
    }
    if SINGULAR_SCOPE_KINDS.contains(&kind) {
        if scope_ref.is_some() {
            return Err(config_error(
                "validation_failed",
                format!("singular scope kind {kind} takes no scope ref"),
                None,
                Some(kind),
                false,
            ));
        }
        return Ok((kind.to_owned(), None, scope_document(kind, None)));
    }
    let reference = match scope_ref {
        Some(reference) if !reference.trim().is_empty() => reference,
        _ => {
            return Err(config_error(
                "validation_failed",
                format!("scope kind {kind} requires a scope ref: {kind}:<ref>"),
                None,
                Some(kind),
                false,
            ))
        }
    };
    Ok((
        kind.to_owned(),
        Some(reference.clone()),
        scope_document(kind, Some(&reference)),
    ))
}

/// Check a parsed scope against a setting's declared allowances. Frozen
/// semantics: outside `allowed_scopes` is `unsupported_scope`, never a
/// fallback to another scope.
fn scope_allowed(setting: &SettingSpec, kind: &str) -> bool {
    setting
        .allowed_scopes
        .iter()
        .any(|allowance| allowance.kind == kind)
}

// ---------------------------------------------------------------------------
// Root / availability probing, shared with the Wave-5 disclosure.
// ---------------------------------------------------------------------------

fn resolve_probed_root(context: &ActionExecutionContext<'_>) -> Result<(PathBuf, Value), Value> {
    let resolved = match resolve_central_root(context.root_options) {
        Ok(resolved) => resolved,
        Err(message) => {
            return Err(config_error(
                "owner_unavailable",
                format!("Central root could not be resolved: {message}"),
                None,
                None,
                true,
            ))
        }
    };
    let health = match inspect_central(&resolved.path) {
        Ok(health) => health,
        Err(message) => {
            return Err(config_error(
                "owner_unavailable",
                format!("Central root could not be probed: {message}"),
                None,
                None,
                true,
            ))
        }
    };
    if health.root_state != "directory" {
        return Err(config_error(
            "owner_unavailable",
            format!(
                "Central root is not a usable directory (state: {})",
                health.root_state
            ),
            None,
            None,
            true,
        ));
    }
    let (state, reason, degradations) = derive_availability(&health);
    let availability = json!({
        "state": state,
        "reason": reason,
        "degradations": degradations,
    });
    Ok((resolved.path, availability))
}

// ---------------------------------------------------------------------------
// Contribution document (C0 §2).
// ---------------------------------------------------------------------------

fn setting_spec_document(setting: &SettingSpec) -> Value {
    json!({
        "setting_ref": setting.setting_ref,
        "section_ref": setting.section_ref,
        "title": setting.title,
        "description": setting.description,
        "value_schema": setting.value_schema,
        "allowed_scopes": setting.allowed_scopes.iter().map(|s| json!({
            "scope_kind": s.kind,
            "scope_ref": s.instance,
        })).collect::<Vec<_>>(),
        "writable": setting.writable,
        "profileable": setting.profileable,
        "sensitive": setting.sensitive,
        "effect": setting.effect,
        "operations": json!({
            "validate": setting.operations.0,
            "plan": setting.operations.1,
            "apply": setting.operations.2,
            "reset": setting.operations.3,
        }),
        "native_ref": setting.native_ref,
    })
}

fn sections_document() -> Value {
    let mut sections: Vec<Value> = Vec::new();
    for setting in settings() {
        let spec = setting_spec_document(setting);
        if let Some(section) = sections.iter_mut().find(|s| s["id"] == setting.section_ref) {
            section["settings"].as_array_mut().unwrap().push(spec);
        } else {
            sections.push(json!({
                "id": setting.section_ref,
                "title": setting.section_title,
                "settings": [spec],
            }));
        }
    }
    json!(sections)
}

/// The bare `oi.configuration-contribution/v1` document. Availability is
/// probed (never asserted); the reading digest follows the 07 §4.5
/// convention exactly (sha256 over the document with every `*_unix_ms` field
/// zeroed and `reading_digest` null).
pub fn build_contribution(root_options: &crate::root::RootOptions) -> Result<Value, String> {
    let observed = now_ms();
    let resolved = resolve_central_root(root_options)?;
    let health = inspect_central(&resolved.path).map_err(|e| e.to_string())?;
    let (availability_state, availability_reason, degradations) = derive_availability(&health);

    let document = json!({
        "schema": CONFIG_CONTRIBUTION_SCHEMA,
        "contract_revision": CONFIG_CONTRACT_REVISION,
        "owner": {
            "owner_ref": CONFIG_OWNER_REF,
            "owner_kind": "product",
            "owner_version": env!("CARGO_PKG_VERSION"),
            "contribution_command": ["ctrl", "config-contribution", "--json"],
            "disclosed_at_unix_ms": observed,
            "reading_digest": Value::Null,
            "reading_digest_covers": "whole contribution, every *_unix_ms field zeroed, owner.reading_digest null",
        },
        "about": "Central's addressable configuration: skill standing in authored skill manifests (writable through Central's own native verbs), plus the authored policy ground Central reads — disclosed read-only, never mutated here.",
        "sections": sections_document(),
        "operations": {
            "transport": "cli/v1",
            "validate": { "availability": "disclosed", "reason": null },
            "plan": { "availability": "disclosed", "reason": null },
            "apply": { "availability": "disclosed", "reason": null },
            "reset": { "availability": "disclosed", "reason": null },
        },
        "availability": { "state": availability_state, "reason": availability_reason },
        "degradations": degradations,
        "obligations": [
            "Authored human ground (Control/user) is disclosed read-only: the configuration verbs refuse it, and Central's proposal/acceptance source-return path stays the only door to authored source.",
            "Skill standing mutations are attributed with the declared caller carried by the transport (config-plane:<changeset_id>), recorded verbatim — the same trust the native control.skills.retire Action accepts.",
            "plan_digest is sha256 over the canonical plan body (plan_digest removed, plan_id and every *_unix_ms field zeroed), computed with the same locally-implemented SHA-256 the Wave-5 disclosure uses.",
        ],
    });

    let mut canonical = document.clone();
    zero_timestamps(&mut canonical);
    let digest = sha256_hex(
        serde_json::to_string(&canonical)
            .unwrap_or_default()
            .as_bytes(),
    );
    let mut document = document;
    document["owner"]["reading_digest"] = json!(digest);
    Ok(document)
}

// ---------------------------------------------------------------------------
// Skill-state reading (the writable setting's native facts).
// ---------------------------------------------------------------------------

struct SkillState {
    name: String,
    /// `None` = no authored manifest (unresolved): standing is never touched.
    standing: Option<SkillStanding>,
}

/// Read every manifest-bearing skill under one resolved skills root. Absent
/// scope directories are an empty (honest) reading, not an error.
fn read_scope_skills(root: &Path, kind: &str, scope_ref: Option<&str>) -> Vec<SkillState> {
    let skills_root = match skill_location(root, kind, scope_ref) {
        Some(location) => location,
        None => return Vec::new(),
    };
    let Ok(entries) = std::fs::read_dir(&skills_root) else {
        return Vec::new();
    };
    let mut states = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let manifest = read_skill_manifest(&entry.path()).ok().flatten();
        let standing = manifest.map(|m| m.standing);
        states.push(SkillState { name, standing });
    }
    states.sort_by(|a, b| a.name.cmp(&b.name));
    states
}

/// Map a config scope address onto Central's native skill scopes. Ground is
/// the root register's personal skills; project is one project's skills.
/// (Machine-scoped skills are not addressable through this transport.)
/// Returns the native scope plus the machine/project argument the native
/// verbs expect, so apply/reset mutate through the very scope they planned.
fn native_skill_scope(
    kind: &str,
    scope_ref: Option<&str>,
) -> Option<(SkillScope, Option<String>, Option<String>)> {
    match (kind, scope_ref) {
        ("ground", None) => Some((SkillScope::ControlUser, None, None)),
        ("project", Some(project)) => Some((
            SkillScope::ProjectcentralUser,
            None,
            Some(project.to_owned()),
        )),
        _ => None,
    }
}

fn skill_location(root: &Path, kind: &str, scope_ref: Option<&str>) -> Option<PathBuf> {
    let (scope, machine, project) = native_skill_scope(kind, scope_ref)?;
    resolve_skill_location(root, scope, machine.as_deref(), project.as_deref())
        .ok()
        .map(|l| l.skills_root)
}

fn skill_native_ref(kind: &str, scope_ref: Option<&str>, skill: &str) -> String {
    let scope_part = match (kind, scope_ref) {
        ("ground", _) => "ground".to_owned(),
        ("project", Some(project)) => format!("project.{project}"),
        _ => "unknown".to_owned(),
    };
    format!("central:skills:{scope_part}:{skill}:standing")
}

fn parse_skill_native_ref(native_ref: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = native_ref.split(':').collect();
    if parts.len() != 5 || parts[0] != "central" || parts[1] != "skills" || parts[4] != "standing" {
        return None;
    }
    let skill = parts[3].to_owned();
    if skill.is_empty() || skill.contains('/') {
        return None;
    }
    Some((parts[2].to_owned(), skill))
}

// ---------------------------------------------------------------------------
// Requested-value parsing for the writable setting (table rows).
// ---------------------------------------------------------------------------

struct Row {
    skill: String,
    standing: SkillStanding,
}

fn violation(code: &str, message: impl Into<String>, path: Option<&str>) -> Value {
    json!({ "code": code, "message": message.into(), "path": path })
}

/// Parse and natively validate one requested table value against one scope.
/// The owner's validation is authoritative (C0 §2.3).
fn validate_rows(root: &Path, kind: &str, scope_ref: Option<&str>, value: &Value) -> Vec<Value> {
    let mut violations = Vec::new();
    let Some(rows) = value.as_array() else {
        return vec![violation(
            "not_a_table",
            "the value must be a JSON array of {skill, standing} rows",
            None,
        )];
    };
    if rows.is_empty() {
        return vec![violation(
            "empty_table",
            "at least one row is required",
            None,
        )];
    }
    let mut seen = std::collections::BTreeSet::new();
    let states = read_scope_skills(root, kind, scope_ref);
    for (index, row) in rows.iter().enumerate() {
        let path = format!("/{index}");
        let Some(row_object) = row.as_object() else {
            violations.push(violation(
                "row_not_object",
                "each row must be an object",
                Some(&path),
            ));
            continue;
        };
        let Some(skill) = row_object.get("skill").and_then(Value::as_str) else {
            violations.push(violation(
                "missing_skill",
                "each row requires a string `skill`",
                Some(&path),
            ));
            continue;
        };
        if skill.is_empty() || skill.contains(':') || skill.contains('/') || skill.trim() != skill {
            violations.push(violation(
                "invalid_skill_name",
                "skill must be a non-empty name without separators or surrounding whitespace",
                Some(&path),
            ));
            continue;
        }
        if !seen.insert(skill.to_owned()) {
            violations.push(violation(
                "duplicate_skill",
                format!("skill {skill} appears more than once"),
                Some(&path),
            ));
            continue;
        }
        let requested = match row_object.get("standing").and_then(Value::as_str) {
            Some("active") => SkillStanding::Active,
            Some("retired") => SkillStanding::Retired,
            _ => {
                violations.push(violation(
                    "invalid_standing",
                    "standing must be \"active\" or \"retired\"",
                    Some(&path),
                ));
                continue;
            }
        };
        let Some(state) = states.iter().find(|s| s.name == skill) else {
            violations.push(violation(
                "unknown_skill",
                format!("no skill named {skill} exists at this scope"),
                Some(&path),
            ));
            continue;
        };
        if state.standing.is_none() {
            violations.push(violation(
                "manifest_missing",
                format!("skill {skill} has no authored manifest; standing is written into authored ground, never invented"),
                Some(&path),
            ));
            continue;
        }
        // Requested standing equal to current is valid (a no-change row);
        // plan drops it, so apply never re-executes a no-op transition.
        let _ = requested;
    }
    violations
}

fn parse_rows(value: &Value) -> Option<Vec<Row>> {
    let rows = value.as_array()?;
    let mut parsed = Vec::new();
    for row in rows {
        let object = row.as_object()?;
        let skill = object.get("skill")?.as_str()?.to_owned();
        let standing = match object.get("standing")?.as_str()? {
            "active" => SkillStanding::Active,
            "retired" => SkillStanding::Retired,
            _ => return None,
        };
        parsed.push(Row { skill, standing });
    }
    Some(parsed)
}

// ---------------------------------------------------------------------------
// Shared request plumbing for validate / plan.
// ---------------------------------------------------------------------------

/// The `oi.config-error/v1` document for a malformed request (the caller
/// lifts it into an `ActionResult` for its own action id).
fn invalid_request_doc(
    message: impl Into<String>,
    setting_ref: Option<&str>,
    scope_kind: Option<&str>,
) -> Value {
    config_error("validation_failed", message, setting_ref, scope_kind, false)
}

/// Resolve a validate/plan request into (setting, scope kind, scope ref,
/// scope doc, value). Every failure is the bare `oi.config-error/v1`
/// document. The inline value arrives as `input["value"]`;
/// `input["value_file"]` may name a path or `-` for stdin.
fn resolve_value_request(
    input: &Value,
) -> Result<(&'static SettingSpec, String, Option<String>, Value, Value), Value> {
    let setting_ref = input.get("setting_ref").and_then(Value::as_str);
    let Some(setting_ref) = setting_ref else {
        return Err(invalid_request_doc(
            "validate/plan requires --setting <setting_ref>.",
            None,
            None,
        ));
    };
    let Some(setting) = setting_by_ref(setting_ref) else {
        return Err(config_error(
            "unsupported_setting",
            format!("unknown setting: {setting_ref} is not part of the central contribution"),
            Some(setting_ref),
            None,
            false,
        ));
    };
    let scope_compact = input.get("scope").and_then(Value::as_str);
    let Some(scope_compact) = scope_compact else {
        return Err(invalid_request_doc(
            "every addressable configuration request carries an explicit scope: --scope <scope_kind|scope_kind:ref>",
            Some(setting_ref),
            None,
        ));
    };
    let (kind, scope_ref, scope_doc) = match parse_scope(scope_compact) {
        Ok(parsed) => parsed,
        Err(mut error) => {
            if error.get("setting_ref").is_none() || error["setting_ref"].is_null() {
                error["setting_ref"] = json!(setting_ref);
            }
            return Err(error);
        }
    };
    if !scope_allowed(setting, &kind) {
        return Err(config_error(
            "unsupported_scope",
            format!("setting {setting_ref} is not addressable at scope kind {kind}"),
            Some(setting_ref),
            Some(&kind),
            false,
        ));
    }
    let inline_value = input.get("value").filter(|v| !v.is_null());
    let value_file = input.get("value_file").and_then(Value::as_str);
    let value = match (inline_value, value_file) {
        (Some(value), None) => value.clone(),
        (None, Some(file)) => {
            let text = match read_document_source(file) {
                Ok(text) => text,
                Err(message) => {
                    return Err(invalid_request_doc(
                        format!("--value-file could not be read: {message}"),
                        Some(setting_ref),
                        Some(&kind),
                    ))
                }
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => value,
                Err(error) => {
                    return Err(invalid_request_doc(
                        format!("--value-file is not valid JSON: {error}"),
                        Some(setting_ref),
                        Some(&kind),
                    ))
                }
            }
        }
        (Some(_), Some(_)) => {
            return Err(invalid_request_doc(
                "pass either --value or --value-file, not both",
                Some(setting_ref),
                Some(&kind),
            ))
        }
        (None, None) => {
            return Err(invalid_request_doc(
                "a value is required: --value <json> or --value-file <path|->",
                Some(setting_ref),
                Some(&kind),
            ))
        }
    };
    Ok((setting, kind, scope_ref, scope_doc, value))
}

fn read_document_source(source: &str) -> Result<String, String> {
    if source == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .take((MAX_DOCUMENT_BYTES + 1) as u64)
            .read_to_string(&mut buffer)
            .map_err(|e| e.to_string())?;
        if buffer.len() > MAX_DOCUMENT_BYTES {
            return Err("document exceeds the bounded transport size".to_owned());
        }
        return Ok(buffer);
    }
    std::fs::read_to_string(source).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

fn validate_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = CONFIG_VALIDATE_ACTION;
    let (setting, kind, scope_ref, scope_doc, value) = match resolve_value_request(input) {
        Ok(resolved) => resolved,
        Err(document) => return config_failure(ACTION, document),
    };

    // Non-writable settings are honestly refused, read-only: the value can
    // never be applied, so the answer is a valid:false validation document.
    if !setting.writable {
        return ActionResult::success(
            ACTION,
            json!({
                "schema": CONFIG_VALIDATION_SCHEMA,
                "setting_ref": setting.setting_ref,
                "scope": scope_doc,
                "valid": false,
                "violations": [violation(
                    "not_writable",
                    "this setting is authored human ground; Central never mutates it through the configuration verbs (proposal/acceptance stays with Central's source-return path)",
                    None,
                )],
                "expected_effect": setting.effect,
            }),
        );
    }

    let (root, _availability) = match resolve_probed_root(context) {
        Ok(resolved) => resolved,
        Err(error) => {
            return error_result(
                ACTION,
                ResultStatus::UnavailableCapability,
                error
                    .get("error_code")
                    .and_then(Value::as_str)
                    .unwrap_or("owner_unavailable"),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("owner unavailable"),
                Some(setting.setting_ref),
                Some(&kind),
                true,
            )
        }
    };
    let violations = validate_rows(&root, &kind, scope_ref.as_deref(), &value);
    ActionResult::success(
        ACTION,
        json!({
            "schema": CONFIG_VALIDATION_SCHEMA,
            "setting_ref": setting.setting_ref,
            "scope": scope_doc,
            "valid": violations.is_empty(),
            "violations": violations,
            "expected_effect": setting.effect,
        }),
    )
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

/// Canonical plan body for the digest (C0 §6/§9, the same convention the
/// other owner lanes use): the plan document with `plan_digest` removed
/// (it cannot carry itself), `plan_id` zeroed, `expires_at_unix_ms` and
/// every `*_unix_ms` field zeroed.
pub fn canonical_plan_digest(plan: &Value) -> String {
    let mut canonical = plan.clone();
    if let Some(object) = canonical.as_object_mut() {
        object.remove("plan_digest");
        object.insert("plan_id".to_owned(), json!(""));
        object.insert("expires_at_unix_ms".to_owned(), json!(0));
    }
    zero_timestamps(&mut canonical);
    sha256_hex(
        serde_json::to_string(&canonical)
            .unwrap_or_default()
            .as_bytes(),
    )
}

fn plan_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = CONFIG_PLAN_ACTION;
    let (setting, kind, scope_ref, scope_doc, value) = match resolve_value_request(input) {
        Ok(resolved) => resolved,
        Err(document) => return config_failure(ACTION, document),
    };
    if !setting.writable {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "not_authorised",
            "authored human ground is never planned through the configuration verbs; Central's proposal/acceptance path owns authored change",
            Some(setting.setting_ref),
            Some(&kind),
            false,
        );
    }
    let (root, _) = match resolve_probed_root(context) {
        Ok(resolved) => resolved,
        Err(error) => {
            return error_result(
                ACTION,
                ResultStatus::UnavailableCapability,
                error
                    .get("error_code")
                    .and_then(Value::as_str)
                    .unwrap_or("owner_unavailable"),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("owner unavailable"),
                Some(setting.setting_ref),
                Some(&kind),
                true,
            )
        }
    };
    let violations = validate_rows(&root, &kind, scope_ref.as_deref(), &value);
    if !violations.is_empty() {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "invalid_value",
            format!(
                "owner-native validation failed with {} violation(s); use config validate for the list",
                violations.len()
            ),
            Some(setting.setting_ref),
            Some(&kind),
            false,
        );
    }
    let rows = parse_rows(&value).unwrap_or_default();
    let states = read_scope_skills(&root, &kind, scope_ref.as_deref());
    let mut changes = Vec::new();
    for row in rows {
        let Some(state) = states.iter().find(|s| s.name == row.skill) else {
            continue;
        };
        let Some(current) = state.standing else {
            continue;
        };
        if current == row.standing {
            continue;
        }
        let (verb, from, to) = match row.standing {
            SkillStanding::Retired => ("retire", "active", "retired"),
            SkillStanding::Active => ("restore", "retired", "active"),
        };
        changes.push(json!({
            "summary": format!("{verb} skill \"{}\": {from} → {to}", row.skill),
            "native_ref": skill_native_ref(&kind, scope_ref.as_deref(), &row.skill),
            "before_ref": format!("central:standing:{from}"),
            "after_ref": format!("central:standing:{to}"),
        }));
    }
    if changes.is_empty() {
        return invalid_request(
            ACTION,
            "every requested row already matches the current standing; there is nothing to plan",
            Some(setting.setting_ref),
            Some(&kind),
        );
    }
    let now = now_ms();
    let plan = json!({
        "schema": CONFIG_PLAN_SCHEMA,
        "plan_id": format!("central-plan-{}", uuid::Uuid::new_v4()),
        "plan_digest": Value::Null,
        "setting_ref": setting.setting_ref,
        "scope": scope_doc,
        "changes": changes,
        "expected_effect": setting.effect,
        "expires_at_unix_ms": now + PLAN_TTL_MS,
        "explain_ref": "control.skills.inspect",
        "authority": {
            "requires": [],
            "granted_by": "declared caller (recorded verbatim; the same authority the native control.skills.retire/restore Actions accept)",
        },
    });
    let digest = canonical_plan_digest(&plan);
    let mut plan = plan;
    plan["plan_digest"] = json!(digest);
    ActionResult::success(ACTION, plan)
}

// ---------------------------------------------------------------------------
// Receipts, history and owner-side idempotency (C0 §9).
// ---------------------------------------------------------------------------

struct ConfigStores {
    history_dir: PathBuf,
    receipts_index: PathBuf,
}

fn open_stores(root: &Path) -> ConfigStores {
    ConfigStores {
        history_dir: root.join(CONFIG_HISTORY_DIR),
        receipts_index: root.join(CONFIG_RECEIPTS_INDEX),
    }
}

/// The owner-side idempotency key digest over
/// (owner_ref, changeset_id, setting_ref, scope, plan_digest).
fn idempotency_key_digest(
    changeset_id: &str,
    setting_ref: &str,
    scope_doc: &Value,
    plan_digest: Option<&str>,
) -> String {
    let key = json!([
        CONFIG_OWNER_REF,
        changeset_id,
        setting_ref,
        scope_doc,
        plan_digest,
    ]);
    sha256_hex(serde_json::to_string(&key).unwrap_or_default().as_bytes())
}

fn load_receipt_index(stores: &ConfigStores) -> Value {
    std::fs::read_to_string(&stores.receipts_index)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "schema": CONFIG_RECEIPT_INDEX_SCHEMA, "keys": {} }))
}

fn record_receipt(
    stores: &ConfigStores,
    receipt: &Value,
    plan: Option<&Value>,
    results: &[Value],
) -> Result<(), String> {
    std::fs::create_dir_all(&stores.history_dir).map_err(|e| e.to_string())?;
    let receipt_id = receipt
        .get("receipt_id")
        .and_then(Value::as_str)
        .ok_or("receipt has no id")?;
    if !receipt_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("receipt id must be a plain filename".to_owned());
    }
    let history = json!({
        "schema": CONFIG_HISTORY_SCHEMA,
        "receipt": receipt,
        "plan": plan,
        "results": results,
    });
    std::fs::write(
        stores.history_dir.join(format!("{receipt_id}.json")),
        serde_json::to_vec_pretty(&history).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let mut index = load_receipt_index(stores);
    let key = receipt
        .get("idempotency_key_digest")
        .and_then(Value::as_str)
        .ok_or("receipt has no idempotency key digest")?
        .to_owned();
    // Only an executed (applied) operation may claim its idempotency key. A
    // failed apply is history, not an executed key: replaying it must retry,
    // never answer no_op (C0 §9 — "re-submitting an executed key").
    let executed = receipt.get("outcome").and_then(Value::as_str) == Some("applied");
    if executed {
        if let Some(object) = index.as_object_mut() {
            object.insert("keys".to_owned(), {
                let mut keys = object
                    .get("keys")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                keys.insert(
                    key,
                    json!({
                        "receipt_id": receipt_id,
                        "recorded_at_unix_ms": now_ms(),
                    }),
                );
                json!(keys)
            });
        }
    }
    std::fs::write(
        &stores.receipts_index,
        serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The owner-side idempotency law: a replayed executed key returns outcome
/// `no_op` naming the original receipt, and the owner must not re-execute.
fn replay_receipt(
    stores: &ConfigStores,
    key_digest: &str,
    setting_ref: &str,
    scope_doc: &Value,
    changeset_id: &str,
    plan_digest: Option<&str>,
) -> Option<ActionResult> {
    let index = load_receipt_index(stores);
    let original_id = index
        .get("keys")
        .and_then(|keys| keys.get(key_digest))
        .and_then(|entry| entry.get("receipt_id"))
        .and_then(Value::as_str)?;
    let path = stores.history_dir.join(format!("{}.json", original_id));
    let text = std::fs::read_to_string(path).ok()?;
    let history: Value = serde_json::from_str(&text).ok()?;
    let original = history.get("receipt")?;
    let mut replay = original.clone();
    replay["schema"] = json!(CONFIG_RECEIPT_SCHEMA);
    replay["receipt_id"] = json!(format!("{original_id}-replay"));
    replay["outcome"] = json!("no_op");
    replay["original_receipt_id"] = json!(original_id);
    replay["setting_ref"] = json!(setting_ref);
    replay["scope"] = scope_doc.clone();
    replay["changeset_id"] = json!(changeset_id);
    replay["plan_digest"] = json!(plan_digest);
    replay["error"] = Value::Null;
    // Owner-internal bookkeeping never reaches the wire document.
    if let Some(object) = replay.as_object_mut() {
        object.remove("idempotency_key_digest");
    }
    Some(ActionResult::success(CONFIG_APPLY_ACTION, replay))
}

fn normalise_changeset(input_changeset: Option<&str>, plan_digest: &str) -> Result<String, String> {
    match input_changeset {
        Some(changeset) => {
            let valid = !changeset.is_empty()
                && changeset.starts_with("cs-")
                && changeset
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if valid {
                Ok(changeset.to_owned())
            } else {
                Err(format!(
                    "--changeset must match cs-<id> with letters, digits, '_' or '-': got {changeset}"
                ))
            }
        }
        None => Ok(format!(
            "cs-plan-{}",
            &plan_digest[..16.min(plan_digest.len())]
        )),
    }
}

// ---------------------------------------------------------------------------
// apply
// ---------------------------------------------------------------------------

fn apply_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = CONFIG_APPLY_ACTION;
    let (root, _) = match resolve_probed_root(context) {
        Ok(resolved) => resolved,
        Err(error) => {
            return error_result(
                ACTION,
                ResultStatus::UnavailableCapability,
                "owner_unavailable",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("owner unavailable"),
                None,
                None,
                true,
            )
        }
    };
    let plan_file = input.get("plan_file").and_then(Value::as_str);
    let Some(plan_source) = plan_file else {
        return invalid_request(
            ACTION,
            "config apply requires --plan-file <path|->",
            None,
            None,
        );
    };
    let text = match read_document_source(plan_source) {
        Ok(text) => text,
        Err(message) => {
            return invalid_request(
                ACTION,
                format!("--plan-file could not be read: {message}"),
                None,
                None,
            )
        }
    };
    let plan: Value = match serde_json::from_str(&text) {
        Ok(plan) => plan,
        Err(error) => {
            return invalid_request(
                ACTION,
                format!("--plan-file is not valid JSON: {error}"),
                None,
                None,
            )
        }
    };
    let setting_ref = plan
        .get("setting_ref")
        .and_then(Value::as_str)
        .unwrap_or("");
    match plan.get("schema").and_then(Value::as_str) {
        Some(CONFIG_PLAN_SCHEMA) => {}
        other => {
            return error_result(
                ACTION,
                ResultStatus::InvalidInput,
                "unsupported_schema",
                format!(
                    "plan schema must be {CONFIG_PLAN_SCHEMA} (got {})",
                    other.unwrap_or("nothing")
                ),
                None,
                None,
                false,
            )
        }
    };
    let Some(setting) = setting_by_ref(setting_ref) else {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unsupported_setting",
            format!("plan names an unknown setting: {setting_ref}"),
            Some(setting_ref),
            None,
            false,
        );
    };
    if !setting.writable {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "not_authorised",
            "authored human ground is never applied through the configuration verbs",
            Some(setting.setting_ref),
            None,
            false,
        );
    }
    let scope_kind = plan
        .pointer("/scope/scope_kind")
        .and_then(Value::as_str)
        .unwrap_or("");
    let scope_ref = plan.pointer("/scope/scope_ref").and_then(Value::as_str);
    let scope_doc = scope_document(scope_kind, scope_ref);
    if !SCOPE_KINDS.contains(&scope_kind) {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unknown_scope_kind",
            format!("plan carries an unknown scope kind: {scope_kind}"),
            Some(setting.setting_ref),
            Some(scope_kind),
            false,
        );
    }
    if !scope_allowed(setting, scope_kind) {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unsupported_scope",
            format!(
                "setting {} is not addressable at scope kind {scope_kind}",
                setting.setting_ref
            ),
            Some(setting.setting_ref),
            Some(scope_kind),
            false,
        );
    }
    let plan_digest = plan
        .get("plan_digest")
        .and_then(Value::as_str)
        .unwrap_or("");
    if plan_digest.is_empty() || canonical_plan_digest(&plan) != plan_digest {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "validation_failed",
            "plan_digest does not match the plan body; the plan is not the owner-minted document",
            Some(setting.setting_ref),
            Some(scope_kind),
            false,
        );
    }
    if let Some(expires) = plan.get("expires_at_unix_ms").and_then(Value::as_u64) {
        if expires != 0 && expires < now_ms() {
            return error_result(
                ACTION,
                ResultStatus::InvalidInput,
                "plan_expired",
                "the plan has expired; plan again",
                Some(setting.setting_ref),
                Some(scope_kind),
                true,
            );
        }
    }
    let changeset_id =
        match normalise_changeset(input.get("changeset").and_then(Value::as_str), plan_digest) {
            Ok(changeset) => changeset,
            Err(message) => {
                return invalid_request(
                    ACTION,
                    message,
                    Some(setting.setting_ref),
                    Some(scope_kind),
                )
            }
        };

    let stores = open_stores(&root);
    let key_digest = idempotency_key_digest(
        &changeset_id,
        setting.setting_ref,
        &scope_doc,
        Some(plan_digest),
    );
    if let Some(replay) = replay_receipt(
        &stores,
        &key_digest,
        setting.setting_ref,
        &scope_doc,
        &changeset_id,
        Some(plan_digest),
    ) {
        return replay;
    }

    // Execute through Central's own native verbs, attributed verbatim.
    let attribution = format!("config-plane:{changeset_id}");
    let native_scope = native_skill_scope(scope_kind, scope_ref);
    let mut results: Vec<Value> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    if let Some(changes) = plan.get("changes").and_then(Value::as_array) {
        for change in changes {
            let native_ref = change
                .get("native_ref")
                .and_then(Value::as_str)
                .unwrap_or("");
            let Some((_scope_part, skill)) = parse_skill_native_ref(native_ref) else {
                failures.push(format!(
                    "plan change carries an unparseable native_ref: {native_ref}"
                ));
                continue;
            };
            let Some(target) = change
                .get("after_ref")
                .and_then(Value::as_str)
                .and_then(|after| after.strip_prefix("central:standing:"))
            else {
                failures.push(format!("plan change for {skill} carries no after_ref"));
                continue;
            };
            let Some((scope, machine, project)) = native_scope.as_ref() else {
                failures.push("scope no longer resolves to a native skills scope".to_owned());
                continue;
            };
            let current = read_scope_skills(&root, scope_kind, scope_ref)
                .into_iter()
                .find(|s| s.name == skill)
                .and_then(|s| s.standing);
            let target_standing = match target {
                "retired" => SkillStanding::Retired,
                "active" => SkillStanding::Active,
                other => {
                    failures.push(format!(
                        "plan change for {skill} carries an unknown target standing: {other}"
                    ));
                    continue;
                }
            };
            if current == Some(target_standing) {
                results.push(json!({
                    "skill": skill, "action": "skip", "ok": true,
                    "detail": "already at the planned standing",
                }));
                continue;
            }
            let action_name = if target == "retired" {
                "retire"
            } else {
                "restore"
            };
            let outcome = match target_standing {
                SkillStanding::Retired => retire_skill(
                    &root,
                    *scope,
                    machine.as_deref(),
                    project.as_deref(),
                    &skill,
                    &attribution,
                    change
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or("configuration-plane apply"),
                )
                .map(|_| ()),
                SkillStanding::Active => restore_skill(
                    &root,
                    *scope,
                    machine.as_deref(),
                    project.as_deref(),
                    &skill,
                )
                .map(|_| ()),
            };
            match outcome {
                Ok(()) => results.push(json!({
                    "skill": skill,
                    "action": action_name,
                    "ok": true,
                    "detail": Value::Null,
                })),
                Err(error) => {
                    failures.push(format!("{skill}: {error}"));
                    results.push(json!({
                        "skill": skill,
                        "action": action_name,
                        "ok": false,
                        "detail": error.to_string(),
                    }));
                }
            }
        }
    }

    let receipt_id = format!("central-receipt-{}", uuid::Uuid::new_v4());
    let expected_effect = plan.get("expected_effect").cloned().unwrap_or(Value::Null);
    let (outcome, error, status) = if failures.is_empty() {
        ("applied", Value::Null, ResultStatus::Success)
    } else {
        (
            "failed",
            json!({
                "code": "validation_failed",
                "message": failures.join("; "),
                "retryable": true,
            }),
            ResultStatus::VerificationFailure,
        )
    };
    let mut receipt = json!({
        "schema": CONFIG_RECEIPT_SCHEMA,
        "receipt_id": receipt_id,
        "owner_ref": CONFIG_OWNER_REF,
        "changeset_id": changeset_id,
        "plan_digest": plan_digest,
        "setting_ref": setting.setting_ref,
        "scope": scope_doc,
        "operation": "apply",
        "outcome": outcome,
        "applied_at_unix_ms": now_ms(),
        "native_ref": format!("central:config-history:{receipt_id}"),
        "expected_effect": expected_effect,
        "original_receipt_id": Value::Null,
        "error": error,
    });
    receipt["idempotency_key_digest"] = json!(key_digest);
    if let Err(message) = record_receipt(&stores, &receipt, Some(&plan), &results) {
        return error_result(
            ACTION,
            ResultStatus::InternalFailure,
            "internal",
            format!("receipt history could not be recorded: {message}"),
            Some(setting.setting_ref),
            Some(scope_kind),
            true,
        );
    }
    // The idempotency key digest is owner-internal bookkeeping; the wire
    // document carries only the frozen receipt fields.
    receipt
        .as_object_mut()
        .unwrap()
        .remove("idempotency_key_digest");
    ActionResult {
        ok: status == ResultStatus::Success,
        status,
        action: Some(ACTION.to_owned()),
        data: Some(receipt),
        error: None,
    }
}

// ---------------------------------------------------------------------------
// reset
// ---------------------------------------------------------------------------

fn reset_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    const ACTION: &str = CONFIG_RESET_ACTION;
    let setting_ref = input.get("setting_ref").and_then(Value::as_str);
    let Some(setting_ref) = setting_ref else {
        return invalid_request(
            ACTION,
            "config reset requires --setting <setting_ref>",
            None,
            None,
        );
    };
    let Some(setting) = setting_by_ref(setting_ref) else {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unsupported_setting",
            format!("unknown setting: {setting_ref} is not part of the central contribution"),
            Some(setting_ref),
            None,
            false,
        );
    };
    if !setting.writable {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "not_authorised",
            "authored human ground has no configuration reset; Central's proposal/acceptance path owns authored change",
            Some(setting.setting_ref),
            None,
            false,
        );
    }
    let scope_compact = input.get("scope").and_then(Value::as_str);
    let Some(scope_compact) = scope_compact else {
        return invalid_request(
            ACTION,
            "config reset carries an explicit scope: --scope <scope_kind|scope_kind:ref>",
            Some(setting.setting_ref),
            None,
        );
    };
    let (kind, scope_ref, scope_doc) = match parse_scope(scope_compact) {
        Ok(parsed) => parsed,
        Err(error) => {
            let scope_kind = error
                .get("scope_kind")
                .and_then(Value::as_str)
                .map(str::to_owned);
            return error_result(
                ACTION,
                ResultStatus::InvalidInput,
                error
                    .get("error_code")
                    .and_then(Value::as_str)
                    .unwrap_or("validation_failed"),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("invalid scope"),
                Some(setting.setting_ref),
                scope_kind.as_deref(),
                false,
            );
        }
    };
    if !scope_allowed(setting, &kind) {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unsupported_scope",
            format!(
                "setting {} is not addressable at scope kind {kind}",
                setting.setting_ref
            ),
            Some(setting.setting_ref),
            Some(&kind),
            false,
        );
    }
    let (root, _) = match resolve_probed_root(context) {
        Ok(resolved) => resolved,
        Err(error) => {
            return error_result(
                ACTION,
                ResultStatus::UnavailableCapability,
                "owner_unavailable",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("owner unavailable"),
                Some(setting.setting_ref),
                Some(&kind),
                true,
            )
        }
    };
    let changeset_id = match input.get("changeset").and_then(Value::as_str) {
        Some(changeset) => match normalise_changeset(Some(changeset), "") {
            Ok(changeset) => changeset,
            Err(message) => {
                return invalid_request(ACTION, message, Some(setting.setting_ref), Some(&kind))
            }
        },
        None => format!("cs-reset-{}", uuid::Uuid::new_v4().simple()),
    };

    let stores = open_stores(&root);
    let key_digest = idempotency_key_digest(&changeset_id, setting.setting_ref, &scope_doc, None);
    if let Some(replay) = replay_receipt(
        &stores,
        &key_digest,
        setting.setting_ref,
        &scope_doc,
        &changeset_id,
        None,
    ) {
        let mut replay = replay;
        replay.action = Some(ACTION.to_owned());
        return replay;
    }

    // Reset returns every retired manifest-bearing skill in scope to the
    // authored baseline standing (active) through the native restore verb.
    let Some((native_scope, machine, project)) = native_skill_scope(&kind, scope_ref.as_deref())
    else {
        return error_result(
            ACTION,
            ResultStatus::InvalidInput,
            "unsupported_scope",
            format!(
                "setting {} has no reset at scope kind {kind}",
                setting.setting_ref
            ),
            Some(setting.setting_ref),
            Some(&kind),
            false,
        );
    };
    let states = read_scope_skills(&root, &kind, scope_ref.as_deref());
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut restored = 0usize;
    for state in &states {
        let Some(SkillStanding::Retired) = state.standing else {
            continue;
        };
        let outcome = restore_skill(
            &root,
            native_scope,
            machine.as_deref(),
            project.as_deref(),
            &state.name,
        );
        match outcome {
            Ok(_) => {
                restored += 1;
                results.push(json!({
                    "skill": state.name, "action": "restore", "ok": true,
                    "detail": Value::Null,
                }));
            }
            Err(error) => {
                failures.push(format!("{}: {error}", state.name));
                results.push(json!({
                    "skill": state.name, "action": "restore", "ok": false,
                    "detail": error.to_string(),
                }));
            }
        }
    }
    let receipt_id = format!("central-receipt-{}", uuid::Uuid::new_v4());
    let (outcome, error, status) = if failures.is_empty() {
        ("applied", Value::Null, ResultStatus::Success)
    } else {
        (
            "failed",
            json!({
                "code": "validation_failed",
                "message": failures.join("; "),
                "retryable": true,
            }),
            ResultStatus::VerificationFailure,
        )
    };
    let mut receipt = json!({
        "schema": CONFIG_RECEIPT_SCHEMA,
        "receipt_id": receipt_id,
        "owner_ref": CONFIG_OWNER_REF,
        "changeset_id": changeset_id,
        "plan_digest": Value::Null,
        "setting_ref": setting.setting_ref,
        "scope": scope_doc,
        "operation": "reset",
        "outcome": outcome,
        "applied_at_unix_ms": now_ms(),
        "native_ref": format!("central:config-history:{receipt_id}"),
        "expected_effect": setting.effect,
        "original_receipt_id": Value::Null,
        "error": error,
        "restored_count": restored,
    });
    receipt["idempotency_key_digest"] = json!(key_digest);
    if let Err(message) = record_receipt(&stores, &receipt, None, &results) {
        return error_result(
            ACTION,
            ResultStatus::InternalFailure,
            "internal",
            format!("receipt history could not be recorded: {message}"),
            Some(setting.setting_ref),
            Some(&kind),
            true,
        );
    }
    receipt
        .as_object_mut()
        .unwrap()
        .remove("idempotency_key_digest");
    let _ = restored;
    ActionResult {
        ok: status == ResultStatus::Success,
        status,
        action: Some(ACTION.to_owned()),
        data: Some(receipt),
        error: None,
    }
}

// ---------------------------------------------------------------------------
// contribution action + registration
// ---------------------------------------------------------------------------

fn contribution_action(
    _registry: &ActionRegistry,
    _input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    match build_contribution(context.root_options) {
        Ok(document) => ActionResult::success(CONFIG_CONTRIBUTION_ACTION, document),
        Err(message) => error_result(
            CONFIG_CONTRIBUTION_ACTION,
            ResultStatus::UnavailableCapability,
            "owner_unavailable",
            message,
            None,
            None,
            true,
        ),
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    output_type: &str,
    mutation_class: MutationClass,
    inputs: Vec<ActionInputDefinition>,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.to_owned(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: vec![],
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

pub fn register_configuration_actions(registry: &mut ActionRegistry) {
    let _ = registry.register(
        descriptor(
            CONFIG_CONTRIBUTION_ACTION,
            "Central configuration contribution",
            "Emit Central's bare oi.configuration-contribution/v1 document: the addressable settings, their scopes, value contracts and the owner-native mutation transport.",
            CONFIG_CONTRIBUTION_SCHEMA,
            MutationClass::ReadOnly,
            vec![],
        ),
        contribution_action,
    );
    let _ = registry.register(
        descriptor(
            CONFIG_VALIDATE_ACTION,
            "Validate a configuration value",
            "Owner-native validation of one requested value at one explicit scope; answers oi.config-validation/v1.",
            CONFIG_VALIDATION_SCHEMA,
            MutationClass::ReadOnly,
            vec![
                ActionInputDefinition { name: "setting_ref".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "scope".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "value".to_owned(), input_type: "object".to_owned(), required: true, choices: None, selection: None },
            ],
        ),
        validate_action,
    );
    let _ = registry.register(
        descriptor(
            CONFIG_PLAN_ACTION,
            "Plan a configuration change",
            "Owner-native plan/explain for one requested change; answers oi.config-plan/v1 with an owner-minted plan_id and plan_digest.",
            CONFIG_PLAN_SCHEMA,
            MutationClass::ReadOnly,
            vec![
                ActionInputDefinition { name: "setting_ref".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "scope".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "value".to_owned(), input_type: "object".to_owned(), required: true, choices: None, selection: None },
            ],
        ),
        plan_action,
    );
    let _ = registry.register(
        descriptor(
            CONFIG_APPLY_ACTION,
            "Apply a configuration plan",
            "Execute an owner-minted plan through Central's own native verbs; answers oi.config-receipt/v1. Idempotent per (owner, changeset, setting, scope, plan_digest).",
            CONFIG_RECEIPT_SCHEMA,
            MutationClass::LocallyMutating,
            vec![
                ActionInputDefinition { name: "plan_file".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "changeset".to_owned(), input_type: "string".to_owned(), required: false, choices: None, selection: None },
            ],
        ),
        apply_action,
    );
    let _ = registry.register(
        descriptor(
            CONFIG_RESET_ACTION,
            "Reset a configuration setting",
            "Return a writable setting to its owner baseline through Central's own native verbs; answers oi.config-receipt/v1.",
            CONFIG_RECEIPT_SCHEMA,
            MutationClass::LocallyMutating,
            vec![
                ActionInputDefinition { name: "setting_ref".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "scope".to_owned(), input_type: "string".to_owned(), required: true, choices: None, selection: None },
                ActionInputDefinition { name: "changeset".to_owned(), input_type: "string".to_owned(), required: false, choices: None, selection: None },
            ],
        ),
        reset_action,
    );
}
