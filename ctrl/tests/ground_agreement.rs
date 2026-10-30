//! A grant Setup founded, brought into agreement with what Setup grants now (O-I #740; manual §10.11: "Run it over an older setup and
//! it brings the setup into agreement"). An older Setup granted the person fewer root Actions than Central authenticates today; on
//! Setup's held Ground Accept the grant gains exactly the root Actions Setup's Ground Accept grants now that it lacks, and nothing else
//! of it moves. A grant Setup did not found (the owner's own, hand-authored) is refused by name and nothing is written. Disposable
//! grounds; the test token is no live credential.
use central_ctrl::continuous_work::authority::{self, AUTHENTICATED_ACTIONS};
use central_ctrl::continuous_work::source::Scope;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const TOKEN: &str = "ground-agreement-test-token-not-a-live-credential-001";
const PRINCIPAL: &str = "human:scratch-world";
const FOUNDING: &str = "oi-setup-adoption:review-token-under-test";
const AGREEING: &str = "dlw-setup-ground-accept:agreement-under-test";
const AUTHORITY: &str = "Control/user/native-action-authority.json";
const RELATIONS: &str = "Control/relations/source-relations.json";
/// The root Actions an older Setup did not grant yet (each authenticated at the root, never at a project only).
const LATER: [&str; 2] = ["central.world.place", "control.apply-proposal"];
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
    fn read(&self, path: &str) -> Vec<u8> {
        fs::read(self.ground().join(path)).unwrap()
    }
}
fn made() -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-ground-agreement-{}-{}-{}",
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
fn accept(acceptance: &str, more: Value) -> Value {
    let mut input =
        json!({"actor_kind":"human","acceptance":"human-accepted","acceptance_ref":acceptance});
    for (key, value) in more.as_object().unwrap() {
        input[key] = value.clone();
    }
    input
}
fn adopted(home: &Home, preview: &str, adopt: &str, input: Value) {
    let shown = cli(home, preview, input.clone());
    assert_eq!(shown["ok"], true, "{shown}");
    let mut input = accept(FOUNDING, input);
    input["expected_digest"] = shown["data"]["digest"].clone();
    let kept = cli(home, adopt, input);
    assert_eq!(kept["ok"], true, "{kept}");
}
/// A ground as Setup's Ground Accept leaves it: its days, its bounds and the person's key.
fn founded() -> Home {
    let home = made();
    adopted(
        &home,
        "central.time.policy.preview",
        "central.time.policy.adopt",
        json!({}),
    );
    adopted(
        &home,
        "central.ground.placement.preview",
        "central.ground.placement.adopt",
        json!({}),
    );
    adopted(
        &home,
        "central.ground.authority.preview",
        "central.ground.authority.adopt",
        json!({"principal_ref":PRINCIPAL,"token_sha256":sha(TOKEN)}),
    );
    home
}
/// The ground as an older Setup founded it: the same grant, without the root Actions Central authenticated only later.
fn older() -> Home {
    let home = founded();
    let mut authority: Value = serde_json::from_slice(&home.read(AUTHORITY)).unwrap();
    authority["grants"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .retain(|action| !LATER.iter().any(|later| action == later));
    fs::write(
        home.ground().join(AUTHORITY),
        serde_json::to_vec_pretty(&authority).unwrap(),
    )
    .unwrap();
    home
}
fn at_root(home: &Home, action: &str) -> Result<(), String> {
    let root = Scope::resolve(&home.ground(), None).unwrap();
    authority::authenticate(&root, Some(TOKEN), action, None, 1_800_000_000)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
fn recognition(home: &Home) -> Value {
    let relations: Value = serde_json::from_slice(&home.read(RELATIONS)).unwrap();
    relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == AUTHORITY)
        .unwrap()["recognition"]
        .clone()
}

#[test]
fn setup_s_current_grant_is_in_agreement_and_the_list_is_the_one_ground_accept_grants() {
    // The agreement uses the list Setup's Ground Accept grants, never a copy of it.
    assert_eq!(
        central_ctrl::ground_founding::setup_root_actions(),
        AUTHENTICATED_ACTIONS
    );
    let home = founded();
    let preview = cli(&home, "central.ground.agreement.preview", json!({}));
    assert_eq!(preview["ok"], true, "{preview}");
    let data = &preview["data"];
    assert_eq!(data["schema"], "central.ground-agreement-preview/v1");
    assert_eq!(data["state"], "present", "{preview}");
    assert_eq!(data["adds"], json!([]));
    assert!(data["digest"].is_null());
    assert_eq!(data["actions"], json!(AUTHENTICATED_ACTIONS));
    // No ground yet; a ground with no authority yet (Setup's Ground Accept founds one first).
    let bare = made();
    let unfounded = cli(&bare, "central.ground.agreement.preview", json!({}));
    assert_eq!(unfounded["data"]["state"], "unfounded", "{unfounded}");
    assert!(unfounded["data"]["digest"].is_null());
    fs::remove_dir_all(bare.ground()).unwrap();
    let none = cli(&bare, "central.ground.agreement.preview", json!({}));
    assert_eq!(none["data"]["state"], "no-ground", "{none}");
}

#[test]
fn an_older_setup_grant_gains_the_root_actions_setup_grants_now_and_nothing_else_of_it_moves() {
    let home = older();
    for action in LATER {
        assert!(
            at_root(&home, action)
                .unwrap_err()
                .contains("lacks this exact scope/action"),
            "{action} before"
        );
    }
    let before: Value = serde_json::from_slice(&home.read(AUTHORITY)).unwrap();
    let preview = cli(&home, "central.ground.agreement.preview", json!({}));
    assert_eq!(preview["ok"], true, "{preview}");
    let data = &preview["data"];
    assert_eq!(data["state"], "absent", "{preview}");
    assert_eq!(data["adds"], json!(LATER), "in the order Setup grants them");
    assert_eq!(data["principal_ref"], PRINCIPAL);
    assert_eq!(data["source_path"], AUTHORITY);
    assert!(
        !preview.to_string().contains(&sha(TOKEN)),
        "no credential digest"
    );
    let digest = data["digest"].as_str().unwrap().to_owned();
    // Previewing writes nothing.
    assert_eq!(
        serde_json::from_slice::<Value>(&home.read(AUTHORITY)).unwrap(),
        before
    );

    let kept = cli(
        &home,
        "central.ground.agreement.adopt",
        accept(AGREEING, json!({"expected_digest":digest})),
    );
    assert_eq!(kept["ok"], true, "{kept}");
    assert_eq!(kept["data"]["adopted"], true);
    assert_eq!(kept["data"]["added"], json!(LATER));
    assert!(
        !kept.to_string().contains(&sha(TOKEN)),
        "no credential digest"
    );

    // The grant holds every Action Setup grants now: what it held stays in place, the rest follows; nothing else of it moved.
    let after: Value = serde_json::from_slice(&home.read(AUTHORITY)).unwrap();
    let (was, now) = (&before["grants"][0], &after["grants"][0]);
    let held = was["actions"].as_array().unwrap();
    let mut expected = held.clone();
    expected.extend(LATER.iter().map(|a| json!(a)));
    assert_eq!(now["actions"], json!(expected));
    for field in [
        "principal_ref",
        "actor_kind",
        "token_sha256",
        "scope_refs",
        "expires_at_unix_seconds",
    ] {
        assert_eq!(now[field], was[field], "{field}");
    }
    assert_eq!(after["grants"].as_array().unwrap().len(), 1);
    for action in AUTHENTICATED_ACTIONS {
        assert!(at_root(&home, action).is_ok(), "{action} after");
    }
    // The source keeps every Accept that shaped it.
    assert_eq!(
        recognition(&home),
        format!("setup-ground-acceptance:{FOUNDING};setup-ground-acceptance:{AGREEING}")
    );
    // In agreement now: the preview says so, and a second adopt writes nothing.
    let again = cli(&home, "central.ground.agreement.preview", json!({}));
    assert_eq!(again["data"]["state"], "present", "{again}");
    let written = home.read(AUTHORITY);
    let twice = cli(
        &home,
        "central.ground.agreement.adopt",
        accept(AGREEING, json!({"expected_digest":digest})),
    );
    assert_eq!(twice["ok"], false, "{twice}");
    assert_eq!(home.read(AUTHORITY), written);
}

#[test]
fn a_grant_setup_did_not_found_is_refused_by_name_and_nothing_is_written() {
    // The owner's shape: a hand-authored grant at the root and several projects, recognised by the owner's own commission, lacking a
    // root Action Central authenticates.
    let home = made();
    let scope = Scope::resolve(&home.ground(), None).unwrap();
    let actions: Vec<&str> = AUTHENTICATED_ACTIONS
        .iter()
        .copied()
        .filter(|a| !LATER.contains(a))
        .collect();
    fs::write(home.ground().join(AUTHORITY), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
        {"principal_ref":PRINCIPAL,"actor_kind":"human","token_sha256":sha(TOKEN),"scope_refs":["control:root","project:Factory"],"actions":actions,
         "expires_at_unix_seconds":4_000_000_000u64}]})).unwrap()).unwrap();
    fs::create_dir_all(home.ground().join("Control/relations")).unwrap();
    let mut relations: Value = fs::read(home.ground().join(RELATIONS))
        .ok()
        .map(|raw| serde_json::from_slice(&raw).unwrap())
        .unwrap_or_else(|| json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[]}));
    relations["relations"].as_array_mut().unwrap().push(json!({"ref":scope.source_ref(AUTHORITY),"path":AUTHORITY,"roles":["native-action-authority"],
        "provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user",
        "recognition":"owner-commissioned-native-day-enablement","recorded_at_unix_seconds":1}));
    fs::write(
        home.ground().join(RELATIONS),
        serde_json::to_vec(&relations).unwrap(),
    )
    .unwrap();
    let kept = || [home.read(AUTHORITY), home.read(RELATIONS)];
    let before = kept();
    let preview = cli(&home, "central.ground.agreement.preview", json!({}));
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["data"]["state"], "hand-authored", "{preview}");
    assert!(preview["data"]["digest"].is_null());
    let reason = preview["data"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("owner-commissioned-native-day-enablement")
            && reason.contains("changed only by its owner"),
        "{reason}"
    );
    let refused = cli(
        &home,
        "central.ground.agreement.adopt",
        accept(AGREEING, json!({"expected_digest":"0".repeat(64)})),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("changed only by its owner"));
    assert_eq!(kept(), before, "both files stay byte-identical");

    // A Setup grant amended by hand (its recognition no longer wholly Setup's), and a Setup-recognised authority holding a second
    // grant: each is named, and nothing is written.
    let amended = older();
    let mut relations: Value = serde_json::from_slice(&amended.read(RELATIONS)).unwrap();
    for relation in relations["relations"].as_array_mut().unwrap() {
        if relation["path"] == AUTHORITY {
            relation["recognition"] = json!(format!(
                "setup-ground-acceptance:{FOUNDING};owner-amended-by-hand"
            ));
        }
    }
    fs::write(
        amended.ground().join(RELATIONS),
        serde_json::to_vec(&relations).unwrap(),
    )
    .unwrap();
    let second = older();
    let mut authority: Value = serde_json::from_slice(&second.read(AUTHORITY)).unwrap();
    let grant = authority["grants"][0].clone();
    authority["grants"].as_array_mut().unwrap().push(grant);
    fs::write(
        second.ground().join(AUTHORITY),
        serde_json::to_vec(&authority).unwrap(),
    )
    .unwrap();
    for (home, named) in [
        (&amended, "owner-amended-by-hand"),
        (&second, "one human grant"),
    ] {
        let before = [home.read(AUTHORITY), home.read(RELATIONS)];
        let preview = cli(home, "central.ground.agreement.preview", json!({}));
        assert_eq!(preview["data"]["state"], "hand-authored", "{preview}");
        assert!(
            preview["data"]["reason"].as_str().unwrap().contains(named),
            "{preview}"
        );
        let refused = cli(
            home,
            "central.ground.agreement.adopt",
            accept(AGREEING, json!({"expected_digest":"0".repeat(64)})),
        );
        assert_eq!(refused["ok"], false, "{refused}");
        assert_eq!([home.read(AUTHORITY), home.read(RELATIONS)], before);
    }
}

#[test]
fn the_agreement_needs_the_person_s_acceptance_and_the_previewed_digest() {
    let home = older();
    let preview = cli(&home, "central.ground.agreement.preview", json!({}));
    let digest = preview["data"]["digest"].as_str().unwrap().to_owned();
    let good = json!({"expected_digest":digest});
    let before = [home.read(AUTHORITY), home.read(RELATIONS)];
    let mut cases = vec![
        json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":AGREEING,"expected_digest":digest}),
        json!({"actor_kind":"human","acceptance_ref":AGREEING,"expected_digest":digest}),
        json!({"actor_kind":"human","acceptance":"human-accepted","expected_digest":digest}),
        accept(AGREEING, json!({})),
        accept(AGREEING, json!({"expected_digest":"0".repeat(64)})),
    ];
    let mut session = accept(AGREEING, good.clone());
    session["agent_session_ref"] = json!("session:1");
    cases.push(session);
    for input in cases {
        let refused = cli(&home, "central.ground.agreement.adopt", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
    assert_eq!(
        [home.read(AUTHORITY), home.read(RELATIONS)],
        before,
        "nothing was written"
    );
    // The grant moving after its preview refuses the adopt: the person accepts only what they saw.
    let mut moved: Value = serde_json::from_slice(&home.read(AUTHORITY)).unwrap();
    moved["grants"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a != "central.world.name");
    fs::write(
        home.ground().join(AUTHORITY),
        serde_json::to_vec_pretty(&moved).unwrap(),
    )
    .unwrap();
    let stale = home.read(AUTHORITY);
    let refused = cli(
        &home,
        "central.ground.agreement.adopt",
        accept(AGREEING, good.clone()),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("preview it again"));
    assert_eq!(home.read(AUTHORITY), stale);
    // A ground with no authority yet: nothing to bring into agreement.
    let bare = made();
    let refused = cli(
        &bare,
        "central.ground.agreement.adopt",
        accept(AGREEING, good),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(!bare.ground().join(AUTHORITY).exists());
}
