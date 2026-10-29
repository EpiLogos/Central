//! A new ground's Day (O-I #740; manual §10.11 Ground): Central's Day stands on one recognised civil-time policy, and a ground
//! Setup makes keeps none. `central.time.policy.preview` says what the person's Accept would adopt — this computer's IANA time
//! zone, the Day turning at midnight — and `central.time.policy.adopt` writes it on that preview's digest, recognised by the
//! acceptance it names. A policy that stands is never replaced. Disposable grounds; the zone comes from the child's own `TZ`.
use central_ctrl::continuous_work::{execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const ZONE: &str = "Pacific/Auckland";
const ACCEPTANCE: &str = "oi-setup-adoption:review-token-under-test";
const HUMAN: &str = "civil-time-adoption-test-token-not-a-live-credential";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Home(PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Home {
    fn ground(&self) -> PathBuf {
        self.0.join("Central")
    }
}
/// A person's home whose ground is `Central`, not yet made.
fn home() -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-civil-time-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    Home(path.canonicalize().unwrap())
}
/// A home with its ground made as Setup makes it (`ctrl init`): no policy of any kind.
fn made() -> Home {
    let home = home();
    central_ctrl::initialize_central(&home.ground()).unwrap();
    home
}
fn cli(home: &Home, action: &str, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root"])
        .arg(home.ground())
        .args(["action", "run", action, &input.to_string()])
        .env("HOME", &home.0)
        .env("TZ", ZONE)
        .env_remove("CENTRAL_ROOT")
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .current_dir(&home.0)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{action}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
fn adopt(home: &Home, digest: &str) -> Value {
    cli(
        home,
        "central.time.policy.adopt",
        json!({"actor_kind":"human","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"expected_digest":digest}),
    )
}
fn relations(root: &Path) -> Vec<Value> {
    let raw = fs::read(root.join("Control/relations/source-relations.json")).unwrap();
    serde_json::from_slice::<Value>(&raw).unwrap()["relations"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_made_ground_has_no_day_until_the_person_adopts_the_previewed_policy() {
    let home = made();
    let refused = cli(&home, "central.time.policy", json!({}));
    assert_eq!(refused["ok"], false, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("requires one explicitly recognised civil-time-policy source")
            && message.contains("central.time.policy.adopt"),
        "the refusal names the way on: {message}"
    );

    let preview = cli(&home, "central.time.policy.preview", json!({}));
    assert_eq!(preview["ok"], true, "{preview}");
    let data = &preview["data"];
    assert_eq!(data["schema"], "central.civil-time-adoption-preview/v1");
    assert_eq!(data["state"], "absent");
    assert_eq!(data["source_path"], "Control/user/civil-time-policy.json");
    assert_eq!(data["timezone_source"], "TZ");
    assert_eq!(
        data["policy"],
        json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":ZONE,"day_boundary_minutes":0,"automatic_day_rollover":true})
    );
    let digest = data["digest"].as_str().unwrap();
    assert_eq!(digest.len(), 64);
    // A preview writes nothing.
    assert!(!home
        .ground()
        .join("Control/user/civil-time-policy.json")
        .exists());

    let adopted = adopt(&home, digest);
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["data"]["adopted"], true);
    assert_eq!(adopted["data"]["reading"]["policy"]["timezone"], ZONE);
    assert_eq!(
        adopted["data"]["source_ref"],
        "central:source:control:root:Control/user/civil-time-policy.json"
    );

    let time = cli(&home, "central.time.policy", json!({}));
    assert_eq!(time["ok"], true, "{time}");
    assert_eq!(time["data"]["policy"]["timezone"], ZONE);
    assert_eq!(time["data"]["policy"]["day_boundary_minutes"], 0);

    let relation = relations(&home.ground())
        .into_iter()
        .find(|r| r["roles"] == json!(["civil-time-policy"]))
        .expect("the policy's relation");
    assert_eq!(relation["provenance"], "human-adopted");
    assert_eq!(relation["standing"], "architecture-contract");
    assert_eq!(relation["path"], "Control/user/civil-time-policy.json");
    assert_eq!(
        relation["recognition"],
        format!("setup-ground-acceptance:{ACCEPTANCE}")
    );
    // The preview now reads the policy that stands, and offers nothing to write.
    let after = cli(&home, "central.time.policy.preview", json!({}));
    assert_eq!(after["data"]["state"], "recognised", "{after}");
    assert!(after["data"]["digest"].is_null());
}

#[test]
fn adopting_again_makes_nothing_twice_and_a_standing_policy_is_never_replaced() {
    let home = made();
    let digest = cli(&home, "central.time.policy.preview", json!({}))["data"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(adopt(&home, &digest)["data"]["adopted"], true);
    let path = home.ground().join("Control/user/civil-time-policy.json");
    let bytes = fs::read(&path).unwrap();
    let count = relations(&home.ground()).len();
    let again = adopt(&home, &digest);
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["data"]["adopted"], false);
    assert_eq!(again["data"]["state"], "recognised");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(relations(&home.ground()).len(), count);

    // A ground whose owner keeps another zone: read as it stands, kept, never rewritten to this computer's zone.
    let owned = made();
    let scope = Scope::resolve(&owned.ground(), None).unwrap();
    let source = "Control/user/civil-time-policy.json";
    let theirs = json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":240,"automatic_day_rollover":true});
    fs::write(
        owned.ground().join(source),
        serde_json::to_vec(&theirs).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(owned.ground().join("Control/relations")).unwrap();
    fs::write(owned.ground().join("Control/relations/source-relations.json"), serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[
        {"ref":scope.source_ref(source),"path":source,"roles":["civil-time-policy"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"owner-commissioned-under-test","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
    let preview = cli(&owned, "central.time.policy.preview", json!({}));
    assert_eq!(preview["data"]["state"], "recognised", "{preview}");
    assert_eq!(preview["data"]["policy"], theirs);
    let kept = adopt(&owned, &digest);
    assert_eq!(kept["data"]["adopted"], false, "{kept}");
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(owned.ground().join(source)).unwrap()).unwrap(),
        theirs
    );
}

#[test]
fn adoption_needs_the_persons_acceptance_and_the_previewed_digest() {
    let home = made();
    let digest = cli(&home, "central.time.policy.preview", json!({}))["data"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    for input in [
        json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"expected_digest":digest}),
        json!({"actor_kind":"human","acceptance_ref":ACCEPTANCE,"expected_digest":digest}),
        json!({"actor_kind":"human","acceptance":"human-accepted","expected_digest":digest}),
        json!({"actor_kind":"human","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"agent_session_ref":"session:1","expected_digest":digest}),
        json!({"actor_kind":"human","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"expected_digest":"0".repeat(64)}),
    ] {
        let refused = cli(&home, "central.time.policy.adopt", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
    assert!(!home
        .ground()
        .join("Control/user/civil-time-policy.json")
        .exists());
    assert_eq!(cli(&home, "central.time.policy", json!({}))["ok"], false);
}

#[test]
fn a_policy_file_no_relation_names_is_named_and_never_overwritten() {
    let home = made();
    let path = home.ground().join("Control/user/civil-time-policy.json");
    fs::write(&path, b"{\"a draft\":true}\n").unwrap();
    let preview = cli(&home, "central.time.policy.preview", json!({}));
    assert_eq!(preview["data"]["state"], "unrecognised", "{preview}");
    assert!(preview["data"]["reason"]
        .as_str()
        .unwrap()
        .contains("Control/user/civil-time-policy.json"));
    assert!(preview["data"]["digest"].is_null());
    let refused = adopt(&home, &"0".repeat(64));
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(fs::read(&path).unwrap(), b"{\"a draft\":true}\n");
}

#[test]
fn the_preview_before_the_ground_is_made_is_the_one_its_accept_presses() {
    let home = home();
    let before = cli(&home, "central.time.policy.preview", json!({}));
    assert_eq!(before["ok"], true, "{before}");
    assert_eq!(before["data"]["state"], "no-ground");
    assert_eq!(before["data"]["policy"]["timezone"], ZONE);
    let digest = before["data"]["digest"].as_str().unwrap().to_owned();
    assert!(!home.ground().exists(), "a preview makes no ground");
    central_ctrl::initialize_central(&home.ground()).unwrap();
    assert_eq!(
        cli(&home, "central.time.policy.preview", json!({}))["data"]["digest"],
        json!(digest)
    );
    assert_eq!(adopt(&home, &digest)["data"]["adopted"], true);
}

#[test]
fn once_adopted_the_day_waits_only_on_the_persons_native_authority() {
    let home = made();
    let digest = cli(&home, "central.time.policy.preview", json!({}))["data"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(adopt(&home, &digest)["data"]["adopted"], true);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let time = execute_with_token_at(&home.ground(), "time_policy", &json!({}), None, now).unwrap();
    // Without the person's native authority the Day is refused for that, and no longer for civil time.
    let refused = execute_with_token_at(
        &home.ground(),
        "day_ensure",
        &json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        now,
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("native-action-authority"), "{refused}");
    // With a recognised authority granting the person the Day, today's Day is made under the adopted policy.
    let root = home.ground();
    let scope = Scope::resolve(&root, None).unwrap();
    let source = "Control/user/native-action-authority.json";
    fs::write(root.join(source), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
        {"principal_ref":"human:test","actor_kind":"human","token_sha256":format!("{:x}",Sha256::digest(HUMAN.as_bytes())),"scope_refs":["control:root"],"actions":["central.day.ensure"],"expires_at_unix_seconds":now + 3600}]})).unwrap()).unwrap();
    let mut doc: Value = serde_json::from_slice(
        &fs::read(root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    doc["relations"].as_array_mut().unwrap().push(json!({"ref":scope.source_ref(source),"path":source,"roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":"controlled-test-only","recorded_at_unix_seconds":1}));
    fs::write(
        root.join("Control/relations/source-relations.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    let day = execute_with_token_at(
        &root,
        "day_ensure",
        &json!({"expected_time_policy_revision":time["revision"]}),
        Some(HUMAN),
        now,
    )
    .unwrap();
    assert_eq!(day["temporal"]["civil_date"], time["civil_date"]);
    assert_eq!(day["created"], true);
}
