//! Configuration-plane contract tests for Central's owner contribution
//! (`oi.configuration-contribution/v1`) and its owner-native mutation
//! transport (`ctrl config validate|plan|apply|reset`), frozen by O-I
//! `docs/cradle/09-CONFIGURATION-PLANE.md` (#299 Gate A / C0, lane C3A).
//!
//! The JSON Schemas under `fixtures/configuration/` are byte-exact copies of
//! the frozen C0 contract schemas (`schemas/oi.*.schema.json` in the O-I
//! c1-kernel reference checkout, Gate A). Every document the owner emits is
//! validated against them here, so contract drift fails in this repository
//! and not at the O-I mount.
//!
//! Every invocation addresses a throwaway Central root through `--root`;
//! the real `~/Central` ground is never touched.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CONTRIBUTION: &str = "oi.configuration-contribution/v1";
const VALIDATION: &str = "oi.config-validation/v1";
const PLAN: &str = "oi.config-plan/v1";
const RECEIPT: &str = "oi.config-receipt/v1";
const ERROR: &str = "oi.config-error/v1";

const WRITABLE_SETTING: &str = "central:skills:central.skills";
const POLICY_SETTING: &str = "central:policy:civil-time.timezone";
const POLICY_SETTING_PLACEMENT: &str = "central:policy:placement.enforcement";

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ctrl")
}

fn fixture_schema(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/configuration")
        .join(name)
}

/// A throwaway Central root; the real ground is never addressed.
struct TempRoot(PathBuf);

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

impl TempRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-config-plane-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        // The full required-root skeleton so the probed availability is the
        // honest "available", not a degradation of a half-built root.
        for dir in [
            "Control/user",
            "Control/agents/governance",
            "Control/agents/wiki",
            "Control/machines",
            ".central",
            "Work",
        ] {
            fs::create_dir_all(path.join(dir)).unwrap();
        }
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[String]) -> Output {
    Command::new(binary()).args(args).output().unwrap()
}

fn run_with_stdin(args: &[String], input: &str) -> Output {
    let mut child = Command::new(binary())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn root_args(root: &Path, tail: &[&str]) -> Vec<String> {
    let mut args = vec!["--root".to_owned(), root.display().to_string()];
    args.extend(tail.iter().map(|flag| (*flag).to_owned()));
    args
}

/// Parse stdout as one bare JSON document; the configuration plane never
/// answers with an Action envelope.
fn json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not one bare JSON document: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_satisfies(document: &Value, schema_file: &str) {
    let raw = fs::read_to_string(fixture_schema(schema_file))
        .unwrap_or_else(|error| panic!("read frozen schema {schema_file}: {error}"));
    let schema: Value = serde_json::from_str(&raw).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(document)
        .map(|error| format!("{error} (at {})", error.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "document does not satisfy the frozen schema {schema_file}:\n{}",
        errors.join("\n")
    );
}

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
            for child in items {
                zero_timestamps(child);
            }
        }
        _ => {}
    }
}

fn sha256_of(value: &Value) -> String {
    let bytes = serde_json::to_string(value).unwrap();
    let digest = Sha256::digest(bytes.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Recompute the canonical plan digest the owner documents: sha256 over the
/// plan with `plan_digest` removed, `plan_id`, `expires_at_unix_ms` and every
/// `*_unix_ms` zeroed.
fn canonical_plan_digest(plan: &Value) -> String {
    let mut canonical = plan.clone();
    if let Some(object) = canonical.as_object_mut() {
        object.remove("plan_digest");
        object.insert("plan_id".to_owned(), json!(""));
        object.insert("expires_at_unix_ms".to_owned(), json!(0));
    }
    zero_timestamps(&mut canonical);
    sha256_of(&canonical)
}

fn seed_skill(root: &Path, name: &str, standing: &str) {
    let dir = root.join("Control/user/skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\n---\nBody."),
    )
    .unwrap();
    fs::write(
        dir.join("skill.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "central.skill/v1",
            "name": name,
            "scope": "control-user",
            "standing": standing,
            "provenance": "human-authored"
        }))
        .unwrap(),
    )
    .unwrap();
}

fn skill_manifest(root: &Path, name: &str) -> Value {
    let raw = fs::read_to_string(
        root.join("Control/user/skills")
            .join(name)
            .join("skill.json"),
    )
    .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn retire_value(skill: &str, standing: &str) -> String {
    json!([{ "skill": skill, "standing": standing }]).to_string()
}

// ---------------------------------------------------------------------------
// The contribution document.
// ---------------------------------------------------------------------------

#[test]
fn contribution_is_a_bare_document_that_satisfies_the_frozen_schema() {
    let temp = TempRoot::new();
    let output = run(&root_args(temp.path(), &["config-contribution", "--json"]));
    assert!(output.status.success());
    let document = json_stdout(&output);

    // Bare document, not an Action envelope.
    assert_eq!(document["schema"], CONTRIBUTION);
    assert!(document.get("status").is_none());
    assert!(document.get("ok").is_none());
    assert_satisfies(&document, "oi.configuration-contribution-v1.schema.json");

    // Owner identity: product_id `central`, the frozen discovery relation.
    assert_eq!(document["owner"]["owner_ref"], "central");
    assert_eq!(document["owner"]["owner_kind"], "product");
    assert_eq!(
        document["owner"]["contribution_command"],
        json!(["ctrl", "config-contribution", "--json"])
    );
    assert_eq!(
        document["contract_revision"],
        "configuration-plane/contribution.1"
    );

    // Availability is probed; a fully-skeletonised temp root is available.
    assert_eq!(document["availability"]["state"], "available");

    // The reading digest follows the 07 §4.5 convention exactly.
    let mut canonical = document.clone();
    canonical["owner"]["reading_digest"] = Value::Null;
    zero_timestamps(&mut canonical);
    assert_eq!(
        document["owner"]["reading_digest"],
        json!(sha256_of(&canonical)),
        "reading_digest is not the documented digest of this document"
    );

    // Read/operability plane separation (C0 §1/§17): no native axes in a
    // contribution document.
    let raw = serde_json::to_string(&document).unwrap();
    assert!(
        !raw.contains("\"declared\""),
        "contribution carries declared"
    );
    assert!(
        !raw.contains("\"effective\""),
        "contribution carries effective"
    );
    assert!(!raw.contains("\"active\""), "contribution carries active");
}

#[test]
fn contribution_settings_carry_the_central_law() {
    let temp = TempRoot::new();
    let output = run(&root_args(temp.path(), &["config-contribution", "--json"]));
    let document = json_stdout(&output);
    assert_satisfies(&document, "oi.configuration-contribution-v1.schema.json");

    let settings: Vec<Value> = document["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["settings"].as_array().unwrap().clone())
        .collect();
    assert!(!settings.is_empty());

    for setting in &settings {
        let setting_ref = setting["setting_ref"].as_str().unwrap();
        // Ref grammar and owner part (C0 §3).
        let parts: Vec<&str> = setting_ref.split(':').collect();
        assert_eq!(parts.len(), 3, "{setting_ref} does not parse");
        assert_eq!(parts[0], "central", "{setting_ref} owner part");
        assert_eq!(parts[1], setting["section_ref"], "{setting_ref} section");
        // The enclosing section exists with that id.
        assert!(
            document["sections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| { s["id"] == setting["section_ref"] }),
            "section {} not declared",
            setting["section_ref"]
        );
    }

    // Authored human ground is disclosed read-only, never profileable.
    let policy: Vec<&Value> = settings
        .iter()
        .filter(|s| s["section_ref"] == "policy")
        .collect();
    assert!(!policy.is_empty(), "policy ground is not disclosed");
    for setting in policy {
        assert_eq!(
            setting["writable"],
            json!(false),
            "{}",
            setting["setting_ref"]
        );
        assert_eq!(
            setting["profileable"],
            json!(false),
            "{}",
            setting["setting_ref"]
        );
        assert_eq!(setting["effect"]["kind"], "none");
        assert_eq!(setting["operations"]["apply"], json!(false));
        assert_eq!(setting["operations"]["reset"], json!(false));
    }

    // The one writable setting is state ctrl already mutates natively, and
    // its ref maps structurally onto the v2 disclosure (C0 §17).
    let skills = settings
        .iter()
        .find(|s| s["setting_ref"] == WRITABLE_SETTING)
        .expect("the writable skill-standing setting is contributed");
    assert_eq!(skills["writable"], json!(true));
    assert_eq!(skills["section_ref"], "skills");
    assert_eq!(skills["operations"], {
        let mut o = serde_json::Map::new();
        for key in ["validate", "plan", "apply", "reset"] {
            o.insert(key.to_owned(), json!(true));
        }
        json!(o)
    });

    // And the same section id / setting key really exist in the v2 plane.
    let disclosure_output = run(&root_args(temp.path(), &["system", "--json"]));
    assert!(disclosure_output.status.success());
    let disclosure = json_stdout(&disclosure_output);
    assert_eq!(disclosure["schema"], "oi.product-settings-disclosure/v2");
    let mapped = disclosure["sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|section| {
            section["id"] == "skills"
                && section["settings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["key"] == "central.skills")
        });
    assert!(
        mapped,
        "contribution ref {:?} has no structural v2 counterpart",
        WRITABLE_SETTING
    );
}

// ---------------------------------------------------------------------------
// validate / plan / apply / reset round trip against a throwaway root.
// ---------------------------------------------------------------------------

#[test]
fn validate_plan_apply_replay_reset_round_trip() {
    let temp = TempRoot::new();
    seed_skill(temp.path(), "alpha-skill", "active");
    seed_skill(temp.path(), "beta-skill", "active");

    // -- validate: honest owner-native answer against the frozen schema. --
    let validate = run(&root_args(
        temp.path(),
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    assert!(validate.status.success());
    let validation = json_stdout(&validate);
    assert_eq!(validation["schema"], VALIDATION);
    assert_satisfies(&validation, "oi.config-validation-v1.schema.json");
    assert_eq!(validation["valid"], json!(true));
    assert_eq!(validation["scope"]["scope_kind"], "ground");

    // An invalid standing is an owner-native violation, not a CLI usage error.
    let bad = run(&root_args(
        temp.path(),
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "deleted"),
        ],
    ));
    assert!(bad.status.success());
    let bad_validation = json_stdout(&bad);
    assert_eq!(bad_validation["valid"], json!(false));
    assert_eq!(bad_validation["violations"][0]["code"], "invalid_standing");

    // An unknown skill is refused: standing is written into authored ground,
    // never invented.
    let unknown = run(&root_args(
        temp.path(),
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("no-such-skill", "retired"),
        ],
    ));
    assert_eq!(
        json_stdout(&unknown)["violations"][0]["code"],
        "unknown_skill"
    );

    // A manifest-less skill directory is unresolved and never touched.
    fs::create_dir_all(temp.path().join("Control/user/skills/ghost-skill")).unwrap();
    let ghost = run(&root_args(
        temp.path(),
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("ghost-skill", "retired"),
        ],
    ));
    assert_eq!(
        json_stdout(&ghost)["violations"][0]["code"],
        "manifest_missing"
    );

    // -- plan: owner-minted plan with a verifiable digest. ---------------
    let plan = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    assert!(plan.status.success());
    let plan_document = json_stdout(&plan);
    assert_eq!(plan_document["schema"], PLAN);
    assert_satisfies(&plan_document, "oi.config-plan-v1.schema.json");
    assert_eq!(
        plan_document["plan_digest"],
        json!(canonical_plan_digest(&plan_document)),
        "plan_digest is not the digest of the canonical plan body"
    );
    assert!(plan_document["plan_id"]
        .as_str()
        .unwrap()
        .starts_with("central-plan-"));

    // --value-file - reads stdin (no argv limits).
    let plan_via_stdin = run_with_stdin(
        &root_args(
            temp.path(),
            &[
                "config",
                "plan",
                "--json",
                "--setting",
                WRITABLE_SETTING,
                "--scope",
                "ground",
                "--value-file",
                "-",
            ],
        ),
        &retire_value("alpha-skill", "retired"),
    );
    assert!(plan_via_stdin.status.success());
    assert_eq!(json_stdout(&plan_via_stdin)["schema"], PLAN);

    // -- apply: executes through the native verbs; receipt + history. ----
    let plan_path = temp.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan_document).unwrap(),
    )
    .unwrap();
    let changeset = "cs-test-round-trip-1";
    let apply = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
            "--changeset",
            changeset,
        ],
    ));
    assert!(apply.status.success());
    let receipt = json_stdout(&apply);
    assert_eq!(receipt["schema"], RECEIPT);
    assert_satisfies(&receipt, "oi.config-receipt-v1.schema.json");
    assert_eq!(receipt["outcome"], "applied");
    assert_eq!(receipt["owner_ref"], "central");
    assert_eq!(receipt["changeset_id"], changeset);
    assert_eq!(receipt["plan_digest"], plan_document["plan_digest"]);
    let original_receipt_id = receipt["receipt_id"].as_str().unwrap().to_owned();
    // native_ref points into Central's own config history.
    let native_ref = receipt["native_ref"].as_str().unwrap();
    assert!(native_ref.starts_with("central:config-history:"));
    let history_file = temp
        .path()
        .join(".central/config/history")
        .join(format!("{original_receipt_id}.json"));
    assert!(history_file.is_file(), "receipt history was not recorded");
    // The manifest really moved.
    assert_eq!(
        skill_manifest(temp.path(), "alpha-skill")["standing"],
        "retired"
    );

    // -- idempotent replay: no_op naming the original receipt. -----------
    let replay = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
            "--changeset",
            changeset,
        ],
    ));
    assert!(replay.status.success());
    let replay_receipt = json_stdout(&replay);
    assert_satisfies(&replay_receipt, "oi.config-receipt-v1.schema.json");
    assert_eq!(replay_receipt["outcome"], "no_op");
    assert_eq!(
        replay_receipt["original_receipt_id"],
        json!(original_receipt_id),
    );
    assert_ne!(replay_receipt["receipt_id"], json!(original_receipt_id));

    // -- reset: returns the scope to the authored baseline. --------------
    let reset_changeset = "cs-test-reset-1";
    let reset = run(&root_args(
        temp.path(),
        &[
            "config",
            "reset",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--changeset",
            reset_changeset,
        ],
    ));
    assert!(reset.status.success());
    let reset_receipt = json_stdout(&reset);
    assert_satisfies(&reset_receipt, "oi.config-receipt-v1.schema.json");
    assert_eq!(reset_receipt["operation"], "reset");
    assert_eq!(reset_receipt["outcome"], "applied");
    assert_eq!(
        skill_manifest(temp.path(), "alpha-skill")["standing"],
        "active"
    );

    // Reset replay is also idempotent under its key.
    let reset_replay = run(&root_args(
        temp.path(),
        &[
            "config",
            "reset",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--changeset",
            reset_changeset,
        ],
    ));
    assert_eq!(json_stdout(&reset_replay)["outcome"], "no_op");
}

#[test]
fn a_failed_apply_is_history_not_an_executed_key_so_retry_reexecutes() {
    let temp = TempRoot::new();
    seed_skill(temp.path(), "alpha-skill", "active");

    let plan = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    let plan_document = json_stdout(&plan);
    let plan_path = temp.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan_document).unwrap(),
    )
    .unwrap();

    // Break the native mutation target: the skill directory disappears, so
    // the native retire verb fails.
    let skill_dir = temp.path().join("Control/user/skills/alpha-skill");
    let parked = temp.path().join("parked-alpha-skill");
    fs::rename(&skill_dir, &parked).unwrap();

    let changeset = "cs-failed-then-retried";
    let failed = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
            "--changeset",
            changeset,
        ],
    ));
    assert!(
        !failed.status.success(),
        "a failed apply must exit non-zero"
    );
    let failed_receipt = json_stdout(&failed);
    assert_satisfies(&failed_receipt, "oi.config-receipt-v1.schema.json");
    assert_eq!(failed_receipt["outcome"], "failed");
    assert!(failed_receipt["error"]["message"]
        .as_str()
        .unwrap()
        .contains("alpha-skill"));

    // Repair and retry the same changeset: the owner must re-execute, not
    // answer no_op — a failed key was never executed.
    fs::rename(&parked, &skill_dir).unwrap();
    let retry = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
            "--changeset",
            changeset,
        ],
    ));
    assert!(retry.status.success());
    let retry_receipt = json_stdout(&retry);
    assert_eq!(retry_receipt["outcome"], "applied");
    assert_eq!(retry_receipt["changeset_id"], changeset);
    assert_eq!(
        skill_manifest(temp.path(), "alpha-skill")["standing"],
        "retired"
    );
}

// ---------------------------------------------------------------------------
// The Central law: authored human ground is refused, structurally.
// ---------------------------------------------------------------------------

#[test]
fn non_writable_settings_are_never_planned_applied_or_reset() {
    let temp = TempRoot::new();

    // validate answers honestly (read-only plane): valid:false with a
    // not_writable violation, because the value can never be applied.
    let validate = run(&root_args(
        temp.path(),
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            POLICY_SETTING,
            "--scope",
            "ground",
            "--value",
            "\"Europe/London\"",
        ],
    ));
    assert!(validate.status.success());
    let validation = json_stdout(&validate);
    assert_eq!(validation["schema"], VALIDATION);
    assert_satisfies(&validation, "oi.config-validation-v1.schema.json");
    assert_eq!(validation["valid"], json!(false));
    assert_eq!(validation["violations"][0]["code"], "not_writable");

    for verb in ["plan", "reset"] {
        let mut args = root_args(
            temp.path(),
            &[
                "config",
                verb,
                "--json",
                "--setting",
                POLICY_SETTING,
                "--scope",
                "ground",
            ],
        );
        if verb == "plan" {
            // plan needs a syntactically present value flag; any value works
            // because the refusal precedes value validation.
            args.push("--value".to_owned());
            args.push(retire_value("alpha-skill", "retired"));
        }
        let output = run(&args);
        assert!(
            !output.status.success(),
            "{verb} on authored ground must fail"
        );
        let error = json_stdout(&output);
        assert_satisfies(&error, "oi.config-error-v1.schema.json");
        assert_eq!(error["error_code"], "not_authorised", "{verb}");
        assert_eq!(error["setting_ref"], POLICY_SETTING);
    }

    // apply refuses a forged plan naming authored ground outright.
    let forged = json!({
        "schema": PLAN,
        "plan_id": "central-plan-forged",
        "plan_digest": "0".repeat(64),
        "setting_ref": POLICY_SETTING,
        "scope": { "scope_kind": "ground", "scope_ref": null },
        "changes": [{ "summary": "overwrite authored policy" }],
        "expected_effect": { "kind": "none", "summary": null, "ref": null },
        "expires_at_unix_ms": 0
    });
    let forged_path = temp.path().join("forged-plan.json");
    fs::write(&forged_path, serde_json::to_vec_pretty(&forged).unwrap()).unwrap();
    let apply = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            forged_path.to_str().unwrap(),
            "--changeset",
            "cs-forged-1",
        ],
    ));
    assert!(!apply.status.success());
    assert_eq!(json_stdout(&apply)["error_code"], "not_authorised");

    // The placement policy is equally refused.
    let placement_plan = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            POLICY_SETTING_PLACEMENT,
            "--scope",
            "ground",
            "--value",
            "\"advisory\"",
        ],
    ));
    assert!(!placement_plan.status.success());
    assert_eq!(json_stdout(&placement_plan)["error_code"], "not_authorised");
}

// ---------------------------------------------------------------------------
// Addressing failures are explicit, structured, non-zero.
// ---------------------------------------------------------------------------

#[test]
fn addressing_failures_are_explicit_structured_errors() {
    let temp = TempRoot::new();
    let value = retire_value("alpha-skill", "retired");

    let cases: Vec<(Vec<&str>, &str, i32)> = vec![
        // Unknown setting.
        (
            vec![
                "config",
                "validate",
                "--json",
                "--setting",
                "central:skills:not.a.setting",
                "--scope",
                "ground",
                "--value",
                &value,
            ],
            "unsupported_setting",
            2,
        ),
        // Valid setting, scope kind outside its allowed_scopes.
        (
            vec![
                "config",
                "plan",
                "--json",
                "--setting",
                WRITABLE_SETTING,
                "--scope",
                "machine",
                "--value",
                &value,
            ],
            "unsupported_scope",
            2,
        ),
        // Scope kind outside the frozen registry.
        (
            vec![
                "config",
                "reset",
                "--json",
                "--setting",
                WRITABLE_SETTING,
                "--scope",
                "cluster",
            ],
            "unknown_scope_kind",
            2,
        ),
        // Missing explicit scope.
        (
            vec![
                "config",
                "validate",
                "--json",
                "--setting",
                WRITABLE_SETTING,
                "--value",
                &value,
            ],
            "validation_failed",
            2,
        ),
    ];
    for (tail, expected_code, expected_exit) in cases {
        let args = root_args(temp.path(), &tail);
        let output = run(&args);
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "exit code for {tail:?}: stdout={}",
            String::from_utf8_lossy(&output.stdout)
        );
        let error = json_stdout(&output);
        assert_eq!(error["schema"], ERROR, "{tail:?}");
        assert_satisfies(&error, "oi.config-error-v1.schema.json");
        assert_eq!(error["error_code"], expected_code, "{tail:?}");
    }

    // A plan body that is not the owner-minted document is refused.
    seed_skill(temp.path(), "alpha-skill", "active");
    let plan = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    let mut tampered = json_stdout(&plan);
    tampered["changes"][0]["summary"] = json!("tampered after minting");
    let tampered_path = temp.path().join("tampered-plan.json");
    fs::write(
        &tampered_path,
        serde_json::to_vec_pretty(&tampered).unwrap(),
    )
    .unwrap();
    let tampered_apply = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            tampered_path.to_str().unwrap(),
        ],
    ));
    assert!(!tampered_apply.status.success());
    assert_eq!(
        json_stdout(&tampered_apply)["error_code"],
        "validation_failed"
    );

    // An expired plan is refused as plan_expired (retryable).
    let fresh = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    let mut expired = json_stdout(&fresh);
    // The canonical digest zeroes expires_at_unix_ms, so ageing a plan does
    // not invalidate its digest — the expiry check must catch it.
    expired["expires_at_unix_ms"] = json!(1);
    let expired_path = temp.path().join("expired-plan.json");
    fs::write(&expired_path, serde_json::to_vec_pretty(&expired).unwrap()).unwrap();
    let expired_apply = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            expired_path.to_str().unwrap(),
        ],
    ));
    assert!(!expired_apply.status.success());
    assert_eq!(json_stdout(&expired_apply)["error_code"], "plan_expired");

    // Unknown config verbs and missing verbs are config errors too.
    for tail in [
        vec!["config", "frobnicate", "--json"],
        vec!["config", "--json"],
    ] {
        let output = run(&root_args(temp.path(), &tail));
        assert!(!output.status.success(), "{tail:?}");
        let error = json_stdout(&output);
        assert_eq!(error["schema"], ERROR, "{tail:?}");
        assert_eq!(error["error_code"], "validation_failed", "{tail:?}");
    }

    // Non-JSON failure output names the code and message, and does not panic
    // (config failures carry the contract document, not an ActionError).
    let human = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--setting",
            POLICY_SETTING,
            "--scope",
            "ground",
            "--value",
            "\"UTC\"",
        ],
    ));
    assert!(!human.status.success());
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.starts_with("not_authorised: "),
        "human-readable config failure should lead with the code, got: {text}"
    );
}

#[test]
fn deterministic_changeset_makes_a_bare_apply_replay_safe() {
    let temp = TempRoot::new();
    seed_skill(temp.path(), "alpha-skill", "active");

    let plan = run(&root_args(
        temp.path(),
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            WRITABLE_SETTING,
            "--scope",
            "ground",
            "--value",
            &retire_value("alpha-skill", "retired"),
        ],
    ));
    let plan_document = json_stdout(&plan);
    let plan_path = temp.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan_document).unwrap(),
    )
    .unwrap();

    // No --changeset: the owner mints a deterministic changeset from the
    // plan digest, so re-submitting the same plan cannot re-execute.
    let first = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
        ],
    ));
    assert!(first.status.success());
    let first_receipt = json_stdout(&first);
    assert_eq!(first_receipt["outcome"], "applied");

    let second = run(&root_args(
        temp.path(),
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_path.to_str().unwrap(),
        ],
    ));
    assert!(second.status.success());
    let second_receipt = json_stdout(&second);
    assert_eq!(second_receipt["outcome"], "no_op");
    assert_eq!(
        second_receipt["original_receipt_id"],
        first_receipt["receipt_id"]
    );
}
