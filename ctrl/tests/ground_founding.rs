//! A new ground's founding sources, adopted on the person's Setup acceptance (O-I #740, round two): the person's native-action
//! authority — a human grant for the credential the host keeps, of exactly the actions Central authenticates — and the root
//! placement policy whose bounds are the ground itself. Each is previewed with a digest and adopted on it, at first establishment
//! only: a ground that keeps one is refused by name and nothing is written. Disposable grounds; the test token is no live credential.
use central_ctrl::continuous_work::authority::{self, AUTHENTICATED_ACTIONS};
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

const TOKEN: &str = "ground-founding-test-token-not-a-live-credential-0001";
const PRINCIPAL: &str = "human:scratch-world";
const ACCEPTANCE: &str = "oi-setup-adoption:review-token-under-test";
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
/// A home whose ground Setup made (`ctrl init`): no policy, no authority.
fn made() -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-ground-founding-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    let home = Home(path.canonicalize().unwrap());
    central_ctrl::initialize_central(&home.ground()).unwrap();
    home
}
fn cli(home: &Home, action: &str, input: Value) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
        .args(["--json", "--root"])
        .arg(home.ground())
        .args(["action", "run", action, &input.to_string()])
        .env("HOME", &home.0)
        .env("TZ", "Pacific/Auckland")
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
fn sha(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn accept(more: Value) -> Value {
    let mut input =
        json!({"actor_kind":"human","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE});
    for (key, value) in more.as_object().unwrap() {
        input[key] = value.clone();
    }
    input
}
fn digest(home: &Home, action: &str, input: Value) -> String {
    let preview = cli(home, action, input);
    assert_eq!(preview["ok"], true, "{preview}");
    preview["data"]["digest"].as_str().unwrap().to_owned()
}
fn relations(root: &Path) -> Vec<Value> {
    match fs::read(root.join("Control/relations/source-relations.json")) {
        Ok(raw) => serde_json::from_slice::<Value>(&raw).unwrap()["relations"]
            .as_array()
            .unwrap()
            .clone(),
        Err(_) => vec![],
    }
}
fn adopt_days(home: &Home) {
    let digest = digest(home, "central.time.policy.preview", json!({}));
    assert_eq!(
        cli(
            home,
            "central.time.policy.adopt",
            accept(json!({"expected_digest":digest}))
        )["data"]["adopted"],
        true
    );
}

#[test]
fn the_key_grants_exactly_the_actions_central_authenticates_and_each_is_a_registered_write() {
    let registry = central_ctrl::cli::create_runtime_action_registry();
    let listed = registry.list();
    let mut sorted = AUTHENTICATED_ACTIONS.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), AUTHENTICATED_ACTIONS.len(), "no action twice");
    for action in AUTHENTICATED_ACTIONS {
        let row = listed
            .iter()
            .find(|row| row.id == *action)
            .unwrap_or_else(|| panic!("{action} is no registered Action"));
        assert_ne!(
            format!("{:?}", row.mutation_class),
            "ReadOnly",
            "{action} authenticates, so it writes"
        );
    }
    let home = made();
    let preview = cli(
        &home,
        "central.ground.authority.preview",
        json!({"principal_ref":PRINCIPAL}),
    );
    assert_eq!(preview["ok"], true, "{preview}");
    let data = &preview["data"];
    assert_eq!(data["schema"], "central.ground-authority-preview/v1");
    assert_eq!(data["state"], "absent");
    assert_eq!(
        data["source_path"],
        "Control/user/native-action-authority.json"
    );
    assert_eq!(data["grant"]["principal_ref"], PRINCIPAL);
    assert_eq!(data["grant"]["actor_kind"], "human");
    assert_eq!(data["grant"]["scope_refs"], json!(["control:root"]));
    assert_eq!(data["grant"]["actions"], json!(AUTHENTICATED_ACTIONS));
    assert!(
        data["grant"].get("token_sha256").is_none(),
        "the preview names no credential"
    );
    assert_eq!(data["digest"].as_str().unwrap().len(), 64);
}

#[test]
fn an_action_outside_the_list_never_authenticates_even_when_a_grant_names_it() {
    let home = made();
    let root = home.ground();
    let scope = Scope::resolve(&root, None).unwrap();
    let source = "Control/user/native-action-authority.json";
    fs::write(root.join(source), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
        {"principal_ref":PRINCIPAL,"actor_kind":"human","token_sha256":sha(TOKEN),"scope_refs":["control:root"],"actions":["central.day.ensure","central.unlisted.write"],"expires_at_unix_seconds":4_000_000_000u64}]})).unwrap()).unwrap();
    fs::create_dir_all(root.join("Control/relations")).unwrap();
    fs::write(root.join("Control/relations/source-relations.json"), serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[
        {"ref":scope.source_ref(source),"path":source,"roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":"controlled-test-only","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
    let refused = authority::authenticate(
        &scope,
        Some(TOKEN),
        "central.unlisted.write",
        None,
        1_800_000_000,
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("AUTHENTICATED_ACTIONS"), "{refused}");
    assert!(authority::authenticate(
        &scope,
        Some(TOKEN),
        "central.day.ensure",
        None,
        1_800_000_000
    )
    .is_ok());
}

#[test]
fn a_new_ground_adopts_its_key_and_placement_and_today_s_day_is_made_with_the_person_s_credential()
{
    let home = made();
    adopt_days(&home);
    // Placement: the ground's own bounds.
    let preview = cli(&home, "central.ground.placement.preview", json!({}));
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["data"]["state"], "absent");
    assert_eq!(
        preview["data"]["source_path"],
        "Control/user/placement.json"
    );
    let policy = &preview["data"]["policy"];
    assert_eq!(policy["schema"], "central.work-placement-policy/v1");
    assert_eq!(
        policy["writable"],
        json!([]),
        "nothing outside the ground, and no Work member granted yet"
    );
    assert_eq!(policy["enforcement"], "native-actions");
    let placed = cli(
        &home,
        "central.ground.placement.adopt",
        accept(json!({"expected_digest":preview["data"]["digest"]})),
    );
    assert_eq!(placed["ok"], true, "{placed}");
    assert_eq!(placed["data"]["adopted"], true);
    let work = cli(&home, "central.work.policy", json!({}));
    assert_eq!(work["ok"], true, "the ground's placement now reads: {work}");

    // The key: only the credential's digest crosses.
    let key_digest = digest(
        &home,
        "central.ground.authority.preview",
        json!({"principal_ref":PRINCIPAL}),
    );
    let kept = cli(
        &home,
        "central.ground.authority.adopt",
        accept(
            json!({"principal_ref":PRINCIPAL,"token_sha256":sha(TOKEN),"expected_digest":key_digest}),
        ),
    );
    assert_eq!(kept["ok"], true, "{kept}");
    assert_eq!(kept["data"]["adopted"], true);
    assert!(
        !kept.to_string().contains(&sha(TOKEN)),
        "the receipt carries no credential digest"
    );
    for (path, role) in [
        (
            "Control/user/native-action-authority.json",
            "native-action-authority",
        ),
        ("Control/user/placement.json", "work-placement-policy"),
    ] {
        let relation = relations(&home.ground())
            .into_iter()
            .find(|r| r["roles"] == json!([role]))
            .unwrap_or_else(|| panic!("{role} relation"));
        assert_eq!(relation["path"], path);
        assert_eq!(relation["provenance"], "human-adopted");
        assert_eq!(relation["standing"], "architecture-contract");
        assert_eq!(
            relation["recognition"],
            format!("setup-ground-acceptance:{ACCEPTANCE}")
        );
    }

    // Today's Day, under the adopted policy, with the person's credential: the owner path the shell's Open today takes.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let time = execute_with_token_at(&home.ground(), "time_policy", &json!({}), None, now).unwrap();
    let day = execute_with_token_at(
        &home.ground(),
        "day_ensure",
        &json!({"expected_time_policy_revision":time["revision"]}),
        Some(TOKEN),
        now,
    )
    .unwrap();
    assert_eq!(day["created"], true);
    assert_eq!(day["temporal"]["civil_date"], time["civil_date"]);
    // Another credential is not the person.
    let other = execute_with_token_at(
        &home.ground(),
        "day_ensure",
        &json!({"expected_time_policy_revision":time["revision"]}),
        Some("another-credential-that-is-not-the-persons-0000000000"),
        now,
    )
    .unwrap_err()
    .to_string();
    assert!(other.contains("not authorised"), "{other}");
}

#[test]
fn a_ground_that_keeps_an_authority_or_a_placement_policy_is_refused_by_name_and_nothing_is_written(
) {
    // A source file standing without its relation.
    let home = made();
    let path = home
        .ground()
        .join("Control/user/native-action-authority.json");
    fs::write(&path, b"{\"owner\":\"keeps this\"}\n").unwrap();
    let preview = cli(
        &home,
        "central.ground.authority.preview",
        json!({"principal_ref":PRINCIPAL}),
    );
    assert_eq!(preview["data"]["state"], "present", "{preview}");
    assert!(preview["data"]["digest"].is_null());
    let refused = cli(
        &home,
        "central.ground.authority.adopt",
        accept(
            json!({"principal_ref":PRINCIPAL,"token_sha256":sha(TOKEN),"expected_digest":"0".repeat(64)}),
        ),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("native-action-authority"));
    assert_eq!(fs::read(&path).unwrap(), b"{\"owner\":\"keeps this\"}\n");
    assert!(relations(&home.ground()).is_empty(), "no relation written");

    // A relation of the role, wherever its file lies.
    let other = made();
    let scope = Scope::resolve(&other.ground(), None).unwrap();
    fs::create_dir_all(other.ground().join("Control/relations")).unwrap();
    fs::write(other.ground().join("Control/relations/source-relations.json"), serde_json::to_vec(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[
        {"ref":scope.source_ref("Control/user/elsewhere.json"),"path":"Control/user/elsewhere.json","roles":["work-placement-policy"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":"owner","recorded_at_unix_seconds":1}]})).unwrap()).unwrap();
    let before = fs::read(
        other
            .ground()
            .join("Control/relations/source-relations.json"),
    )
    .unwrap();
    assert_eq!(
        cli(&other, "central.ground.placement.preview", json!({}))["data"]["state"],
        "present"
    );
    let refused = cli(
        &other,
        "central.ground.placement.adopt",
        accept(json!({"expected_digest":"0".repeat(64)})),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(!other.ground().join("Control/user/placement.json").exists());
    assert_eq!(
        fs::read(
            other
                .ground()
                .join("Control/relations/source-relations.json")
        )
        .unwrap(),
        before
    );
}

#[test]
fn adoption_needs_the_person_s_acceptance_a_human_principal_a_credential_digest_and_the_previewed_digest(
) {
    let home = made();
    let key = digest(
        &home,
        "central.ground.authority.preview",
        json!({"principal_ref":PRINCIPAL}),
    );
    let good = json!({"principal_ref":PRINCIPAL,"token_sha256":sha(TOKEN),"expected_digest":key});
    let mut cases = vec![
        json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE}),
        json!({"actor_kind":"human","acceptance_ref":ACCEPTANCE}),
        json!({"actor_kind":"human","acceptance":"human-accepted"}),
    ]
    .into_iter()
    .map(|gate| {
        let mut input = good.clone();
        for (k, v) in gate.as_object().unwrap() {
            input[k] = v.clone();
        }
        input
    })
    .collect::<Vec<_>>();
    for (field, value) in [
        ("principal_ref", json!("agent:someone")),
        ("token_sha256", json!(TOKEN)),
        ("token_sha256", json!("ABCDEF".repeat(10) + "ABCD")),
        ("expected_digest", json!("0".repeat(64))),
    ] {
        let mut input = accept(good.clone());
        input[field] = value;
        cases.push(input);
    }
    let mut session = accept(good.clone());
    session["agent_session_ref"] = json!("session:1");
    cases.push(session);
    for input in cases {
        let refused = cli(&home, "central.ground.authority.adopt", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
    assert!(!home
        .ground()
        .join("Control/user/native-action-authority.json")
        .exists());
    let place = digest(&home, "central.ground.placement.preview", json!({}));
    assert_eq!(
        cli(
            &home,
            "central.ground.placement.adopt",
            json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"expected_digest":place})
        )["ok"],
        false
    );
    assert!(!home.ground().join("Control/user/placement.json").exists());
}

const FORM: &str = "<!doctype html><title>Day</title><script type=\"application/json\" id=\"ql-doc\">{\"meta\":{\"type\":\"daily\"},\"fields\":{}}</script>\n";

#[test]
fn a_new_ground_keeps_the_day_form_it_is_given_once_and_never_replaces_one_that_stands() {
    let home = made();
    let preview = cli(
        &home,
        "central.ground.form.preview",
        json!({"name":"ql-daily-die.html","content":FORM}),
    );
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["data"]["state"], "absent");
    assert_eq!(
        preview["data"]["source_path"],
        "Control/forms/ql-daily-die.html"
    );
    let kept = cli(
        &home,
        "central.ground.form.adopt",
        accept(
            json!({"name":"ql-daily-die.html","content":FORM,"expected_digest":preview["data"]["digest"]}),
        ),
    );
    assert_eq!(kept["ok"], true, "{kept}");
    assert_eq!(kept["data"]["adopted"], true);
    let path = home.ground().join("Control/forms/ql-daily-die.html");
    assert_eq!(fs::read_to_string(&path).unwrap(), FORM);
    // Central's own file route reads it where it stands: the Day's form is a location of the ground.
    let listed = cli(&home, "central.files.list", json!({"path":"Control/forms"}));
    assert_eq!(listed["ok"], true, "{listed}");
    assert!(listed["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "ql-daily-die.html" && e["retrieval_allowed"] == true));
    let relation = relations(&home.ground())
        .into_iter()
        .find(|r| r["roles"] == json!(["document-form"]))
        .expect("the form's relation");
    assert_eq!(relation["provenance"], "generated-derived");
    assert_eq!(
        relation["recognition"],
        format!("setup-ground-acceptance:{ACCEPTANCE}")
    );
    // Kept once: the same form is present; another form never replaces it.
    let again = cli(
        &home,
        "central.ground.form.preview",
        json!({"name":"ql-daily-die.html","content":"<html>another</html>"}),
    );
    assert_eq!(again["data"]["state"], "present", "{again}");
    assert!(again["data"]["digest"].is_null());
    let refused = cli(
        &home,
        "central.ground.form.adopt",
        accept(
            json!({"name":"ql-daily-die.html","content":"<html>another</html>","expected_digest":"0".repeat(64)}),
        ),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(fs::read_to_string(&path).unwrap(), FORM);
    // Only a form's own name, and the person's acceptance.
    for (input, why) in [
        (
            accept(
                json!({"name":"../user/x.html","content":FORM,"expected_digest":"0".repeat(64)}),
            ),
            "a path",
        ),
        (
            accept(json!({"name":"x.json","content":FORM,"expected_digest":"0".repeat(64)})),
            "not html",
        ),
        (
            json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":ACCEPTANCE,"name":"other.html","content":FORM,"expected_digest":"0".repeat(64)}),
            "an agent",
        ),
    ] {
        assert_eq!(
            cli(&home, "central.ground.form.adopt", input)["ok"],
            false,
            "{why}"
        );
    }
}
