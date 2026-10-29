//! A world's declared place (`central.world.place`, O-I #745): where the Sky counts the world's hours when its Observer is Shared
//! (DLW manual §22.1, "the world's place when Observer is Shared"; QL-MEF #254 D12, "Nara's location, or a declared world place").
//! A project's world keeps it in its ProjectCentral manifest beside its name, and the world map discloses it; your own world keeps it
//! on its root world record (`control:root`, `central.world-relations/v1`), read through `central.world-relations.read`. Both are the
//! person's own acts, admitted only on their native authority. Disposable grounds; the test token is no live credential.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const TOKEN: &str = "world-place-test-token-not-a-live-credential-000000001";
const PRINCIPAL: &str = "human:scratch-world";
const PROJECT: &str = "field-notes";
const ACTION: &str = "central.world.place";
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

/// A ground with one project and the person's grant of `actions` at the root and the project.
fn made(actions: &[&str]) -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-world-place-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    let home = Home(path.canonicalize().unwrap());
    let root = home.ground();
    central_ctrl::initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work").join(PROJECT)).unwrap();
    fs::write(
        root.join("Work").join(PROJECT).join("README.md"),
        "# notes\n",
    )
    .unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(
        &root,
        &root.join("Work").join(PROJECT),
        PROJECT,
    )
    .unwrap();
    let source = "Control/user/native-action-authority.json";
    let digest = format!("{:x}", Sha256::digest(TOKEN.as_bytes()));
    fs::write(root.join(source), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root",
        "grants":[{"principal_ref":PRINCIPAL,"actor_kind":"human","token_sha256":digest,
            "scope_refs":["control:root",format!("project:{PROJECT}")],"actions":actions,"expires_at_unix_seconds":4_000_000_000u64}]})).unwrap())
    .unwrap();
    fs::create_dir_all(root.join("Control/relations")).unwrap();
    let path = root.join("Control/relations/source-relations.json");
    let mut relations: Value = fs::read(&path)
        .ok()
        .map(|raw| serde_json::from_slice(&raw).unwrap())
        .unwrap_or_else(|| json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[]}));
    relations["relations"].as_array_mut().unwrap().push(json!({"ref":format!("central:source:control:root:{source}"),"path":source,
        "roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user",
        "recognition":"controlled-test-only","recorded_at_unix_seconds":1}));
    fs::write(path, serde_json::to_vec(&relations).unwrap()).unwrap();
    home
}

/// One Action through the real `ctrl`, as the O-I kernel runs it: the person's credential only where `token` is named.
fn cli(home: &Home, action: &str, input: Value, token: Option<&str>) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ctrl"));
    command
        .args(["--json", "--root"])
        .arg(home.ground())
        .args(["action", "run", action, &input.to_string()])
        .env("HOME", &home.0)
        .env_remove("CENTRAL_ROOT")
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .current_dir(&home.0);
    if let Some(token) = token {
        command.env("CENTRAL_NATIVE_TOKEN", token);
    }
    let output = command.output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{action}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// A real place on Earth, as the Sky's hours ask for it (QL's `ql.sky-day-request/v1` place).
fn hastings() -> Value {
    json!({"reference":"geonames:2647356","label":"Hastings","latitude_degrees":50.85,"longitude_degrees":0.57,
        "altitude_metres":10.0,"timezone":"Europe/London"})
}

fn project_of(map: &Value) -> Value {
    map["data"]["work"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["name"] == PROJECT)
        .unwrap()
        .clone()
}

#[test]
fn a_project_world_declares_its_place_on_the_person_s_authority_and_the_world_map_discloses_it() {
    let home = made(&[ACTION]);
    let manifest = home
        .ground()
        .join("Work")
        .join(PROJECT)
        .join("ProjectCentral/project.json");
    let before = project_of(&cli(&home, "central.world", json!({}), None));
    assert!(before.get("world_place").is_none(), "{before}");
    let revision = before["manifest_revision"].as_str().unwrap().to_owned();
    let place = |place: Value, expected: &str, token: Option<&str>| {
        cli(
            &home,
            ACTION,
            json!({"project":PROJECT,"place":place,"expected_revision":expected}),
            token,
        )
    };

    // Only the person's own press declares it.
    let refused = place(hastings(), &revision, None);
    assert_eq!(refused["ok"], false, "{refused}");
    // A place the Sky could not count hours at is refused in words, and nothing is written.
    let mut far_north = hastings();
    far_north["latitude_degrees"] = json!(91.0);
    let mut nowhere = hastings();
    nowhere["timezone"] = json!("Nowhere/Atlantis");
    let mut too_high = hastings();
    too_high["altitude_metres"] = json!(10_001.0);
    let mut unnamed = hastings();
    unnamed["label"] = json!("  ");
    let mut extra = hastings();
    extra["elevation"] = json!(3);
    for wrong in [
        far_north,
        nowhere,
        too_high,
        unnamed,
        extra,
        json!("Hastings"),
    ] {
        let refused = place(wrong.clone(), &revision, Some(TOKEN));
        assert_eq!(refused["ok"], false, "{wrong}: {refused}");
    }
    let stale = place(
        hastings(),
        "central.content-fnv1a64/v1:1:0000000000000000",
        Some(TOKEN),
    );
    assert_eq!(stale["ok"], true, "{stale}");
    assert_eq!(stale["data"]["outcome"], "conflict");
    assert!(project_of(&cli(&home, "central.world", json!({}), None))
        .get("world_place")
        .is_none());

    let placed = place(hastings(), &revision, Some(TOKEN));
    assert_eq!(placed["ok"], true, "{placed}");
    let data = &placed["data"];
    assert_eq!(data["schema"], "central.world-place/v1");
    assert_eq!(data["outcome"], "placed");
    assert_eq!(data["world_ref"], format!("project:{PROJECT}"));
    assert_eq!(data["place"], hastings());
    assert_eq!(data["previous_place"], Value::Null);
    assert_eq!(data["previous_revision"], revision);
    assert_eq!(data["principal_ref"], PRINCIPAL);
    assert_eq!(data["automatic_agent_or_model_invocation"], false);
    let kept: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(kept["place"], hastings());
    assert_eq!(kept["project_id"], PROJECT, "the world's identity stands");
    let after = project_of(&cli(&home, "central.world", json!({}), None));
    assert_eq!(after["world_place"], hastings(), "{after}");
    assert_eq!(after["manifest_revision"], data["revision"]);

    // The same place again changes nothing; none takes it away.
    let same = place(hastings(), data["revision"].as_str().unwrap(), Some(TOKEN));
    assert_eq!(same["data"]["outcome"], "unchanged", "{same}");
    let cleared = place(Value::Null, data["revision"].as_str().unwrap(), Some(TOKEN));
    assert_eq!(cleared["data"]["outcome"], "cleared", "{cleared}");
    assert_eq!(cleared["data"]["previous_place"], hastings());
    let kept: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert!(kept.get("place").is_none(), "{kept}");
    let doctor = cli(
        &home,
        "projectcentral.doctor",
        json!({"project":PROJECT}),
        None,
    );
    assert_eq!(doctor["ok"], true, "{doctor}");
}

#[test]
fn your_world_declares_its_place_on_its_root_world_record() {
    let home = made(&[ACTION]);
    let read = || {
        cli(
            &home,
            "central.world-relations.read",
            json!({"scope":"root","ref":"control:root"}),
            None,
        )
    };
    // With no world record, your world has nowhere to keep a place: refused in words, nothing created.
    let refused = cli(
        &home,
        ACTION,
        json!({"place":hastings(),"expected_revision":"w1"}),
        Some(TOKEN),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused.to_string().contains("world record"), "{refused}");
    assert_eq!(read()["ok"], false);

    let declared = cli(
        &home,
        "central.world-relations.save",
        json!({"scope":"root","record":{"schema":"central.world-relations/v1","ref":"control:root","revision":"w1","parent":null}}),
        None,
    );
    assert_eq!(declared["ok"], true, "{declared}");
    let unauthorised = cli(
        &home,
        ACTION,
        json!({"place":hastings(),"expected_revision":"w1"}),
        None,
    );
    assert_eq!(unauthorised["ok"], false, "{unauthorised}");
    let stale = cli(
        &home,
        ACTION,
        json!({"place":hastings(),"expected_revision":"w0"}),
        Some(TOKEN),
    );
    assert_eq!(stale["data"]["outcome"], "conflict", "{stale}");

    let placed = cli(
        &home,
        ACTION,
        json!({"place":hastings(),"expected_revision":"w1"}),
        Some(TOKEN),
    );
    assert_eq!(placed["ok"], true, "{placed}");
    let data = &placed["data"];
    assert_eq!(data["outcome"], "placed");
    assert_eq!(data["world_ref"], "control:root");
    assert_eq!(data["previous_revision"], "w1");
    assert_eq!(data["revision"], "w2");
    let record = read();
    assert_eq!(record["data"]["record"]["place"], hastings(), "{record}");
    assert_eq!(record["data"]["revision"], "w2");
    assert_eq!(
        record["data"]["record"]["parent"],
        Value::Null,
        "only the place moves"
    );

    // A world record carrying a place the Sky could not count at is refused by the record's own save, too.
    let mut nowhere = hastings();
    nowhere["timezone"] = json!("Nowhere/Atlantis");
    let mut bad = record["data"]["record"].clone();
    bad["revision"] = json!("w3");
    bad["place"] = nowhere;
    let refused = cli(
        &home,
        "central.world-relations.save",
        json!({"scope":"root","record":bad,"expected_revision":"w2"}),
        None,
    );
    assert_eq!(refused["ok"], false, "{refused}");

    let cleared = cli(
        &home,
        ACTION,
        json!({"place":null,"expected_revision":"w2"}),
        Some(TOKEN),
    );
    assert_eq!(cleared["data"]["outcome"], "cleared", "{cleared}");
    assert!(read()["data"]["record"].get("place").is_none());
}
