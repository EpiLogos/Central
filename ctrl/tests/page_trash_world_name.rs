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

/// Central's trash stands on the world's own folders, its Control and Work. A world missing one cannot take a page to the trash, and says
/// which, in words, never as the system's raw error (O-I #747 round 895, DLW build ruling 2026-11-08: Delete in a walk world with no Work
/// folder answered "No such file or directory (os error 2)"). Nothing moves.
#[test]
fn a_world_missing_a_folder_the_trash_stands_on_says_which_in_words_and_nothing_moves() {
    let home = made(&["central.files.trash"]);
    let path = format!("{FLOWS}/flow-2026-11-09-0900.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, FLOWS, "flow-2026-11-09-0900.html");
    fs::rename(home.ground().join("Work"), home.ground().join("Work.away")).unwrap();
    let refused = cli(
        &home,
        "central.files.trash",
        json!({"location":location,"expected_revision":revision,"project":Value::Null}),
        Some(TOKEN),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    let said = refused.to_string();
    assert!(!said.contains("os error"), "{refused}");
    assert!(
        said.contains(
            "This world's Work folder is missing, and Central's trash stands on it: nothing was moved."
        ),
        "{refused}"
    );
    assert!(home.exists(&path), "nothing moved");
}

/// The trash carries who acts, as a save does (`actor`, `actor_kind`; O-I #747 round 895, DLW build ruling 2026-11-09): the act is the
/// person's own, so a caller that says it is an agent is refused in words even on the person's token, and nothing moves; the person's
/// own attribution goes.
#[test]
fn the_trash_takes_who_acts_and_refuses_a_caller_that_is_not_human_in_words() {
    let home = made(&["central.files.trash"]);
    let path = format!("{FLOWS}/flow-2026-11-09-1000.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, FLOWS, "flow-2026-11-09-1000.html");
    let trash = |actor: &str, kind: &str| {
        cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":revision,"project":Value::Null,"actor":actor,"actor_kind":kind}),
            Some(TOKEN),
        )
    };
    let agent = trash("agent:lane", "agent");
    assert_eq!(agent["ok"], false, "{agent}");
    assert!(
        agent.to_string().contains(
            "Central's trash takes a page by the person's own act: a caller that is not human cannot move it, and nothing was moved"
        ),
        "{agent}"
    );
    assert!(home.exists(&path), "nothing moved");
    // O-I #747 round 896, 0R-authority.10: a kind that is not text is refused by that cause, and an actor is bounded as a save bounds it.
    let said = |input: Value| {
        let refused = cli(&home, "central.files.trash", input, Some(TOKEN));
        assert_eq!(refused["ok"], false, "{refused}");
        assert!(home.exists(&path), "nothing moved: {refused}");
        refused.to_string()
    };
    let numeric = said(
        json!({"location":location,"expected_revision":revision,"project":Value::Null,"actor":"human:test","actor_kind":7}),
    );
    assert!(numeric.contains("actor_kind must be text"), "{numeric}");
    for actor in [json!("x".repeat(1025)), json!("  "), json!(7)] {
        let refused = said(
            json!({"location":location,"expected_revision":revision,"project":Value::Null,"actor":actor,"actor_kind":"human"}),
        );
        assert!(
            refused.contains("actor must be bounded and nonempty"),
            "{refused}"
        );
    }
    let person = trash("human:test", "human");
    assert_eq!(person["ok"], true, "{person}");
    assert_eq!(person["data"]["outcome"], "trashed", "{person}");
    assert!(!home.exists(&path));
    // Who acted is recorded in the trash's event, as the caller said it; the person the token authenticated is the receipt's principal.
    assert_eq!(person["data"]["change"]["actor"], "human:test", "{person}");
    assert_eq!(person["data"]["principal_ref"], PRINCIPAL);
    let history = cli(
        &home,
        "central.files.history",
        json!({"location":location}),
        None,
    );
    assert_eq!(history["ok"], true, "{history}");
    let newest = &history["data"]["entries"][0];
    assert_eq!(
        (&newest["revision"], &newest["actor"], &newest["actor_kind"]),
        (&json!(""), &json!("human:test"), &json!("human")),
        "the trash is the newest event: {history}"
    );
}

/// A project page's trash stands on every place its project's scope reads: the world's Work, the project's own folder, its ProjectCentral
/// and the `project.json` there. Each, missing, is named in words, never as the system's raw error or as a page outside a page folder,
/// and nothing moves (O-I #747 round 896, 0R-authority.6).
#[test]
fn a_project_page_whose_ground_is_missing_names_the_place_in_words_and_nothing_moves() {
    let home = made(&["central.files.trash"]);
    let path = format!("{HUMAN}/flow-2026-11-09-1100.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, HUMAN, "flow-2026-11-09-1100.html");
    for (place, said) in [
        (
            "Work/field-notes/ProjectCentral/project.json",
            "This world's Work/field-notes/ProjectCentral/project.json file is missing",
        ),
        (
            "Work/field-notes/ProjectCentral",
            "This world's Work/field-notes/ProjectCentral folder is missing",
        ),
        (
            "Work/field-notes",
            "This world's Work/field-notes folder is missing",
        ),
        ("Work", "This world's Work folder is missing"),
    ] {
        let away = format!("{place}.away");
        fs::rename(home.ground().join(place), home.ground().join(&away)).unwrap();
        let refused = cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":revision,"project":PROJECT}),
            Some(TOKEN),
        );
        fs::rename(home.ground().join(&away), home.ground().join(place)).unwrap();
        assert_eq!(refused["ok"], false, "{place}: {refused}");
        let words = refused.to_string();
        assert!(
            !words.contains("os error") && !words.contains("page folder"),
            "{place}: {refused}"
        );
        assert!(
            words.contains(&format!(
                "{said}, and Central's trash stands on it: nothing was moved."
            )),
            "{place}: {refused}"
        );
        assert!(home.exists(&path), "{place}: nothing moved");
    }
    // Its ground whole again, the same page leaves.
    let trashed = cli(
        &home,
        "central.files.trash",
        json!({"location":location,"expected_revision":revision,"project":PROJECT}),
        Some(TOKEN),
    );
    assert_eq!(trashed["data"]["outcome"], "trashed", "{trashed}");
}

/// The trash reads the person's authority before anything moves: the world's source relations, and the authority source they recognise.
/// A world missing either says which, in words, never as the system's raw error, and nothing moves (O-I #747 round 896, F7: the
/// missing-place words covered the scope's own reads, not the authority's). Its ground whole again, the same page leaves.
#[test]
fn a_world_missing_the_ground_of_the_person_s_authority_says_which_in_words_and_nothing_moves() {
    let home = made(&["central.files.trash"]);
    let path = format!("{FLOWS}/flow-2026-11-09-1200.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, FLOWS, "flow-2026-11-09-1200.html");
    for place in [
        "Control/user/native-action-authority.json",
        "Control/relations/source-relations.json",
    ] {
        let away = format!("{place}.away");
        fs::rename(home.ground().join(place), home.ground().join(&away)).unwrap();
        let refused = cli(
            &home,
            "central.files.trash",
            json!({"location":location,"expected_revision":revision,"project":Value::Null}),
            Some(TOKEN),
        );
        fs::rename(home.ground().join(&away), home.ground().join(place)).unwrap();
        assert_eq!(refused["ok"], false, "{place}: {refused}");
        let words = refused.to_string();
        assert!(!words.contains("os error"), "{place}: {refused}");
        assert!(
            words.contains(&format!(
                "This world's {place} file is missing, and Central's trash reads the person's authority from it: nothing was moved."
            )),
            "{place}: {refused}"
        );
        assert!(home.exists(&path), "{place}: nothing moved");
    }
    let trashed = cli(
        &home,
        "central.files.trash",
        json!({"location":location,"expected_revision":revision,"project":Value::Null}),
        Some(TOKEN),
    );
    assert_eq!(trashed["data"]["outcome"], "trashed", "{trashed}");
}

/// A trash keeps who the caller says acted and, beside it, the person the token authenticated (O-I #747 round 896, F8): the claim is
/// the caller's, as a save's is, and the principal is the grant's. The event carries both, and so does its history; the project's source
/// horizon is attributed to the claimed actor, read back here.
#[test]
fn a_trash_keeps_the_authenticated_person_beside_the_actor_it_claims() {
    let home = made(&["central.files.trash"]);
    let path = format!("{HUMAN}/flow-2026-11-09-1300.html");
    home.put(&path, PAGE);
    let (location, revision) = located(&home, HUMAN, "flow-2026-11-09-1300.html");
    let trashed = cli(
        &home,
        "central.files.trash",
        json!({"location":location,"expected_revision":revision,"project":PROJECT,"actor":"human:named-by-the-caller","actor_kind":"human"}),
        Some(TOKEN),
    );
    assert_eq!(trashed["data"]["outcome"], "trashed", "{trashed}");
    let change = &trashed["data"]["change"];
    assert_eq!(
        (&change["actor"], &change["principal_ref"]),
        (&json!("human:named-by-the-caller"), &json!(PRINCIPAL)),
        "{trashed}"
    );
    let history = cli(
        &home,
        "central.files.history",
        json!({"location":location}),
        None,
    );
    let newest = &history["data"]["entries"][0];
    assert_eq!(
        (
            &newest["revision"],
            &newest["actor"],
            &newest["principal_ref"]
        ),
        (
            &json!(""),
            &json!("human:named-by-the-caller"),
            &json!(PRINCIPAL)
        ),
        "{history}"
    );
    let horizon = cli(
        &home,
        "projectcentral.change.horizon",
        json!({"project":PROJECT}),
        None,
    );
    let removed = horizon["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|change| change["source_path"] == "ProjectCentral/user/flow-2026-11-09-1300.html")
        .unwrap_or_else(|| panic!("{horizon}"));
    assert_eq!(
        (&removed["kind"], &removed["actor"], &removed["actor_kind"]),
        (
            &json!("removed"),
            &json!("human:named-by-the-caller"),
            &json!("human")
        ),
        "{removed}"
    );
    // A restore names no principal, as no token authenticated it.
    let restored = cli(
        &home,
        "central.files.restore",
        json!({"location":location,"expected_revision":"","revision":revision,
        "actor":"oi-desktop-user","actor_kind":"human"}),
        None,
    );
    assert_eq!(restored["data"]["outcome"], "created", "{restored}");
    assert!(
        restored["data"]["change"].get("principal_ref").is_none(),
        "{restored}"
    );
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
