//! A page leaves its world to Central's trash and comes back through restore (`central.files.trash`, `central.files.restore`), and
//! a world is named by the person (`central.world.name`), its name disclosed by the world map (O-I #743). Both writes are the person's
//! own acts, admitted only on their native authority. Disposable grounds; the test token is no live credential.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const TOKEN: &str = "page-trash-test-token-not-a-live-credential-00000001";
const PRINCIPAL: &str = "human:scratch-world";
const PROJECT: &str = "field-notes";
const FLOWS: &str = "Control/user/flows";
const HUMAN: &str = "Work/field-notes/ProjectCentral/user";
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
    fn put(&self, path: &str, content: &str) {
        fs::write(self.ground().join(path), content).unwrap();
    }
    fn exists(&self, path: &str) -> bool {
        fs::symlink_metadata(self.ground().join(path)).is_ok()
    }
}

/// A ground with your world's Flows home, one project, and the person's grant of `actions` at the root and the project.
fn made(actions: &[&str]) -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-page-trash-{}-{}-{}",
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
    fs::create_dir_all(root.join(FLOWS)).unwrap();
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
    granted(&home, actions);
    home
}

/// The person's grant, written straight into the disposable ground and recognised for the test.
fn granted(home: &Home, actions: &[&str]) {
    let root = home.ground();
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

/// A file's location as the owner's own listing of its folder returns it, and its revision as the owner reads it.
fn located(home: &Home, folder: &str, name: &str) -> (Value, String) {
    let listed = cli(home, "central.files.list", json!({"path":folder}), None);
    assert_eq!(listed["ok"], true, "{listed}");
    let location = listed["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("{name} in {listed}"))["location"]
        .clone();
    let read = cli(
        home,
        "central.files.read",
        json!({"location":location}),
        None,
    );
    assert_eq!(read["ok"], true, "{read}");
    (
        location,
        read["data"]["revision"].as_str().unwrap().to_owned(),
    )
}

const PAGE: &str = "<!doctype html><title>A page</title><script type=\"application/json\" id=\"ql-doc\">{\"meta\":{\"title\":\"A page\"},\"entries\":[]}</script>\n";

#[test]
fn a_page_of_your_world_leaves_for_the_trash_on_the_person_s_authority_and_restore_brings_it_back()
{
    let home = made(&["central.files.trash"]);
    let path = format!("{FLOWS}/flow-2026-10-29-1015.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, FLOWS, "flow-2026-10-29-1015.html");
    let trash = |expected: &str, token: Option<&str>| {
        cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":expected}),
            token,
        )
    };

    // Never without the person: no credential, nothing moves.
    let refused = trash(&revision, None);
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(
        refused.to_string().contains("CENTRAL_NATIVE_TOKEN"),
        "{refused}"
    );
    assert!(home.exists(&path));
    // A stale revision: the page moved since it was read, so nothing moves.
    let stale = trash("central.content-fnv1a64/v1:1:0000000000000000", Some(TOKEN));
    assert_eq!(stale["ok"], true, "{stale}");
    assert_eq!(stale["data"]["schema"], "central.file-trash/v1");
    assert_eq!(stale["data"]["outcome"], "conflict");
    assert_eq!(stale["data"]["current_revision"], revision);
    assert_eq!(stale["data"]["changed"], false);
    assert!(home.exists(&path));

    let trashed = trash(&revision, Some(TOKEN));
    assert_eq!(trashed["ok"], true, "{trashed}");
    let data = &trashed["data"];
    assert_eq!(data["schema"], "central.file-trash/v1");
    assert_eq!(data["outcome"], "trashed");
    assert_eq!(data["location"], location);
    assert_eq!(data["project"], Value::Null);
    assert_eq!(data["previous_revision"], revision);
    assert_eq!(data["revision"], "");
    assert_eq!(data["byte_len"], PAGE.len());
    assert_eq!(data["changed"], true);
    assert_eq!(data["principal_ref"], PRINCIPAL);
    assert_eq!(data["change"]["previous_revision"], revision);
    assert_eq!(data["change"]["revision"], "");
    assert_eq!(data["change"]["actor"], PRINCIPAL);
    assert_eq!(data["change"]["actor_kind"], "human");
    assert_eq!(
        data["recover"],
        json!({"action":"central.files.restore","expected_revision":"","revision":revision})
    );
    assert!(!home.exists(&path), "the page left its world");

    // Its history still reads, the trash its newest event.
    let history = cli(
        &home,
        "central.files.history",
        json!({"location":location}),
        None,
    );
    assert_eq!(history["ok"], true, "{history}");
    assert_eq!(history["data"]["current_revision"], "");
    assert_eq!(history["data"]["entries"][0]["revision"], "");
    assert_eq!(history["data"]["entries"][0]["previous_revision"], revision);

    // Restore brings it back, byte for byte, from the revision the trash kept.
    let restored = cli(
        &home,
        "central.files.restore",
        json!({"location":location,"expected_revision":"","revision":revision,
        "actor":"oi-desktop-user","actor_kind":"human"}),
        None,
    );
    assert_eq!(restored["ok"], true, "{restored}");
    assert_eq!(restored["data"]["schema"], "central.file-mutation/v1");
    assert_eq!(restored["data"]["outcome"], "created");
    assert_eq!(restored["data"]["revision"], revision);
    assert_eq!(restored["data"]["change"]["restored_from"], revision);
    assert_eq!(restored["data"]["change"]["previous_revision"], "");
    assert_eq!(fs::read_to_string(home.ground().join(&path)).unwrap(), PAGE);
    // It stands again: a second restore from absence is a conflict, as any stale write is.
    let again = cli(
        &home,
        "central.files.restore",
        json!({"location":location,"expected_revision":"","revision":revision,
        "actor":"oi-desktop-user","actor_kind":"human"}),
        None,
    );
    assert_eq!(again["data"]["outcome"], "conflict", "{again}");
}

#[test]
fn a_project_page_leaves_its_world_attributed_to_the_person_and_comes_back_through_its_door() {
    let home = made(&["central.files.trash"]);
    let path = format!("{HUMAN}/flow-2026-10-29-1016.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, HUMAN, "flow-2026-10-29-1016.html");
    // A page in a project names its project, and only that one.
    for project in [Value::Null, json!("elsewhere")] {
        let refused = cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":revision,"project":project}),
            Some(TOKEN),
        );
        assert_eq!(refused["ok"], false, "{refused}");
        assert!(home.exists(&path));
    }
    let trashed = cli(
        &home,
        "central.files.trash",
        json!({"location":location,"expected_revision":revision,"project":PROJECT}),
        Some(TOKEN),
    );
    assert_eq!(trashed["ok"], true, "{trashed}");
    assert_eq!(trashed["data"]["outcome"], "trashed");
    assert_eq!(trashed["data"]["project"], PROJECT);
    assert!(!home.exists(&path));
    // The project's source horizon records it Removed, the person's act.
    let horizon = cli(
        &home,
        "projectcentral.change.horizon",
        json!({"project":PROJECT}),
        None,
    );
    let changes = horizon["data"]["changes"].as_array().unwrap();
    let removed = changes
        .iter()
        .rev()
        .find(|change| change["source_path"] == "ProjectCentral/user/flow-2026-10-29-1016.html")
        .unwrap_or_else(|| panic!("{horizon}"));
    assert_eq!(removed["kind"], "removed", "{removed}");
    assert_eq!(removed["actor"], PRINCIPAL);
    assert_eq!(removed["actor_kind"], "human");

    let restored = cli(
        &home,
        "central.files.restore",
        json!({"location":location,"expected_revision":"","revision":revision,
        "actor":"oi-desktop-user","actor_kind":"human"}),
        None,
    );
    assert_eq!(restored["ok"], true, "{restored}");
    assert_eq!(restored["data"]["outcome"], "created");
    assert_eq!(restored["data"]["change"]["restored_from"], revision);
    assert_eq!(fs::read_to_string(home.ground().join(&path)).unwrap(), PAGE);
    // Back through the project's own creation door: the horizon has it again, as the project's own source.
    let horizon = cli(
        &home,
        "projectcentral.change.horizon",
        json!({"project":PROJECT}),
        None,
    );
    let added = horizon["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|change| change["source_path"] == "ProjectCentral/user/flow-2026-10-29-1016.html")
        .unwrap()
        .clone();
    assert_eq!(added["kind"], "added", "{added}");
    assert_eq!(added["actor_kind"], "human");
}

#[test]
fn only_a_page_in_a_world_s_page_folder_leaves_and_an_untrashed_absence_is_no_restore() {
    let home = made(&["central.files.trash"]);
    home.put("Work/field-notes/notes.html", PAGE);
    fs::create_dir_all(home.ground().join("Control/user/elsewhere")).unwrap();
    home.put("Control/user/elsewhere/page.html", PAGE);
    for (folder, name, project) in [
        ("Work/field-notes", "notes.html", json!(PROJECT)),
        ("Work/field-notes", "README.md", json!(PROJECT)),
        ("Control/user/elsewhere", "page.html", Value::Null),
    ] {
        let (location, revision) = located(&home, folder, name);
        let refused = cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":revision,"project":project}),
            Some(TOKEN),
        );
        assert_eq!(refused["ok"], false, "{folder}/{name}: {refused}");
        assert!(
            refused.to_string().contains("page folder"),
            "{folder}/{name}: {refused}"
        );
        assert!(home.exists(&format!("{folder}/{name}")));
    }
    // An absent page whose history records no trash has nothing to restore.
    let path = format!("{FLOWS}/flow-2026-10-29-1017.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, FLOWS, "flow-2026-10-29-1017.html");
    fs::remove_file(home.ground().join(&path)).unwrap();
    let refused = cli(
        &home,
        "central.files.restore",
        json!({"location":location,"expected_revision":"","revision":revision,
        "actor":"oi-desktop-user","actor_kind":"human"}),
        None,
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(!home.exists(&path));
}

#[test]
fn a_world_is_named_by_the_person_and_the_world_map_discloses_its_name() {
    let home = made(&["central.world.name"]);
    let manifest = home
        .ground()
        .join("Work")
        .join(PROJECT)
        .join("ProjectCentral/project.json");
    let project_of = |map: &Value| {
        map["data"]["work"]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|project| project["name"] == PROJECT)
            .unwrap()
            .clone()
    };
    let before = project_of(&cli(&home, "central.world", json!({}), None));
    assert!(before.get("world_name").is_none(), "{before}");
    let revision = before["manifest_revision"].as_str().unwrap().to_owned();
    let name = |name: &str, expected: &str, token: Option<&str>| {
        cli(
            &home,
            "central.world.name",
            json!({"project":PROJECT,"name":name,"expected_revision":expected}),
            token,
        )
    };

    let refused = name("Field Notes", &revision, None);
    assert_eq!(refused["ok"], false, "{refused}");
    for words in ["   ", "a\u{7}b", &"x".repeat(121)] {
        let refused = name(words, &revision, Some(TOKEN));
        assert_eq!(refused["ok"], false, "{words:?}: {refused}");
    }
    let stale = name(
        "Field Notes",
        "central.content-fnv1a64/v1:1:0000000000000000",
        Some(TOKEN),
    );
    assert_eq!(stale["ok"], true, "{stale}");
    assert_eq!(stale["data"]["outcome"], "conflict");
    assert!(project_of(&cli(&home, "central.world", json!({}), None))
        .get("world_name")
        .is_none());

    let named = name("  Field Notes  ", &revision, Some(TOKEN));
    assert_eq!(named["ok"], true, "{named}");
    let data = &named["data"];
    assert_eq!(data["schema"], "central.world-name/v1");
    assert_eq!(data["outcome"], "named");
    assert_eq!(data["project"], PROJECT);
    assert_eq!(data["world_ref"], format!("project:{PROJECT}"));
    assert_eq!(data["name"], "Field Notes");
    assert_eq!(data["previous_name"], Value::Null);
    assert_eq!(data["previous_revision"], revision);
    assert_eq!(data["principal_ref"], PRINCIPAL);
    let kept: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(kept["name"], "Field Notes");
    assert_eq!(
        kept["project_id"], PROJECT,
        "the world's identity is unchanged"
    );
    let after = project_of(&cli(&home, "central.world", json!({}), None));
    assert_eq!(after["world_name"], "Field Notes", "{after}");
    assert_eq!(after["manifest_revision"], data["revision"]);
    assert_eq!(after["name"], PROJECT, "its folder keeps its name");
    let projected = cli(
        &home,
        "central.world.project",
        json!({"project":PROJECT}),
        None,
    );
    assert_eq!(
        projected["data"]["project"]["world_name"], "Field Notes",
        "{projected}"
    );

    // The same name again changes nothing.
    let same = name(
        "Field Notes",
        data["revision"].as_str().unwrap(),
        Some(TOKEN),
    );
    assert_eq!(same["data"]["outcome"], "unchanged", "{same}");
    assert_eq!(same["data"]["revision"], data["revision"]);
    let renamed = name(
        "Notes from the field",
        data["revision"].as_str().unwrap(),
        Some(TOKEN),
    );
    assert_eq!(renamed["data"]["outcome"], "named", "{renamed}");
    assert_eq!(renamed["data"]["previous_name"], "Field Notes");
    // The project still stands whole.
    let doctor = cli(
        &home,
        "projectcentral.doctor",
        json!({"project":PROJECT}),
        None,
    );
    assert_eq!(doctor["ok"], true, "{doctor}");
}
