//! A move in the file map is the person's own act (DLW build ruling 2026-11-09, round 53 item 2; O-I 0G's review): Central admits
//! `central.file-map.move-plan`, `central.file-map.move-apply` and `central.file-map.move-rollback` only on the host's native
//! credential (`CENTRAL_NATIVE_TOKEN`) for a human grant naming that exact Action at the root, as its other owner-authority acts
//! do. Anything that runs `ctrl` directly with no credential, a credential no grant names, or an agent's credential is refused in
//! Central's words, and nothing is planned or moved. `move-plan` is no read: Central classes it locally-mutating, and it writes the
//! durable journal that names the place a file goes, which apply and rollback then carry out by its id alone.
//!
//!
//! Since round 54's Central follow-up (DLW build ruling 2026-11-09), so is each of the file map's acts that changes the person's
//! ground or what they have adopted: `register`, `link`, `scope-register`, `adopt-db`, `record-adopt` and `pool` (file_map.rs
//! `PERSON_ACTS` says what each writes). Each is held by a grant where it stands (file_map.rs `grant_scope`): an act that names a
//! project of Work by the grant at that project, as Central's other acts that name a project are; the moves, `scope-register` and an
//! act at the root by the root's. A grant that names only some is refused at the others, and a grant is refused wherever it does not
//! stand. In process, `file_map::execute` carries no credential and is refused alike.
//!
//! Test credentials and authority fixtures exist only in this disposable world. bkmr never runs: the world names a bkmr that is not
//! there, so a call to it could only fail.
use central_ctrl::continuous_work::source::Scope;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const MOVES: [&str; 3] = [
    "central.file-map.move-plan",
    "central.file-map.move-apply",
    "central.file-map.move-rollback",
];
/// The file map's other acts of the person's own (round 54's Central follow-up).
const GROUND_ACTS: [&str; 6] = [
    "central.file-map.register",
    "central.file-map.link",
    "central.file-map.scope-register",
    "central.file-map.adopt-db",
    "central.file-map.record-adopt",
    "central.file-map.pool",
];
const PERSON: &str = "file-map-moves-person-controlled-test-not-a-live-secret";
const AGENT: &str = "file-map-moves-agent-controlled-test-not-a-live-secret";
const UNKNOWN: &str = "file-map-moves-unknown-controlled-test-not-a-live-secret";

struct World(PathBuf);
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl World {
    fn root(&self) -> PathBuf {
        self.0.join("Central")
    }
}

/// One `ctrl action run` as a separate process, with the credential (or none) in its environment only.
fn ctrl(world: &World, action: &str, input: &Value, token: Option<&str>) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ctrl"));
    command
        .args([
            "--json",
            "--root",
            world.root().to_str().unwrap(),
            "action",
            "run",
            action,
            &input.to_string(),
        ])
        .current_dir(&world.0)
        .env("CENTRAL_ROOT", world.root())
        .env("HOME", world.0.join("home"))
        .env("CENTRAL_BKMR_BIN", world.0.join("no-bkmr-here"));
    match token {
        Some(token) => command.env("CENTRAL_NATIVE_TOKEN", token),
        None => command.env_remove("CENTRAL_NATIVE_TOKEN"),
    };
    let output = command.output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{action}: {error}; stdout {}; stderr {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// A Central root holding one page of the person's own, and the authored native-action-authority source the host credential is
/// checked against: the person's grant for the three moves at the root, and an agent's grant for the same acts.
fn world() -> World {
    granting(&MOVES, "control:root")
}

/// [`world`] whose person's grant names `actions` at `scope` (the agent's names them at the root), with a project `alpha` in Work.
fn granting(actions: &[&str], scope: &str) -> World {
    let world = World(
        std::env::temp_dir().join(format!("central-file-map-moves-{}", uuid::Uuid::new_v4())),
    );
    fs::create_dir_all(world.0.join("home")).unwrap();
    let root = world.root();
    central_ctrl::initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/note.md"), "the person's page\n").unwrap();
    fs::create_dir_all(root.join("Work/alpha/ProjectCentral/user")).unwrap();
    fs::write(
        root.join("Work/alpha/ProjectCentral/project.json"),
        json!({"schema":"central.project/v1","project_id":"alpha","human_source":"ProjectCentral/user",
            "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}})
        .to_string(),
    )
    .unwrap();
    fs::write(root.join("Work/alpha/plan.md"), "alpha's plan\n").unwrap();
    let grant = |token: &str, kind: &str, scope: &str| {
        json!({"principal_ref":format!("{kind}:file-map-moves-test"),"actor_kind":kind,
            "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":[scope],
            "actions":actions,"expires_at_unix_seconds":u64::MAX})
    };
    let authority = "Control/user/file-map-moves-authority.json";
    fs::write(
        root.join(authority),
        serde_json::to_vec(
            &json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root",
            "grants":[grant(PERSON, "human", scope), grant(AGENT, "agent", "control:root")]}),
        )
        .unwrap(),
    )
    .unwrap();
    let relations = root.join("Control/relations/source-relations.json");
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    let mut document: Value = fs::read(&relations)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[]}));
    let scope = Scope::resolve(&root, None).unwrap();
    document["relations"].as_array_mut().unwrap().push(json!({
        "ref":scope.source_ref(authority),"path":authority,"roles":["native-action-authority"],
        "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
        "recognition":"controlled-test-only-not-personal-adoption","recorded_at_unix_seconds":1}));
    fs::write(&relations, serde_json::to_vec(&document).unwrap()).unwrap();
    world
}

/// The journals Central keeps for its moves (file_map_moves.rs `save`): none is written by a refused plan.
fn journals(root: &Path) -> usize {
    fs::read_dir(root.join(".central/bkmr/moves"))
        .map(|entries| {
            entries
                .filter(|entry| {
                    entry.as_ref().is_ok_and(|entry| {
                        entry.path().extension().is_some_and(|ext| ext == "json")
                    })
                })
                .count()
        })
        .unwrap_or(0)
}

/// Each credential that is not the person's is refused in Central's words, opening with whose act it is, then the cause.
fn refused_without_the_person(world: &World, action: &str, input: &Value, opening: &str) {
    for (token, cause) in [
        (None, "missing bounded CENTRAL_NATIVE_TOKEN"),
        (Some(UNKNOWN), "native credential is not authorised"),
        (Some(AGENT), "requires a human-scoped native credential"),
    ] {
        refused_in_words(world, action, input, token, opening, cause);
    }
}

/// One refusal of `action` on `token`: Central's denial, in words that open with whose act it is and name `cause`.
fn refused_in_words(
    world: &World,
    action: &str,
    input: &Value,
    token: Option<&str>,
    opening: &str,
    cause: &str,
) {
    let refused = ctrl(world, action, input, token);
    let case = format!("{action} with {}", token.unwrap_or("no credential"));
    assert_eq!(refused["ok"], false, "{case}: {refused}");
    assert_eq!(
        refused["error"]["code"], "central.file_map_denied",
        "{case}: {refused}"
    );
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(words.starts_with(opening), "{case}: {words}");
    assert!(words.contains(cause), "{case}: {words}");
}

/// Every file of the world but the disposable home, with its bytes, or where it links: what a refused act must leave as it was.
fn ground(world: &World) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![world.root()];
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_symlink() {
                let target = fs::read_link(&path).unwrap();
                files.push((path, target.to_string_lossy().as_bytes().to_vec()));
            } else if kind.is_dir() {
                files.push((path.clone(), Vec::new()));
                stack.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

/// The file map's revision, which every write of the ground's is made on.
fn revision(world: &World, project: Option<&str>) -> String {
    let inspected = ctrl(
        world,
        "central.file-map.inspect",
        &json!({"project":project,"resources":false}),
        None,
    );
    assert_eq!(inspected["ok"], true, "{inspected}");
    inspected["data"]["result"]["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Where the person's page stands as a Source.
fn page_source(world: &World) -> String {
    let located = ctrl(
        world,
        "central.file-map.locate",
        &json!({"path":"Control/user/note.md"}),
        None,
    );
    assert_eq!(located["ok"], true, "{located}");
    located["data"]["result"]["source"]["ref"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_move_is_the_persons_own_act_and_anything_else_that_runs_ctrl_is_refused_in_centrals_words() {
    let world = world();
    let root = world.root();
    let page = root.join("Control/user/note.md");
    let moved = root.join("Control/user/renamed.md");
    // Reading where the page stands is a read, and needs no credential.
    let located = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"path":"Control/user/note.md"}),
        None,
    );
    assert_eq!(located["ok"], true, "{located}");
    let source = located["data"]["result"]["source"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let plan = json!({"source_ref":source,"destination":"Control/user/renamed.md"});

    // The plan names the place: no one but the person writes it.
    refused_without_the_person(
        &world,
        "central.file-map.move-plan",
        &plan,
        "Planning a move is the person's own press: ",
    );
    assert_eq!(journals(&root), 0, "a refused plan writes no journal");
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(journals(&root), 1);
    let plan_id = planned["data"]["result"]["plan_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let carried = json!({"plan_id":plan_id,"quiesced":true});

    refused_without_the_person(
        &world,
        "central.file-map.move-apply",
        &carried,
        "Moving a file is the person's own press: ",
    );
    assert!(
        page.is_file() && !moved.exists(),
        "a refused move moves nothing"
    );
    let applied = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert!(
        !page.exists() && moved.is_file(),
        "the person's press moves the page"
    );

    refused_without_the_person(
        &world,
        "central.file-map.move-rollback",
        &carried,
        "Moving a file back is the person's own press: ",
    );
    assert!(
        moved.is_file() && !page.exists(),
        "a refused rollback moves nothing"
    );
    let rolled = ctrl(
        &world,
        "central.file-map.move-rollback",
        &carried,
        Some(PERSON),
    );
    assert_eq!(rolled["ok"], true, "{rolled}");
    assert!(page.is_file() && !moved.exists(), "and back");
    assert_eq!(fs::read_to_string(&page).unwrap(), "the person's page\n");
}

/// Round 54's Central follow-up (DLW build ruling 2026-11-09; review of #338, C1): a grant that names only some of the person's acts is
/// refused at the others, whatever else it names.
#[test]
fn a_grant_that_names_only_some_of_the_persons_acts_is_refused_at_the_others() {
    let world = granting(
        &["central.file-map.move-plan", "central.file-map.register"],
        "control:root",
    );
    let root = world.root();
    let source = page_source(&world);
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/renamed.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let plan_id = planned["data"]["result"]["plan_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = ground(&world);
    let lacks = "lacks this exact scope/action";
    let carried = json!({"plan_id":plan_id,"quiesced":true});
    for (action, input, opening) in [
        (
            "central.file-map.move-apply",
            carried.clone(),
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            carried,
            "Moving a file back is the person's own press: ",
        ),
        (
            "central.file-map.link",
            json!({"source_ref":source,"path":"links/note","owner":"person","expected_revision":revision(&world, None)}),
            "Linking a Source into a folder is the person's own press: ",
        ),
        (
            "central.file-map.pool",
            json!({"enable":true}),
            "Making a scope's whole tree Source is the person's own press: ",
        ),
    ] {
        refused_in_words(&world, action, &input, Some(PERSON), opening, lacks);
    }
    assert_eq!(
        ground(&world),
        before,
        "an act the grant does not name does nothing"
    );
    assert!(root.join("Control/user/note.md").is_file());
    // And what it names it takes.
    fs::write(root.join("Control/user/second.md"), "a second page\n").unwrap();
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Control/user/second.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
}

/// The same follow-up (C1): a grant holds the person's acts only where it stands. One at a project holds that project's register and
/// pool, and is refused at the root's acts (a move, a scope's register, a register at the root) whatever project they name; one at the
/// root is refused at an act of the project's. Nothing refused is planned or written.
#[test]
fn a_grant_holds_the_persons_acts_only_where_it_stands() {
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    let before = ground(&world);
    let lacks = "lacks this exact scope/action";
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &json!({"project":"alpha","source_ref":page_source(&world),"destination":"Work/alpha/note.md"}),
        Some(PERSON),
        "Planning a move is the person's own press: ",
        lacks,
    );
    assert_eq!(journals(&root), 0, "a refused plan writes no journal");
    refused_in_words(
        &world,
        "central.file-map.scope-register",
        &json!({"project":"alpha","name":"Music","path":world.0.join("home"),"kind":"folder","allow_external":true,
            "media":["wav"],"expected_revision":revision(&world, None)}),
        Some(PERSON),
        "Registering a scope is the person's own press: ",
        lacks,
    );
    refused_in_words(
        &world,
        "central.file-map.register",
        &json!({"path":"Control/user/note.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
        "Registering a Source is the person's own press: ",
        lacks,
    );
    assert_eq!(ground(&world), before, "nothing is written at the root");
    // At the project it stands at, by the project's name or its World, the grant holds the project's acts.
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":"alpha","path":"plan.md","expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let pooled = ctrl(
        &world,
        "central.file-map.pool",
        &json!({"project":"project:alpha","enable":false}),
        Some(PERSON),
    );
    assert_eq!(pooled["ok"], true, "{pooled}");
    // And a grant at the root is refused at the project's.
    let world = granting(&acts, "control:root");
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.register",
        &json!({"project":"alpha","path":"plan.md","expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
        "Registering a Source is the person's own press: ",
        lacks,
    );
    assert_eq!(ground(&world), before, "nothing is written at the project");
}

/// The same follow-up (C1): in process, `file_map::execute` carries no credential, so each of the person's acts is refused there in
/// the same words, before anything is read or written; a read is not.
#[test]
fn in_process_execute_carries_no_credential_and_each_of_the_persons_acts_is_refused() {
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let world = granting(&acts, "control:root");
    let root = world.root();
    let before = ground(&world);
    for (op, opening) in [
        ("move-plan", "Planning a move is the person's own press: "),
        ("move-apply", "Moving a file is the person's own press: "),
        (
            "move-rollback",
            "Moving a file back is the person's own press: ",
        ),
        (
            "register",
            "Registering a Source is the person's own press: ",
        ),
        (
            "link",
            "Linking a Source into a folder is the person's own press: ",
        ),
        (
            "scope-register",
            "Registering a scope is the person's own press: ",
        ),
        (
            "adopt-db",
            "Adopting a bookmark database is the person's own press: ",
        ),
        (
            "record-adopt",
            "Adopting a bookmark is the person's own press: ",
        ),
        (
            "pool",
            "Making a scope's whole tree Source is the person's own press: ",
        ),
    ] {
        let refused = central_ctrl::file_map::execute(&root, op, &json!({})).unwrap_err();
        assert_eq!(
            refused.kind(),
            std::io::ErrorKind::PermissionDenied,
            "{op}: {refused}"
        );
        let words = refused.to_string();
        assert!(words.starts_with(opening), "{op}: {words}");
        assert!(
            words.contains("missing bounded CENTRAL_NATIVE_TOKEN"),
            "{op}: {words}"
        );
    }
    assert_eq!(ground(&world), before);
    assert!(central_ctrl::file_map::execute(&root, "inspect", &json!({"resources":false})).is_ok());
}

/// Round 54's Central follow-up (DLW build ruling 2026-11-09; review of #338, C4): each of the file map's acts that changes the person's
/// ground or what they have adopted is theirs. Run with no credential, a credential no grant names, or an agent's, each is refused in
/// Central's words and the ground is as it was; on the person's own credential each goes past their gate.
#[test]
fn the_file_maps_acts_on_the_persons_ground_are_theirs_and_anything_else_that_runs_ctrl_is_refused()
{
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let world = granting(&acts, "control:root");
    let root = world.root();
    let music = world.0.join("Music");
    fs::create_dir_all(&music).unwrap();
    fs::write(music.join("voice.wav"), b"RIFF").unwrap();
    let bookmarks = world.0.join("bookmarks.db");
    rusqlite::Connection::open(&bookmarks)
        .unwrap()
        .execute_batch(
            "CREATE TABLE kept(value TEXT); INSERT INTO kept VALUES ('the person''s bookmarks');",
        )
        .unwrap();
    fs::write(root.join("Control/user/second.md"), "a second page\n").unwrap();
    let source = page_source(&world);

    // register: the Source written into the root's relations document.
    let register =
        json!({"path":"Control/user/second.md","expected_revision":revision(&world, None)});
    let before = ground(&world);
    refused_without_the_person(
        &world,
        "central.file-map.register",
        &register,
        "Registering a Source is the person's own press: ",
    );
    assert_eq!(ground(&world), before, "a refused register writes nothing");
    let registered = ctrl(&world, "central.file-map.register", &register, Some(PERSON));
    assert_eq!(registered["ok"], true, "{registered}");

    // link: a link made in the person's own folder, and kept in the relations document.
    let link = json!({"source_ref":source,"path":"links/note","owner":"person","expected_revision":revision(&world, None)});
    let before = ground(&world);
    refused_without_the_person(
        &world,
        "central.file-map.link",
        &link,
        "Linking a Source into a folder is the person's own press: ",
    );
    assert_eq!(ground(&world), before, "a refused link makes none");
    let linked = ctrl(&world, "central.file-map.link", &link, Some(PERSON));
    assert_eq!(linked["ok"], true, "{linked}");
    assert!(fs::symlink_metadata(root.join("links/note"))
        .unwrap()
        .file_type()
        .is_symlink());

    // pool: whether the whole tree is Source, kept in the relations document.
    let pool = json!({"enable":false});
    let before = ground(&world);
    refused_without_the_person(
        &world,
        "central.file-map.pool",
        &pool,
        "Making a scope's whole tree Source is the person's own press: ",
    );
    assert_eq!(ground(&world), before);
    let pooled = ctrl(&world, "central.file-map.pool", &pool, Some(PERSON));
    assert_eq!(pooled["ok"], true, "{pooled}");

    // scope-register: which of the person's folders the map reads.
    let scope = json!({"name":"Music","path":music,"kind":"folder","allow_external":true,"media":["wav"],
        "expected_revision":revision(&world, None)});
    let before = ground(&world);
    refused_without_the_person(
        &world,
        "central.file-map.scope-register",
        &scope,
        "Registering a scope is the person's own press: ",
    );
    assert_eq!(
        ground(&world),
        before,
        "a refused register declares no scope"
    );
    let scoped = ctrl(
        &world,
        "central.file-map.scope-register",
        &scope,
        Some(PERSON),
    );
    assert_eq!(scoped["ok"], true, "{scoped}");
    assert_eq!(scoped["data"]["result"]["world_ref"], "folder:Music");

    // adopt-db and record-adopt: what the person has adopted. On their credential each goes past their gate, to what it does
    // (here bkmr is not there, so adoption stops at it in its own words).
    for (action, input, opening) in [
        (
            "central.file-map.adopt-db",
            json!({"database":bookmarks,"quiesced":true,"expected_revision":revision(&world, None)}),
            "Adopting a bookmark database is the person's own press: ",
        ),
        (
            "central.file-map.record-adopt",
            json!({"source_ref":source,"record_id":1,"record_revision":"r","expected_revision":revision(&world, None)}),
            "Adopting a bookmark is the person's own press: ",
        ),
    ] {
        let before = ground(&world);
        refused_without_the_person(&world, action, &input, opening);
        assert_eq!(
            ground(&world),
            before,
            "{action}: a refused adoption adopts nothing"
        );
        let pressed = ctrl(&world, action, &input, Some(PERSON));
        let words = pressed["error"]["message"].as_str().unwrap_or_default();
        assert!(
            !words.starts_with(opening),
            "{action} on the person's credential goes past their gate: {pressed}"
        );
    }
    let kept: String = rusqlite::Connection::open(&bookmarks)
        .unwrap()
        .query_row("SELECT value FROM kept", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        kept, "the person's bookmarks",
        "the database adopted is only read"
    );
}
