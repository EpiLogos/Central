//! Disposable native Worlds: these test policies/credentials are not production
//! fixtures, installed source, or a claim about the owner's actual Daily Die.
use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const HUMAN: &str = "w2-root-day-test-token-not-a-live-credential";
struct World(PathBuf);
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn world() -> World {
    let path = std::env::temp_dir().join(format!(
        "central-root-day-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    central_ctrl::initialize_central(&path).unwrap();
    // An ordinary directory is not a participating child Project.
    fs::create_dir_all(path.join("Work/sources")).unwrap();
    let scope = Scope::resolve(&path, None).unwrap();
    let mut relations = vec![];
    for (name, role, value) in [
        (
            "placement.json",
            "work-placement-policy",
            json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":[{"path":"Work/sources","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}),
        ),
        (
            "time.json",
            "civil-time-policy",
            json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}),
        ),
        (
            "authority.json",
            "native-action-authority",
            json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{"principal_ref":"human:test","actor_kind":"human","token_sha256":format!("{:x}",Sha256::digest(HUMAN.as_bytes())),"scope_refs":["control:root"],"actions":["central.day.ensure","central.document.create","central.document.mutate"],"expires_at_unix_seconds":300000}]}),
        ),
    ] {
        let source = format!("Control/user/{name}");
        fs::write(path.join(&source), serde_json::to_vec(&value).unwrap()).unwrap();
        relations.push(json!({"ref":scope.source_ref(&source),"path":source,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":"controlled-test-only","recorded_at_unix_seconds":1}));
    }
    fs::create_dir_all(path.join("Control/relations")).unwrap();
    fs::write(path.join("Control/relations/source-relations.json"),serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
    World(path)
}
fn cli(root: &Path, action: &str, input: Value) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root"])
        .arg(root)
        .args(["action", "run", action, &input.to_string()])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&result.stdout).unwrap_or_else(|_| {
        panic!(
            "not a native envelope: {} / {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        )
    });
    assert_eq!(
        result.status.success(),
        value["ok"] == true,
        "exit/envelope disagree: {value}"
    );
    value
}
fn source(root: &Path, path: &str) -> Value {
    let horizon = cli(root, "projectcentral.change.horizon", json!({}));
    assert_eq!(horizon["ok"], true, "{horizon}");
    horizon["data"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["binding"]["path"] == path)
        .unwrap()
        .clone()
}
fn ensure(root: &Path, at: u64) -> Value {
    let time = execute_at(root, "time_policy", &json!({}), at).unwrap();
    execute_with_token_at(
        root,
        "day_ensure",
        &json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        at,
    )
    .unwrap()
}
fn document(root: &Path, day: &Value) -> Value {
    let policy = execute_at(root, "policy", &json!({}), 100).unwrap();
    execute_with_token_at(root,"document_create",&json!({"kind":"day","document_id":"test:bound-day","day_ref":day["day_ref"],"expected_revision":day["revision"]["revision"],"expected_policy_revision":policy["revision"],"template_payload":{"supplied":"unchanged","nested":{"retained":true}},"fields":[{"id":"test:supplied","label":"Test supplied field","template_pointer":"/supplied"}]}),Some(HUMAN),100).unwrap()
}

#[test]
fn root_without_child_project_read_edit_restart_and_history_use_one_source() {
    let w = world();
    assert!(!w.0.join("Work/sources/ProjectCentral").exists());
    fs::write(w.0.join("Control/user/intent.md"), "authored ground\n").unwrap();
    let observed = source(&w.0, "Control/user/intent.md");
    let reference = &observed["binding"]["ref"];
    let read = cli(
        &w.0,
        "projectcentral.source.read",
        json!({"source_ref":reference}),
    );
    assert_eq!(read["ok"], true, "{read}");
    assert_eq!(read["data"]["world_ref"], "control:root");
    assert_eq!(read["data"]["content"], "authored ground\n");
    let saved = cli(
        &w.0,
        "projectcentral.source.write",
        json!({"project":null,"source_ref":reference,"expected_revision":observed["revision"]["revision"],"content":"revised ground\n","actor":"human:test","actor_kind":"human"}),
    );
    assert_eq!(saved["ok"], true, "{saved}");
    // A fresh process is a restart, not an in-memory cache readback.
    let reopened = cli(
        &w.0,
        "projectcentral.source.read",
        json!({"source_ref":reference}),
    );
    assert_eq!(reopened["data"]["content"], "revised ground\n");
    assert_eq!(reopened["data"]["source"]["ref"], *reference);
    assert_eq!(
        reopened["data"]["revision"],
        saved["data"]["receipt"]["revision"]
    );
    let history = cli(
        &w.0,
        "projectcentral.change.horizon",
        json!({"project":null}),
    );
    assert!(history["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["source_ref"] == *reference && c["actor"] == "human:test"));
    assert!(!w.0.join("ProjectCentral").exists());
    assert!(!w.0.join("Work/sources/ProjectCentral").exists());
}
#[test]
fn root_revisions_and_authored_ground_cannot_be_bypassed() {
    let w = world();
    let path = w.0.join("Control/user/intent.md");
    fs::write(&path, "v1").unwrap();
    let observed = source(&w.0, "Control/user/intent.md");
    let mut input = json!({"source_ref":observed["binding"]["ref"],"expected_revision":observed["revision"]["revision"],"content":"overwrite","actor":"agent:test","actor_kind":"agent"});
    let denied = cli(&w.0, "projectcentral.source.write", input.clone());
    assert_eq!(denied["ok"], false);
    assert!(denied.to_string().contains("authored human ground"));
    fs::write(&path, "external edit").unwrap();
    input["actor_kind"] = json!("human");
    input["actor"] = json!("human:test");
    let stale = cli(&w.0, "projectcentral.source.write", input.clone());
    assert_eq!(stale["ok"], false);
    assert!(stale.to_string().contains("revision conflict"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "external edit");
    input["project"] = json!("");
    assert_eq!(
        cli(&w.0, "projectcentral.source.read", input.clone())["ok"],
        false
    );
    input["project"] = json!("sources");
    assert_eq!(cli(&w.0, "projectcentral.source.read", input)["ok"], false);
}
#[test]
fn root_denied_missing_and_temporal_owned_sources_remain_refusals() {
    let w = world();
    let private = w.0.join("Control/user/private");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(".no-agent-retrieval"), "").unwrap();
    fs::write(private.join("secret.md"), "must not disclose").unwrap();
    let observed = source(&w.0, "Control/user/private/secret.md");
    let denied = cli(
        &w.0,
        "projectcentral.source.read",
        json!({"source_ref":observed["binding"]["ref"]}),
    );
    assert_eq!(denied["ok"], false);
    assert!(!denied.to_string().contains("must not disclose"));
    let missing = cli(
        &w.0,
        "projectcentral.source.read",
        json!({"source_ref":"unknown:source"}),
    );
    assert_eq!(missing["ok"], false);
    let day = ensure(&w.0, 100);
    let refused = cli(
        &w.0,
        "projectcentral.source.write",
        json!({"source_ref":day["source"]["ref"],"expected_revision":day["revision"]["revision"],"content":"generic overwrite","actor":"human:test","actor_kind":"human"}),
    );
    assert_eq!(refused["ok"], false);
    assert!(refused
        .to_string()
        .contains("authenticated owner operation"));
}
#[test]
fn current_day_resolves_native_document_not_blank_carrier_or_template() {
    let w = world();
    let day = ensure(&w.0, 100);
    assert_eq!(day["document_state"], "uninitialised");
    assert!(day["document"].is_null());
    let doc = document(&w.0, &day);
    // An unrelated file called Daily Die must not influence native resolution.
    fs::write(
        w.0.join("Control/user/ql-daily-die.html"),
        "unrelated template",
    )
    .unwrap();
    let read = execute_at(&w.0, "day_read", &json!({}), 100).unwrap();
    assert_eq!(read["document_state"], "ready");
    assert_eq!(read["document"], doc);
    assert_eq!(read["document"]["source"]["ref"], day["source"]["ref"]);
    assert_eq!(
        read["document"]["document"]["template_payload"]["nested"]["retained"],
        true
    );
    assert_eq!(read["automatic_agent_or_model_invocation"], false);
}
#[test]
fn rollover_leaves_held_day_and_document_unchanged() {
    let w = world();
    let old = ensure(&w.0, 100);
    let doc = document(&w.0, &old);
    let next = ensure(&w.0, 90000);
    assert_ne!(next["day_ref"], old["day_ref"]);
    assert_eq!(next["document_state"], "uninitialised");
    let held = execute_at(&w.0, "day_read", &json!({"day_ref":old["day_ref"]}), 90000).unwrap();
    assert_eq!(held["document"], doc);
    assert_eq!(held["temporal"]["lifecycle"], "open");
    assert_eq!(
        execute_at(&w.0, "day_read", &json!({}), 90000).unwrap()["day_ref"],
        next["day_ref"]
    );
}
#[test]
fn damaged_or_denied_bound_document_is_not_downgraded_to_empty_success() {
    let w = world();
    let day = ensure(&w.0, 100);
    let doc = document(&w.0, &day);
    let path = w.0.join(doc["source"]["path"].as_str().unwrap());
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, "{broken").unwrap();
    assert!(execute_at(&w.0, "day_read", &json!({}), 100).is_err());
    fs::write(&path, &original).unwrap();
    fs::write(path.parent().unwrap().join(".no-agent-retrieval"), "").unwrap();
    assert_eq!(
        execute_at(&w.0, "day_read", &json!({}), 100)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}
