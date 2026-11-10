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
//! Since round 54d (DLW build ruling 2026-11-09, F1), an act also stands where it writes: a root grant does not cover a Work tree, so a
//! place inside a Work project, however it is spelled, names that project, and a Source or a move's journal target there needs that
//! project's grant as well as the one the act stands on.
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
/// The root's registrar: a human grant for `scope-register` alone, at the root, so a world can declare a folder scope and a project
/// outside the ground before the person's acts are probed there (round 54b item 3).
const OWNER: &str = "file-map-moves-owner-controlled-test-not-a-live-secret";

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
    ctrl_with(world, action, input, token, &[])
}

/// [`ctrl`] with `variables` in the process's environment as well.
fn ctrl_with(
    world: &World,
    action: &str,
    input: &Value,
    token: Option<&str>,
    variables: &[(&str, &str)],
) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ctrl"));
    command.envs(variables.iter().copied());
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

/// [`world`] whose person's grant names `actions` at `scope` (the agent's names them at the root, and the registrar's names only
/// `scope-register` there), with a project `alpha` in Work.
fn granting(actions: &[&str], scope: &str) -> World {
    let world = World(
        std::env::temp_dir().join(format!("central-file-map-moves-{}", uuid::Uuid::new_v4())),
    );
    fs::create_dir_all(world.0.join("home")).unwrap();
    let root = world.root();
    central_ctrl::initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/note.md"), "the person's page\n").unwrap();
    project(&root.join("Work/alpha"), "alpha");
    fs::write(root.join("Work/alpha/plan.md"), "alpha's plan\n").unwrap();
    let grant = |token: &str, kind: &str, scope: &str, actions: &[&str]| {
        json!({"principal_ref":format!("{kind}:file-map-moves-test"),"actor_kind":kind,
            "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":[scope],
            "actions":actions,"expires_at_unix_seconds":u64::MAX})
    };
    let authority = "Control/user/file-map-moves-authority.json";
    fs::write(
        root.join(authority),
        serde_json::to_vec(
            &json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root",
            "grants":[grant(PERSON, "human", scope, actions), grant(AGENT, "agent", "control:root", actions),
                grant(OWNER, "human", "control:root", &["central.file-map.scope-register"])]}),
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

/// A project's manifest at `folder`, whose World is `project:<id>`.
fn project(folder: &Path, id: &str) {
    fs::create_dir_all(folder.join("ProjectCentral/user")).unwrap();
    fs::write(
        folder.join("ProjectCentral/project.json"),
        json!({"schema":"central.project/v1","project_id":id,"human_source":"ProjectCentral/user",
            "wiki":{"profile":"okf-wiki/v1","source":"ProjectCentral/agents/wiki/wiki.json"}})
        .to_string(),
    )
    .unwrap();
}

/// A folder `Music` and a project `far`, both outside the ground, each declared at the root by its registrar ([`OWNER`]): scopes
/// that stand at the root grant whatever an act names (file_map.rs `grant_scope`). No scope is read after `far` is declared, so
/// its state folder (`.central/scopes/far`) is not made yet.
fn declare_outside_scopes(world: &World) {
    let music = world.0.join("Music");
    fs::create_dir_all(&music).unwrap();
    fs::write(music.join("voice.wav"), b"RIFF").unwrap();
    let far = world.0.join("far");
    project(&far, "far");
    fs::write(far.join("far.md"), "far's page\n").unwrap();
    for declared in [
        json!({"name":"Music","path":fs::canonicalize(&music).unwrap(),"kind":"folder","allow_external":true,"media":["wav"]}),
        json!({"name":"far","path":fs::canonicalize(&far).unwrap(),"kind":"project","allow_external":true}),
    ] {
        let mut declared = declared;
        declared["expected_revision"] = revision(world, None).into();
        let registered = ctrl(
            world,
            "central.file-map.scope-register",
            &declared,
            Some(OWNER),
        );
        assert_eq!(registered["ok"], true, "{registered}");
    }
}

/// What a refusal of each of the person's acts on the ground opens with (file_map.rs `PERSON_ACTS`).
fn opening(action: &str) -> &'static str {
    match action {
        "central.file-map.register" => "Registering a Source is the person's own press: ",
        "central.file-map.link" => "Linking a Source into a folder is the person's own press: ",
        "central.file-map.pool" => "Making a scope's whole tree Source is the person's own press: ",
        "central.file-map.adopt-db" => "Adopting a bookmark database is the person's own press: ",
        "central.file-map.record-adopt" => "Adopting a bookmark is the person's own press: ",
        other => panic!("{other} is not one of the acts probed here"),
    }
}

/// An input for one of the person's acts on the ground (no move, no scope's register), at `project` or at the root.
fn act_input(
    action: &str,
    project: Option<&str>,
    revision: &str,
    source: &str,
    bookmarks: &Path,
) -> Value {
    let mut input = match action {
        "central.file-map.register" => json!({"path":"plan.md"}),
        "central.file-map.link" => {
            json!({"source_ref":source,"path":"links/note","owner":"person"})
        }
        "central.file-map.pool" => json!({"enable":false}),
        "central.file-map.adopt-db" => json!({"database":bookmarks,"quiesced":true}),
        "central.file-map.record-adopt" => {
            json!({"source_ref":source,"record_id":1,"record_revision":"r"})
        }
        other => panic!("{other} is not one of the acts probed here"),
    };
    input["expected_revision"] = revision.into();
    if let Some(project) = project {
        input["project"] = project.into();
    }
    input
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
    refused_as(
        world,
        action,
        input,
        token,
        "central.file_map_denied",
        opening,
        cause,
    );
}

/// [`refused_in_words`] with the refusal's `code`: a conflict or a Source that does not read is no denial.
fn refused_as(
    world: &World,
    action: &str,
    input: &Value,
    token: Option<&str>,
    code: &str,
    opening: &str,
    cause: &str,
) {
    let refused = ctrl(world, action, input, token);
    let case = format!("{action} with {}", token.unwrap_or("no credential"));
    assert_eq!(refused["ok"], false, "{case}: {refused}");
    assert_eq!(refused["error"]["code"], code, "{case}: {refused}");
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

/// [`ground`] without the lock files an act that runs takes where it stands (`file-map.lock` and `source-mutation.lock` in a
/// scope's `.central`): empty files, made the first time an act gets past the gate and kept from then on. This is what an act
/// refused past the gate, under its locks, must leave as it was; an act refused at the gate takes no lock, and is held to
/// [`ground`] itself.
fn ground_but_locks(world: &World) -> Vec<(PathBuf, Vec<u8>)> {
    ground(world)
        .into_iter()
        .filter(|(path, bytes)| {
            !(bytes.is_empty()
                && [".central/file-map.lock", ".central/source-mutation.lock"]
                    .iter()
                    .any(|lock| path.ends_with(lock)))
        })
        .collect()
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

/// The ground spelled through macOS's firmlink (`/System/Volumes/Data` and the ground's canonical path) where that spelling is the
/// ground's very folder and stays a spelling of its own when made canonical; None elsewhere, as on Linux.
fn firmlink(root: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let canonical = fs::canonicalize(root).ok()?;
    let firm = Path::new("/System/Volumes/Data").join(canonical.strip_prefix("/").ok()?);
    let (held, spelled) = (fs::metadata(&canonical).ok()?, fs::metadata(&firm).ok()?);
    (held.dev() == spelled.dev()
        && held.ino() == spelled.ino()
        && fs::canonicalize(&firm).ok()? != canonical)
        .then_some(firm)
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

/// Round 54b item 1 (DLW build ruling 2026-11-09; review B of #339, finding 1): the gate asks for the credential before it reads any
/// scope. Here a project's manifest is a folder, so no scope of this ground can be built; an act that names a project is still refused
/// for its credential, in those words, and nothing is written.
#[test]
fn the_gate_asks_for_the_credential_before_it_reads_any_scope() {
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    fs::create_dir_all(root.join("Work/broken/ProjectCentral/project.json")).unwrap();
    let bookmarks = world.0.join("bookmarks.db");
    let before = ground(&world);
    for action in [
        "central.file-map.register",
        "central.file-map.link",
        "central.file-map.pool",
        "central.file-map.adopt-db",
        "central.file-map.record-adopt",
    ] {
        let input = act_input(action, Some("alpha"), "unread", "unread", &bookmarks);
        refused_without_the_person(&world, action, &input, opening(action));
    }
    assert_eq!(ground(&world), before, "a refused act writes nothing");
}

/// The same item: where an act stands is read without making anything. A project outside the ground keeps its state in
/// `.central/scopes/<name>`, which the map's first read of it makes; an act refused at the gate never does, whatever credential it
/// carried, the person's included where their grant does not stand.
#[test]
fn an_act_refused_at_the_gate_makes_no_scope_folder() {
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    declare_outside_scopes(&world);
    let state = root.join(".central/scopes/far");
    assert!(!state.exists(), "no read of far has been made yet");
    let bookmarks = world.0.join("bookmarks.db");
    let before = ground(&world);
    for action in [
        "central.file-map.register",
        "central.file-map.link",
        "central.file-map.pool",
    ] {
        let input = act_input(action, Some("far"), "unread", "unread", &bookmarks);
        refused_without_the_person(&world, action, &input, opening(action));
        refused_in_words(
            &world,
            action,
            &input,
            Some(PERSON),
            opening(action),
            "lacks this exact scope/action",
        );
    }
    assert!(!state.exists(), "a refused act makes no scope folder");
    assert_eq!(ground(&world), before, "a refused act writes nothing");
    revision(&world, Some("far"));
    assert!(state.is_dir(), "the map's first read of far makes it");
}

/// Round 54b item 2 (review B of #339, finding 2): a scope's register stands at the root, where its gate and its write stand, and it
/// locks there: a `project` it is given chooses none of its locks. Each lock Central takes is a file in the `.central` folder of the
/// scope it serialises.
#[test]
fn a_scopes_register_locks_at_the_root_whatever_project_it_names() {
    let world = granting(&GROUND_ACTS, "control:root");
    let root = world.root();
    let music = world.0.join("Music");
    fs::create_dir_all(&music).unwrap();
    fs::write(music.join("voice.wav"), b"RIFF").unwrap();
    let alpha = root.join("Work/alpha/.central");
    assert!(!alpha.exists());
    let scoped = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"project":"alpha","name":"Music","path":fs::canonicalize(&music).unwrap(),"kind":"folder",
            "allow_external":true,"media":["wav"],"expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(scoped["ok"], true, "{scoped}");
    assert_eq!(scoped["data"]["result"]["world_ref"], "folder:Music");
    assert!(
        !alpha.exists(),
        "alpha holds no lock of the root's register"
    );
    assert!(root.join(".central/source-mutation.lock").is_file());
}

/// Round 54b item 3 (review B of #339, finding 3): the person's acts reach every scope rule. The person's grant stands at one of two
/// projects of Work (alpha, not beta). Their register, link, pool, adopt-db and record-adopt at alpha go past the gate, by its name or
/// its World. At beta they are refused, and at the root (no project named). A project spelled with `..` or as a path, a folder scope
/// and a project outside the ground all stand at the root, and are refused there. A grant at the root is refused at alpha's acts.
/// Nothing refused is written.
#[test]
fn the_persons_acts_reach_every_scope_rule() {
    let acts: Vec<&str> = MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect();
    let probed = [
        "central.file-map.register",
        "central.file-map.link",
        "central.file-map.pool",
        "central.file-map.adopt-db",
        "central.file-map.record-adopt",
    ];
    let lacks = "lacks this exact scope/action";
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    project(&root.join("Work/beta"), "beta");
    fs::write(root.join("Work/beta/plan.md"), "beta's plan\n").unwrap();
    declare_outside_scopes(&world);
    // The map's first read of far makes its state folder; the refusals below are asked of a ground that has it.
    revision(&world, Some("far"));
    let bookmarks = world.0.join("bookmarks.db");
    rusqlite::Connection::open(&bookmarks)
        .unwrap()
        .execute_batch("CREATE TABLE kept(value TEXT);")
        .unwrap();
    let source = page_source(&world);
    let before = ground(&world);
    for named in [
        Some("beta"),
        Some("project:beta"),
        None,
        Some("../alpha"),
        Some("Work/alpha"),
        Some("alpha/../alpha"),
        Some("folder:Music"),
        Some("Music"),
        Some("far"),
        Some("project:far"),
    ] {
        for action in probed {
            let input = act_input(action, named, "unread", &source, &bookmarks);
            refused_in_words(&world, action, &input, Some(PERSON), opening(action), lacks);
        }
    }
    assert_eq!(ground(&world), before, "nothing refused is written");
    // At alpha, by its name or its World, each goes past the gate to what it does.
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &act_input(
            "central.file-map.register",
            Some("alpha"),
            &revision(&world, Some("alpha")),
            &source,
            &bookmarks,
        ),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let located = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"path":"Work/alpha/plan.md"}),
        None,
    );
    assert_eq!(located["ok"], true, "{located}");
    let plan = located["data"]["result"]["source"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let linked = ctrl(
        &world,
        "central.file-map.link",
        &act_input(
            "central.file-map.link",
            Some("project:alpha"),
            &revision(&world, Some("alpha")),
            &plan,
            &bookmarks,
        ),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    assert!(fs::symlink_metadata(root.join("Work/alpha/links/note"))
        .unwrap()
        .file_type()
        .is_symlink());
    let pooled = ctrl(
        &world,
        "central.file-map.pool",
        &act_input(
            "central.file-map.pool",
            Some("alpha"),
            &revision(&world, Some("alpha")),
            &plan,
            &bookmarks,
        ),
        Some(PERSON),
    );
    assert_eq!(pooled["ok"], true, "{pooled}");
    for action in ["central.file-map.adopt-db", "central.file-map.record-adopt"] {
        let pressed = ctrl(
            &world,
            action,
            &act_input(
                action,
                Some("alpha"),
                &revision(&world, Some("alpha")),
                &plan,
                &bookmarks,
            ),
            Some(PERSON),
        );
        let words = pressed["error"]["message"].as_str().unwrap_or_default();
        assert!(
            pressed["ok"] == true
                || pressed["error"]["code"] != "central.file_map_denied"
                    && !words.starts_with(opening(action)),
            "{action} at alpha goes past the gate: {pressed}"
        );
    }
    // A grant at the root is refused at alpha's acts.
    let world = granting(&acts, "control:root");
    let source = page_source(&world);
    let before = ground(&world);
    for named in [Some("alpha"), Some("project:alpha")] {
        for action in probed {
            let input = act_input(action, named, "unread", &source, &bookmarks);
            refused_in_words(&world, action, &input, Some(PERSON), opening(action), lacks);
        }
    }
    assert_eq!(ground(&world), before, "nothing is written at the project");
}

/// One grant of the authority Source: `token`'s digest, its actor, the scopes it stands at, its acts and when it expires.
fn grant(token: &str, kind: &str, scopes: &[&str], actions: &[&str], expires: u64) -> Value {
    json!({"principal_ref":format!("{kind}:file-map-moves-test"),"actor_kind":kind,
        "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":scopes,
        "actions":actions,"expires_at_unix_seconds":expires})
}

/// The world's authority Source rewritten to hold `grants` (its relation is unchanged, so it stays the Source the gate reads).
fn authority(world: &World, grants: Vec<Value>) {
    fs::write(
        world.root().join("Control/user/file-map-moves-authority.json"),
        serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants}))
            .unwrap(),
    )
    .unwrap();
}

/// Every one of the person's acts.
fn every_act() -> Vec<&'static str> {
    MOVES.iter().chain(GROUND_ACTS.iter()).copied().collect()
}

/// Round 54d F1 (review of #340): an act stands where it writes, not only where it names. A root grant does not cover a Work tree,
/// so with the person's grant at the root alone, a link made inside alpha's tree and a register of alpha's file are refused in words
/// that say both: where the act names (no project, so the root) and the Work project its path is in. The path is resolved on the
/// disk first, so no spelling of it reaches alpha unseen: `..`, `./`, a trailing slash, an absolute path, a link in Work to alpha,
/// `..` after a link in the root into alpha (round 54g item 8), on a disk that folds case another case, and on macOS the firmlink
/// spelling of the ground (round 54g item 7). Nothing refused is written.
#[test]
fn a_root_grant_writes_into_no_work_project_by_any_spelling_of_the_path() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    std::os::unix::fs::symlink(root.join("Work/alpha"), root.join("Work/alias")).unwrap();
    // `Control/user/door/..` is alpha's own folder on the disk, though `Control/user` as written.
    fs::create_dir_all(root.join("Work/alpha/inner")).unwrap();
    std::os::unix::fs::symlink(
        root.join("Work/alpha/inner"),
        root.join("Control/user/door"),
    )
    .unwrap();
    let source = page_source(&world);
    let plan = fs::canonicalize(root.join("Work/alpha/plan.md")).unwrap();
    let before = ground(&world);
    let link = |path: &str| json!({"source_ref":source,"path":path,"owner":"person"});
    let register = |path: &str| json!({"path":path});
    let mut probes = vec![
        ("central.file-map.link", link("Work/alpha/links/note")),
        (
            "central.file-map.link",
            link("Control/../Work/alpha/links/note"),
        ),
        ("central.file-map.link", link("./Work/alpha/links/note")),
        ("central.file-map.link", link("Work/alias/links/note")),
        ("central.file-map.link", link("Work/alpha/links/note/")),
        ("central.file-map.register", register("Work/alpha/plan.md")),
        (
            "central.file-map.register",
            register("Control/user/../../Work/alpha/plan.md"),
        ),
        ("central.file-map.register", register("Work/alias/plan.md")),
        ("central.file-map.register", register("Work/alpha/")),
        (
            "central.file-map.register",
            json!({"path":plan,"allow_external":true}),
        ),
        (
            "central.file-map.link",
            link("Control/user/door/../links/note"),
        ),
        (
            "central.file-map.register",
            register("Control/user/door/../plan.md"),
        ),
    ];
    if let Some(firm) = firmlink(&root) {
        probes.push((
            "central.file-map.register",
            json!({"path":firm.join("Work/alpha/plan.md"),"allow_external":true}),
        ));
    }
    // A disk that folds case reads `work/ALPHA` as alpha's own folder; on one that does not, it is no folder at all.
    let folds = root.join("work/ALPHA").exists();
    if folds {
        probes.push(("central.file-map.link", link("work/ALPHA/links/note")));
        probes.push(("central.file-map.register", register("work/ALPHA/plan.md")));
    }
    for (action, mut input) in probes {
        input["expected_revision"] = revision(&world, None).into();
        refused_in_words(
            &world,
            action,
            &input,
            Some(PERSON),
            opening(action),
            "is in the Work project alpha, but the act names no project, so it stands at the root: an act on alpha's tree names project alpha and stands on its grant",
        );
    }
    assert_eq!(ground(&world), before, "nothing refused is written");
}

/// The same finding: the same acts on the project's grant pass, and a Source or a move that reaches into a Work project needs that
/// project's grant as well as the root's. A link at the root of alpha's Source, a move of the root's page into alpha, and the move's
/// apply, each refused on a grant at the root alone and passing on a grant at the root and alpha. Alpha's own register and link pass
/// on alpha's grant alone.
#[test]
fn an_act_that_reaches_a_work_project_stands_on_that_projects_grant() {
    let acts = every_act();
    let lacks = "lacks this exact scope/action";
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    let bookmarks = world.0.join("bookmarks.db");
    // Alpha's own acts, on alpha's grant: its register, then its link.
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &act_input(
            "central.file-map.register",
            Some("alpha"),
            &revision(&world, Some("alpha")),
            "",
            &bookmarks,
        ),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let located = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"path":"Work/alpha/plan.md"}),
        None,
    );
    assert_eq!(located["ok"], true, "{located}");
    let alpha_plan = located["data"]["result"]["source"]["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let linked = ctrl(
        &world,
        "central.file-map.link",
        &act_input(
            "central.file-map.link",
            Some("alpha"),
            &revision(&world, Some("alpha")),
            &alpha_plan,
            &bookmarks,
        ),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    // The root's acts that reach alpha, on the root's grant alone: refused, and nothing written.
    authority(
        &world,
        vec![grant(PERSON, "human", &["control:root"], &acts, u64::MAX)],
    );
    let source = page_source(&world);
    let mut at_root = json!({"source_ref":alpha_plan,"path":"links/alpha-plan","owner":"person","expected_revision":revision(&world, None)});
    let into_alpha = json!({"source_ref":source,"destination":"Work/alpha/note.md"});
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.link",
        &at_root,
        Some(PERSON),
        opening("central.file-map.link"),
        lacks,
    );
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &into_alpha,
        Some(PERSON),
        "Planning a move is the person's own press: ",
        lacks,
    );
    assert_eq!(ground(&world), before, "nothing refused is written");
    assert_eq!(journals(&root), 0);
    // On the root's grant and alpha's, they pass.
    authority(
        &world,
        vec![grant(
            PERSON,
            "human",
            &["control:root", "project:alpha"],
            &acts,
            u64::MAX,
        )],
    );
    at_root["expected_revision"] = revision(&world, None).into();
    let linked = ctrl(&world, "central.file-map.link", &at_root, Some(PERSON));
    assert_eq!(linked["ok"], true, "{linked}");
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &into_alpha,
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    // The journal's move into alpha is carried out on the same grants: the root's alone is refused.
    authority(
        &world,
        vec![grant(PERSON, "human", &["control:root"], &acts, u64::MAX)],
    );
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
        "Moving a file is the person's own press: ",
        lacks,
    );
    assert_eq!(ground(&world), before, "a refused apply moves nothing");
    authority(
        &world,
        vec![grant(
            PERSON,
            "human",
            &["control:root", "project:alpha"],
            &acts,
            u64::MAX,
        )],
    );
    let applied = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert!(root.join("Work/alpha/note.md").is_file());
}

/// The same finding: where the project an act names and the place it writes disagree, it is refused in words saying both, on a
/// grant that holds both projects. Nothing refused is written.
#[test]
fn a_project_and_a_path_that_disagree_are_refused_saying_both() {
    let acts = every_act();
    let world = granting(&acts, "project:alpha");
    let root = world.root();
    project(&root.join("Work/beta"), "beta");
    fs::write(root.join("Work/beta/plan.md"), "beta's plan\n").unwrap();
    authority(
        &world,
        vec![grant(
            PERSON,
            "human",
            &["project:alpha", "project:beta"],
            &acts,
            u64::MAX,
        )],
    );
    let source = page_source(&world);
    let beta_plan = fs::canonicalize(root.join("Work/beta/plan.md")).unwrap();
    let before = ground(&world);
    for (action, named, input, reached) in [
        (
            "central.file-map.register",
            "alpha",
            json!({"path":beta_plan,"allow_external":true}),
            "beta",
        ),
        (
            "central.file-map.register",
            "alpha",
            json!({"path":"../beta/plan.md"}),
            "beta",
        ),
        (
            "central.file-map.link",
            "alpha",
            json!({"source_ref":source,"path":"../beta/links/note","owner":"person"}),
            "beta",
        ),
        (
            "central.file-map.link",
            "beta",
            json!({"source_ref":source,"path":"../alpha/links/note","owner":"person"}),
            "alpha",
        ),
    ] {
        let mut input = input;
        input["project"] = named.into();
        input["expected_revision"] = revision(&world, Some(named)).into();
        refused_in_words(
            &world,
            action,
            &input,
            Some(PERSON),
            opening(action),
            &format!("is in the Work project {reached}, but the act names project {named}: an act on {reached}'s tree names project {reached}"),
        );
    }
    assert_eq!(ground(&world), before, "nothing refused is written");
}

/// Round 54d F3 (review of #340): what the reviewer probed by hand. A grant that has expired, a grant of a native service's, and two
/// grants that share one token's digest are each refused, and so is every other spelling of a project the grant stands at: its
/// absolute path, another case, a trailing slash or space, a combining mark, and a link in Work to it. Nothing refused is written.
#[test]
fn an_expired_a_services_and_a_shared_grant_and_every_other_spelling_are_refused() {
    let acts = every_act();
    let world = granting(&acts, "control:root");
    let root = world.root();
    std::os::unix::fs::symlink(root.join("Work/alpha"), root.join("Work/alias")).unwrap();
    let register =
        json!({"path":"Control/user/note.md","expected_revision":revision(&world, None)});
    let opening = opening("central.file-map.register");
    for (grants, cause) in [
        (
            vec![grant(PERSON, "human", &["control:root"], &acts, 1)],
            "native credential is expired or lacks this exact scope/action",
        ),
        (
            vec![grant(
                PERSON,
                "native-service",
                &["control:root"],
                &acts,
                u64::MAX,
            )],
            "requires a human-scoped native credential",
        ),
        (
            vec![
                grant(PERSON, "human", &["control:root"], &acts, u64::MAX),
                grant(PERSON, "human", &["project:alpha"], &acts, u64::MAX),
            ],
            "ambiguous credential grants require source reconciliation",
        ),
    ] {
        authority(&world, grants);
        let before = ground(&world);
        refused_in_words(
            &world,
            "central.file-map.register",
            &register,
            Some(PERSON),
            opening,
            cause,
        );
        assert_eq!(ground(&world), before, "nothing refused is written");
    }
    authority(
        &world,
        vec![grant(PERSON, "human", &["project:alpha"], &acts, u64::MAX)],
    );
    let before = ground(&world);
    let absolute = root.join("Work/alpha");
    let canonical = fs::canonicalize(&absolute).unwrap();
    for spelled in [
        absolute.to_str().unwrap(),
        canonical.to_str().unwrap(),
        "ALPHA",
        "Alpha",
        "alpha/",
        "alpha ",
        " alpha",
        "alias",
        "project:Alpha",
    ] {
        let input = json!({"project":spelled,"path":"plan.md","expected_revision":"unread"});
        refused_in_words(
            &world,
            "central.file-map.register",
            &input,
            Some(PERSON),
            opening,
            "lacks this exact scope/action",
        );
    }
    assert_eq!(ground(&world), before, "nothing refused is written");
    // A real NFD spelling of a project's name is another name (round 54g item 8): `Café`, its folder made in NFC, asked for as `Cafe`
    // and a combining acute accent is refused; asked for as it was made, it is selected.
    let (nfc, nfd) = ("Caf\u{e9}", "Cafe\u{301}");
    project(&root.join("Work").join(nfc), "cafe");
    fs::write(
        root.join("Work").join(nfc).join("plan.md"),
        "the café's plan\n",
    )
    .unwrap();
    authority(
        &world,
        vec![grant(PERSON, "human", &["project:cafe"], &acts, u64::MAX)],
    );
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.register",
        &json!({"project":nfd,"path":"plan.md","expected_revision":"unread"}),
        Some(PERSON),
        opening,
        "lacks this exact scope/action",
    );
    assert_eq!(ground(&world), before, "nothing refused is written");
    let selected = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":nfc,"path":"plan.md","expected_revision":revision(&world, Some(nfc))}),
        Some(PERSON),
    );
    assert_eq!(selected["ok"], true, "{selected}");
}

/// Round 54d F5 (review of #340): an authority Source that does not read is refused in Central's own words, never the parser's,
/// which would echo a value of the Source to a caller with no grant; and the token is matched before the Source's revision, so a
/// caller without a valid token learns nothing of it.
#[test]
fn the_authority_source_says_nothing_of_itself_to_a_caller_without_a_grant() {
    let world = granting(&every_act(), "control:root");
    let register = json!({"path":"Control/user/note.md","expected_revision":revision(&world, None),
        "expected_authority_revision":"a-revision-the-source-does-not-have"});
    let opening = opening("central.file-map.register");
    refused_in_words(
        &world,
        "central.file-map.register",
        &register,
        Some(UNKNOWN),
        opening,
        "native credential is not authorised",
    );
    refused_as(
        &world,
        "central.file-map.register",
        &register,
        Some(PERSON),
        "central.file_map_conflict",
        opening,
        "native authority revision changed",
    );
    fs::write(
        world.root().join("Control/user/file-map-moves-authority.json"),
        br#"{"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{"principal_ref":"human:x","actor_kind":"human","token_sha256":"00","scope_refs":["control:root"],"actions":[],"expires_at_unix_seconds":"a-value-of-the-source-canary"}]}"#,
    )
    .unwrap();
    let plain = json!({"path":"Control/user/note.md","expected_revision":revision(&world, None)});
    for token in [UNKNOWN, PERSON] {
        let refused = ctrl(&world, "central.file-map.register", &plain, Some(token));
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert_eq!(refused["ok"], false, "{refused}");
        assert!(words.starts_with(opening), "{words}");
        assert!(
            words.contains("invalid native-action-authority source"),
            "{words}"
        );
        assert!(
            !words.contains("canary") && !words.contains("invalid type"),
            "the parser's words: {words}"
        );
    }
}

/// Round 54d F9 (review of #340): a project scope nested inside a Work project, registered at the root, stands on the root's grant, as
/// the root registered it, and so does an act in its own tree. A place outside its tree, in the Work project around it, is that
/// project's.
#[test]
fn a_project_nested_in_a_work_project_stands_on_the_roots_grant_in_its_own_tree_alone() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work/alpha/sub"), "sub");
    fs::write(root.join("Work/alpha/sub/sub.md"), "sub's page\n").unwrap();
    let declared = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"sub","path":"Work/alpha/sub","kind":"project","expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(declared["ok"], true, "{declared}");
    let own = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":"sub","path":"sub.md","expected_revision":revision(&world, Some("sub"))}),
        Some(PERSON),
    );
    assert_eq!(own["ok"], true, "{own}");
    let around = json!({"project":"sub","path":"../plan.md","expected_revision":revision(&world, Some("sub"))});
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.register",
        &around,
        Some(PERSON),
        opening("central.file-map.register"),
        "is in the Work project alpha, but the act names project sub: an act on alpha's tree names project alpha",
    );
    assert_eq!(ground(&world), before, "nothing refused is written");
}

/// A folder `Control/user/pdir` holding a project scope `proj` the root registered, registered itself as the root's Source, and its
/// move to `destination` planned and applied on the person's grant, the apply interrupted after its rename (round 54f item 1, the
/// review of #343's P10). `proj` is then declared where it no longer stands, so the live scope map does not read until the move is
/// finished or undone. Answers the move's carried input.
fn interrupted_move(world: &World, destination: &str) -> Value {
    let root = world.root();
    project(&root.join("Control/user/pdir/proj"), "proj");
    fs::write(root.join("Control/user/pdir/proj/p.md"), "proj's page\n").unwrap();
    let declared = ctrl(
        world,
        "central.file-map.scope-register",
        &json!({"name":"proj","path":"Control/user/pdir/proj","kind":"project","expected_revision":revision(world, None)}),
        Some(OWNER),
    );
    assert_eq!(declared["ok"], true, "{declared}");
    let registered = ctrl(
        world,
        "central.file-map.register",
        &json!({"path":"Control/user/pdir","expected_revision":revision(world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let planned = ctrl(
        world,
        "central.file-map.move-plan",
        &json!({"source_ref":registered["data"]["result"]["source_ref"],"destination":destination}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    let interrupted = ctrl_with(
        world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
        &[("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME", "1")],
    );
    assert_eq!(interrupted["ok"], false, "{interrupted}");
    assert!(!root.join("Control/user/pdir").exists());
    assert!(root.join(destination).join("proj/p.md").is_file());
    let stale = ctrl(
        world,
        "central.file-map.inspect",
        &json!({"resources":false}),
        None,
    );
    assert_eq!(
        stale["ok"], false,
        "the live map does not read mid-move: {stale}"
    );
    carried
}

/// Round 54f item 1 (review of #343, finding 1): an interrupted move is finished through the person's gate. Its apply stands on the
/// scopes its journal runs from, not the live map, which does not read until the move is done.
#[test]
fn an_interrupted_move_whose_scope_is_stale_is_replayed_through_the_gate() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let carried = interrupted_move(&world, "Control/user/pdir2");
    let replayed = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(replayed["ok"], true, "{replayed}");
    assert!(!root.join("Control/user/pdir").exists());
    assert!(root.join("Control/user/pdir2/proj/p.md").is_file());
    assert_eq!(
        ctrl(
            &world,
            "central.file-map.inspect",
            &json!({"project":"proj","resources":false}),
            None
        )["ok"],
        true,
        "the live map reads again, with proj where it now stands"
    );
}

/// The same finding: an interrupted move is undone through the person's gate, on the scopes its journal runs from.
#[test]
fn an_interrupted_move_whose_scope_is_stale_is_rolled_back_through_the_gate() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let carried = interrupted_move(&world, "Control/user/pdir2");
    let undone = ctrl(
        &world,
        "central.file-map.move-rollback",
        &carried,
        Some(PERSON),
    );
    assert_eq!(undone["ok"], true, "{undone}");
    assert!(root.join("Control/user/pdir/proj/p.md").is_file());
    assert!(!root.join("Control/user/pdir2").exists());
    assert_eq!(
        ctrl(
            &world,
            "central.file-map.inspect",
            &json!({"project":"proj","resources":false}),
            None
        )["ok"],
        true,
        "the live map reads again, with proj back where it stood"
    );
}

/// The same finding: an interrupted move into alpha, finished or undone on a grant that no longer holds alpha, is refused in words
/// that name alpha, the journal's project the grant is missing at, never the live map's failure to read. Nothing is moved; on the
/// root's grant and alpha's it is finished.
#[test]
fn an_interrupted_move_refused_where_its_grant_is_missing_names_the_journals_project() {
    let acts = every_act();
    let world = granting(&acts, "control:root");
    authority(
        &world,
        vec![
            grant(
                PERSON,
                "human",
                &["control:root", "project:alpha"],
                &acts,
                u64::MAX,
            ),
            grant(
                OWNER,
                "human",
                &["control:root"],
                &["central.file-map.scope-register"],
                u64::MAX,
            ),
        ],
    );
    let root = world.root();
    let carried = interrupted_move(&world, "Work/alpha/pdir");
    authority(
        &world,
        vec![
            grant(PERSON, "human", &["control:root"], &acts, u64::MAX),
            grant(
                OWNER,
                "human",
                &["control:root"],
                &["central.file-map.scope-register"],
                u64::MAX,
            ),
        ],
    );
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            "it reaches the Work project alpha, whose grant does not hold it: native credential is expired or lacks this exact scope/action",
        );
        let words = ctrl(&world, action, &carried, Some(PERSON))["error"]["message"].to_string();
        assert!(!words.contains("os error"), "{action}: {words}");
    }
    assert_eq!(
        ground(&world),
        before,
        "a refused replay or rollback moves nothing"
    );
    authority(
        &world,
        vec![grant(
            PERSON,
            "human",
            &["control:root", "project:alpha"],
            &acts,
            u64::MAX,
        )],
    );
    let replayed = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(replayed["ok"], true, "{replayed}");
    assert!(root.join("Work/alpha/pdir/proj/p.md").is_file());
}

/// The same item: a move's apply and rollback read the scopes its journal runs from without making anything. A project outside the
/// ground in the journal keeps its state in `.central/scopes/<name>`; a replay or a rollback refused where the grant does not stand
/// never makes it.
#[test]
fn a_move_refused_at_the_gate_makes_no_scope_folder_of_its_journal() {
    let acts = every_act();
    let world = granting(&acts, "control:root");
    let root = world.root();
    declare_outside_scopes(&world);
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(&world),"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    let state = root.join(".central/scopes/far");
    fs::remove_dir_all(&state).unwrap();
    authority(
        &world,
        vec![grant(PERSON, "human", &["project:alpha"], &acts, u64::MAX)],
    );
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            "lacks this exact scope/action",
        );
    }
    assert!(
        !state.exists(),
        "a refused replay or rollback makes no scope folder"
    );
    assert_eq!(
        ground(&world),
        before,
        "a refused replay or rollback writes nothing"
    );
}

/// The root's move of `Control/user/note.md` to `Control/user/note2.md`, planned on the person's grant, its journal then edited by
/// `edit` as any agent that can write a file in the ground could edit it (round 54f.2, the re-check of #343's N1). Answers the move's
/// carried input.
fn edited_journal(world: &World, edit: impl FnOnce(&mut Value)) -> Value {
    let planned = ctrl(
        world,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(world),"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    let journal = world.root().join(format!(".central/bkmr/moves/{id}.json"));
    let mut plan: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    edit(&mut plan);
    fs::write(&journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    json!({"plan_id":id,"quiesced":true})
}

/// Round 54f.2 (R1): a move stands at the root, the journal's own root scope, wherever the journal lists it. A journal whose scopes
/// are reordered so alpha comes first is refused on a grant at alpha alone, and nothing moves; on the root's grant it is carried out
/// there.
#[test]
fn a_journal_whose_scopes_are_reordered_still_stands_at_the_root() {
    let acts = every_act();
    let world = granting(&acts, "control:root");
    let root = world.root();
    let carried = edited_journal(&world, |plan| {
        plan["worlds"].as_array_mut().unwrap().reverse();
        assert_eq!(plan["worlds"][0]["world"], "project:alpha");
    });
    authority(
        &world,
        vec![grant(PERSON, "human", &["project:alpha"], &acts, u64::MAX)],
    );
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            "lacks this exact scope/action",
        );
    }
    assert_eq!(
        ground(&world),
        before,
        "a grant at alpha alone moves nothing at the root"
    );
    authority(
        &world,
        vec![grant(PERSON, "human", &["control:root"], &acts, u64::MAX)],
    );
    let applied = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert!(root.join("Control/user/note2.md").is_file());
}

/// Round 54f.2 (R2): the Work project a move reaches is found on the disk, and its journal is read where the act reads it, at the
/// ground. A journal that sends the file into alpha and leaves alpha out of its scopes, or lists alpha first, is refused on the
/// root's grant alone, in words that name alpha, and nothing moves.
#[test]
fn a_journal_that_hides_the_project_its_move_goes_into_is_refused_naming_it() {
    for edit in ["left out", "listed first"] {
        let acts = every_act();
        let world = granting(&acts, "control:root");
        let root = world.root();
        let carried = edited_journal(&world, |plan| {
            let scopes = plan["worlds"].as_array_mut().unwrap();
            match edit {
                "left out" => scopes.retain(|scope| scope["world"] == "control:root"),
                _ => scopes.reverse(),
            }
            plan["to"] = json!("Work/alpha/note2.md");
        });
        let before = ground(&world);
        for (action, opening) in [
            (
                "central.file-map.move-apply",
                "Moving a file is the person's own press: ",
            ),
            (
                "central.file-map.move-rollback",
                "Moving a file back is the person's own press: ",
            ),
        ] {
            refused_in_words(
                &world,
                action,
                &carried,
                Some(PERSON),
                opening,
                "it reaches the Work project alpha, whose grant does not hold it",
            );
        }
        assert_eq!(
            ground(&world),
            before,
            "alpha {edit}: the root's grant alone writes nothing into alpha"
        );
        assert!(!root.join("Work/alpha/note2.md").exists(), "alpha {edit}");
    }
}

/// Round 54g item 12 (the re-check of #343, N2): a Work project a move reaches whose declaration does not read is refused in words
/// that name it, not in a bare OS error alone. The journal sends the file into alpha and leaves alpha out of its scopes, so the gate
/// finds alpha on the disk; alpha's `project.json` is then a link to itself, or (where this process cannot read past it) a file
/// with no permissions. Each is refused with its own error's code, a failure and a denial. Nothing moves.
#[test]
fn a_work_project_whose_declaration_does_not_read_is_named_in_the_refusal() {
    use std::os::unix::fs::PermissionsExt;
    for (broken, code) in [
        ("a link to itself", "central.file_map_failure"),
        ("no permissions", "central.file_map_denied"),
    ] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        let carried = edited_journal(&world, |plan| {
            plan["worlds"]
                .as_array_mut()
                .unwrap()
                .retain(|scope| scope["world"] == "control:root");
            plan["to"] = json!("Work/alpha/note2.md");
        });
        let before = ground(&world);
        let declaration = root.join("Work/alpha/ProjectCentral/project.json");
        let kept = fs::read(&declaration).unwrap();
        match broken {
            "a link to itself" => {
                fs::remove_file(&declaration).unwrap();
                std::os::unix::fs::symlink("project.json", &declaration).unwrap();
            }
            _ => {
                fs::set_permissions(&declaration, fs::Permissions::from_mode(0o000)).unwrap();
                if fs::read(&declaration).is_ok() {
                    eprintln!("skipped {broken}: this process reads a file with no permissions (a superuser)");
                    continue;
                }
            }
        }
        for (action, opening) in [
            (
                "central.file-map.move-apply",
                "Moving a file is the person's own press: ",
            ),
            (
                "central.file-map.move-rollback",
                "Moving a file back is the person's own press: ",
            ),
        ] {
            refused_as(
                &world,
                action,
                &carried,
                Some(PERSON),
                code,
                opening,
                "it reaches the Work project alpha, which does not read: ",
            );
        }
        if broken == "a link to itself" {
            fs::remove_file(&declaration).unwrap();
        } else {
            fs::set_permissions(&declaration, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(&declaration, kept).unwrap();
        assert_eq!(ground(&world), before, "{broken}: nothing is written");
        assert!(!root.join("Work/alpha/note2.md").exists(), "{broken}");
    }
}

/// Round 54g item 12, for a project the journal itself runs from: a move planned into alpha on the root's and alpha's grants lists
/// alpha among its journal's scopes, and nobody edits the journal. When alpha's declaration then stops reading (a link to itself,
/// or a file with no permissions), the move's apply and rollback are refused in words that name alpha, with the error's own code,
/// and nothing moves. Once the declaration reads again the same journal is applied and rolled back.
#[test]
fn a_project_a_journal_runs_from_whose_declaration_does_not_read_is_named_in_the_refusal() {
    use std::os::unix::fs::PermissionsExt;
    for (broken, code) in [
        ("a link to itself", "central.file_map_failure"),
        ("no permissions", "central.file_map_denied"),
    ] {
        let world = granting(&every_act(), "control:root");
        granted_at(&world, &["control:root", "project:alpha"]);
        let root = world.root();
        let planned = ctrl(
            &world,
            "central.file-map.move-plan",
            &json!({"source_ref":page_source(&world),"destination":"Work/alpha/note2.md"}),
            Some(PERSON),
        );
        assert_eq!(planned["ok"], true, "{planned}");
        let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
        let before = ground(&world);
        let declaration = root.join("Work/alpha/ProjectCentral/project.json");
        let kept = fs::read(&declaration).unwrap();
        match broken {
            "a link to itself" => {
                fs::remove_file(&declaration).unwrap();
                std::os::unix::fs::symlink("project.json", &declaration).unwrap();
            }
            _ => {
                fs::set_permissions(&declaration, fs::Permissions::from_mode(0o000)).unwrap();
                if fs::read(&declaration).is_ok() {
                    eprintln!("skipped {broken}: this process reads a file with no permissions (a superuser)");
                    fs::set_permissions(&declaration, fs::Permissions::from_mode(0o644)).unwrap();
                    continue;
                }
            }
        }
        for (action, opening) in [
            (
                "central.file-map.move-apply",
                "Moving a file is the person's own press: ",
            ),
            (
                "central.file-map.move-rollback",
                "Moving a file back is the person's own press: ",
            ),
        ] {
            refused_as(
                &world,
                action,
                &carried,
                Some(PERSON),
                code,
                opening,
                "its journal runs from the project alpha, which does not read: ",
            );
        }
        if broken == "a link to itself" {
            fs::remove_file(&declaration).unwrap();
        } else {
            fs::set_permissions(&declaration, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(&declaration, kept).unwrap();
        assert_eq!(ground(&world), before, "{broken}: nothing is written");
        assert!(!root.join("Work/alpha/note2.md").exists(), "{broken}");
        let applied = ctrl(
            &world,
            "central.file-map.move-apply",
            &carried,
            Some(PERSON),
        );
        assert_eq!(applied["ok"], true, "{broken}: {applied}");
        assert!(root.join("Work/alpha/note2.md").is_file(), "{broken}");
        let back = ctrl(
            &world,
            "central.file-map.move-rollback",
            &carried,
            Some(PERSON),
        );
        assert_eq!(back["ok"], true, "{broken}: {back}");
        assert!(root.join("Control/user/note.md").is_file(), "{broken}");
        assert!(!root.join("Work/alpha/note2.md").exists(), "{broken}");
    }
}

/// Round 54f.2: the journal's root scope is the ground itself, as `move-plan` writes it. A journal that sets it inside a Work project
/// would carry the move out in alpha's tree on the root's grant; it is refused as no journal of the ground, and nothing moves.
#[test]
fn a_journal_that_sets_the_roots_scope_inside_a_work_project_is_refused() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/alpha/deep")).unwrap();
    let page = root.join("Work/alpha/deep/page.md");
    fs::write(&page, "the person's page\n").unwrap();
    let held = fs::metadata(&page).unwrap();
    let carried = edited_journal(&world, |plan| {
        for scope in plan["worlds"].as_array_mut().unwrap() {
            if scope["world"] == "control:root" {
                scope["path"] = json!("Work/alpha/deep");
            }
        }
        plan["from"] = json!("page.md");
        plan["to"] = json!("page2.md");
        plan["device"] = json!(held.dev());
        plan["inode"] = json!(held.ino());
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
        plan["refresh_worlds"] = json!([]);
    });
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_as(
            &world,
            action,
            &carried,
            Some(PERSON),
            "central.file_map_failure",
            opening,
            "Invalid move journal: it sets the root's scope away from the ground",
        );
    }
    assert_eq!(
        ground(&world),
        before,
        "nothing in alpha moves on the root's grant"
    );
    assert!(page.is_file());
}

/// The person's grant for every act at `scopes`, with the registrar's for `scope-register` at the root beside it.
fn granted_at(world: &World, scopes: &[&str]) {
    let acts = every_act();
    authority(
        world,
        vec![
            grant(PERSON, "human", scopes, &acts, u64::MAX),
            grant(
                OWNER,
                "human",
                &["control:root"],
                &["central.file-map.scope-register"],
                u64::MAX,
            ),
        ],
    );
}

/// The root's relations document, changed by `edit` as a hand or an older `ctrl` could have written it.
fn edit_root_relations(world: &World, edit: impl FnOnce(&mut Value)) {
    let relations = world.root().join("Control/relations/source-relations.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&relations).unwrap()).unwrap();
    edit(&mut document);
    fs::write(&relations, serde_json::to_vec(&document).unwrap()).unwrap();
}

/// A Source of the root's at `path`, kept in the root's file map as a registration made before round 54d's gate could have kept it
/// (a root Source in a Work project's tree, or one that holds a Work project). Answers its SourceRef.
fn legacy_root_source(world: &World, path: &str) -> String {
    let reference = Scope::resolve(&world.root(), None)
        .unwrap()
        .source_ref(path);
    edit_root_relations(world, |document| {
        document["file_map"]["resources"][&reference] = json!({"path":path,"title":path});
    });
    reference
}

/// Round 54g item 4 (review of #343, finding 2): a move stands at each Work project it writes in, and that is every one whose
/// links it rewrites, not only where its Source stands and where it goes. Alpha links the root's page; a move of the page repoints
/// alpha's link, so its plan, and its apply and rollback, are refused on the root's grant alone in words that name alpha, and
/// nothing is staged in alpha. On the root's grant and alpha's the move is carried out and alpha's link follows the page.
#[test]
fn a_root_move_that_repoints_a_link_in_alpha_stands_on_alphas_grant() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    granted_at(&world, &["control:root", "project:alpha"]);
    let source = page_source(&world);
    let linked = ctrl(
        &world,
        "central.file-map.link",
        &json!({"project":"alpha","source_ref":source,"path":"links/note","owner":"person",
            "expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    let reaches = "it reaches the Work project alpha, whose grant does not hold it";
    let plan = json!({"source_ref":source,"destination":"Control/user/note2.md"});
    granted_at(&world, &["control:root"]);
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &plan,
        Some(PERSON),
        "Planning a move is the person's own press: ",
        reaches,
    );
    assert_eq!(
        ground(&world),
        before,
        "a refused plan stages nothing in alpha"
    );
    assert_eq!(journals(&root), 0);
    granted_at(&world, &["control:root", "project:alpha"]);
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], true, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    granted_at(&world, &["control:root"]);
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_in_words(&world, action, &carried, Some(PERSON), opening, reaches);
    }
    assert_eq!(
        ground(&world),
        before,
        "a refused apply or rollback moves nothing"
    );
    granted_at(&world, &["control:root", "project:alpha"]);
    let applied = ctrl(
        &world,
        "central.file-map.move-apply",
        &carried,
        Some(PERSON),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        fs::canonicalize(root.join("Work/alpha/links/note")).unwrap(),
        fs::canonicalize(root.join("Control/user/note2.md")).unwrap(),
        "alpha's link follows the page"
    );
}

/// Round 54g item 4: a move's journal writes only where its move does. A journal edited after its plan to rewrite alpha's relations,
/// where the move changes no link, is refused on the root's grant and on alpha's alike; one whose link step is edited to land
/// outside its scope is refused on the grants its plan was made on. Nothing is written by either.
#[test]
fn an_edited_journal_is_refused_before_anything_is_written() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let carried = edited_journal(&world, |plan| {
        plan["documents"]
            .as_array_mut()
            .unwrap()
            .push(json!({"world":"project:alpha","path":null,"before":null,
            "after":"{\"edited\":\"by someone else\"}"}));
    });
    let alphas = root.join("Work/alpha/ProjectCentral/relations/source-relations.json");
    for scopes in [
        &["control:root"][..],
        &["control:root", "project:alpha"][..],
    ] {
        granted_at(&world, scopes);
        let before = ground(&world);
        for (action, opening) in [
            (
                "central.file-map.move-apply",
                "Moving a file is the person's own press: ",
            ),
            (
                "central.file-map.move-rollback",
                "Moving a file back is the person's own press: ",
            ),
        ] {
            refused_as(
                &world,
                action,
                &carried,
                Some(PERSON),
                "central.file_map_failure",
                opening,
                "Invalid move journal: it rewrites the relations of project:alpha, where its move changes no link",
            );
        }
        assert_eq!(ground(&world), before, "{scopes:?}: nothing is written");
        assert!(
            !alphas.exists(),
            "{scopes:?}: alpha's relations stay unwritten"
        );
    }
    // A plan that repoints alpha's link, its link step then edited to land outside alpha.
    granted_at(&world, &["control:root", "project:alpha"]);
    let source = page_source(&world);
    let linked = ctrl(
        &world,
        "central.file-map.link",
        &json!({"project":"alpha","source_ref":source,"path":"links/note","owner":"person",
            "expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/note3.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    let journal = root.join(format!(".central/bkmr/moves/{id}.json"));
    let mut plan: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    assert_eq!(plan["links"][0]["world"], "project:alpha", "{plan}");
    plan["links"][0]["after_path"] = json!("../../Control/user/planted");
    fs::write(&journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    let before = ground(&world);
    refused_as(
        &world,
        "central.file-map.move-apply",
        &json!({"plan_id":id,"quiesced":true}),
        Some(PERSON),
        "central.file_map_failure",
        "Moving a file is the person's own press: ",
        "Invalid move journal: its link step at links/note names a place its move does not write",
    );
    assert_eq!(ground(&world), before, "an edited link step writes nothing");
    assert!(!root.join("Control/user/planted").exists());
}

/// Round 54g item 5 (review of #343, finding 3): a place that holds a Work project's folder reaches that project's tree. `Work`
/// itself, and the ground by any spelling, are refused a register on the root's grant, in words that say which project they hold.
/// A move of a root Source that holds Work (kept before round 54d could refuse it) needs the grant of every project it holds:
/// refused naming alpha on the root's grant, naming beta on the root's and alpha's, and planned on all three. Nothing refused is
/// written.
#[test]
fn a_place_that_holds_a_work_project_reaches_its_tree() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work/beta"), "beta");
    let before = ground(&world);
    for spelled in ["Work", "Work/", ".", "Control/.."] {
        refused_in_words(
            &world,
            "central.file-map.register",
            &json!({"path":spelled,"expected_revision":revision(&world, None)}),
            Some(PERSON),
            opening("central.file-map.register"),
            &format!("{spelled} holds the Work project alpha, but the act names no project, so it stands at the root"),
        );
    }
    assert_eq!(ground(&world), before, "nothing refused is written");
    let work = legacy_root_source(&world, "Work");
    let plan = json!({"source_ref":work,"destination":"Work2"});
    for (scopes, missing) in [
        (&["control:root"][..], "alpha"),
        (&["control:root", "project:alpha"][..], "beta"),
    ] {
        granted_at(&world, scopes);
        let before = ground(&world);
        refused_in_words(
            &world,
            "central.file-map.move-plan",
            &plan,
            Some(PERSON),
            "Planning a move is the person's own press: ",
            &format!("it reaches the Work project {missing}, whose grant does not hold it"),
        );
        assert_eq!(
            ground(&world),
            before,
            "without {missing}'s grant: a refused move plans nothing"
        );
    }
    assert_eq!(journals(&root), 0);
    granted_at(&world, &["control:root", "project:alpha", "project:beta"]);
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], true, "{planned}");
}

/// Round 54g item 6 (review of #343, finding 4): Work's projects come from Work's own entries. The root's registrar cannot register
/// a scope under alpha's name at another place, and a register written by hand that does so (as one made before round 54g) stands
/// in for nothing at the gate: a root-only register and link into alpha are refused naming alpha, before the shadow and after it.
/// The after is the re-check's P3 (round 54g item 11): a gate that took Work's projects from the scopes it reads would let both
/// through and make `Work/alpha/links/note`.
#[test]
fn a_scope_registered_under_a_work_projects_name_stands_in_for_none() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let zeta = world.0.join("zeta");
    project(&zeta, "zeta");
    let zeta = fs::canonicalize(zeta).unwrap();
    let source = page_source(&world);
    let refused_into_alpha = |when: &str| {
        for (action, input) in [
            (
                "central.file-map.register",
                json!({"path":"Work/alpha/plan.md"}),
            ),
            (
                "central.file-map.link",
                json!({"source_ref":source,"path":"Work/alpha/links/note","owner":"person"}),
            ),
        ] {
            let mut input = input;
            input["expected_revision"] = revision(&world, None).into();
            let before = ground(&world);
            refused_in_words(
                &world,
                action,
                &input,
                Some(PERSON),
                opening(action),
                "is in the Work project alpha, but the act names no project, so it stands at the root",
            );
            assert_eq!(
                ground(&world),
                before,
                "{action} {when}: nothing refused is written"
            );
        }
        assert!(!root.join("Work/alpha/links/note").exists(), "{when}");
    };
    refused_into_alpha("before the shadow");
    // The registrar's grant holds this act, so it is refused past the gate, under the root's locks.
    let before = ground_but_locks(&world);
    let shadow = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"alpha","path":zeta,"allow_external":true,"expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(shadow["ok"], false, "{shadow}");
    assert_eq!(
        shadow["error"]["code"], "central.file_map_conflict",
        "{shadow}"
    );
    assert!(
        shadow["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(
                "The scope name alpha is already taken: it is the Work project at Work/alpha"
            ),
        "{shadow}"
    );
    assert_eq!(
        ground_but_locks(&world),
        before,
        "nothing refused is written"
    );
    edit_root_relations(&world, |document| {
        document["file_map"]["scopes"]["alpha"] = json!(zeta);
    });
    refused_into_alpha("after the shadow");
}

/// Round 54g item 7 (review of #343, finding 5): on macOS the firmlink `/System/Volumes/Data` spells every folder of the ground
/// another way. A folder inside alpha, spelled so, is refused as a folder scope ("inside this ground"), as its canonical spelling
/// is; and a folder scope's declaration edited by hand to that spelling is not read, so its file links nowhere on the root's grant.
#[test]
fn the_firmlink_spelling_of_a_folder_in_alpha_is_no_folder_scope() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let Some(firm) = firmlink(&root) else {
        eprintln!("no firmlink spelling of the ground here; nothing to probe");
        return;
    };
    let audio = root.join("Work/alpha/audio");
    fs::create_dir_all(&audio).unwrap();
    fs::write(audio.join("voice.wav"), b"RIFF").unwrap();
    let spelled = firm.join("Work/alpha/audio");
    // Both refusals below are the act's own, past the gate and under the root's locks.
    let before = ground_but_locks(&world);
    let registered = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"audio","path":spelled,"kind":"folder","allow_external":true,"media":["wav"],
            "expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(registered["ok"], false, "{registered}");
    assert!(
        registered["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("is inside this ground"),
        "{registered}"
    );
    assert_eq!(
        ground_but_locks(&world),
        before,
        "nothing refused is written"
    );
    // The Music folder outside the ground, its declaration then pointed at alpha's folder by that spelling.
    declare_outside_scopes(&world);
    let declaration = root.join(".central/scopes/Music/scope.json");
    let mut declared: Value = serde_json::from_slice(&fs::read(&declaration).unwrap()).unwrap();
    declared["root"] = json!(spelled);
    fs::write(&declaration, serde_json::to_vec_pretty(&declared).unwrap()).unwrap();
    let link = json!({"source_ref":"central:source:folder:Music:voice.wav","path":"Control/user/voice-link",
        "owner":"person","expected_revision":revision(&world, None)});
    let before = ground_but_locks(&world);
    let refused = ctrl(&world, "central.file-map.link", &link, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("is inside this ground"),
        "{refused}"
    );
    assert_eq!(
        ground_but_locks(&world),
        before,
        "nothing refused is written"
    );
    assert!(fs::symlink_metadata(root.join("Control/user/voice-link")).is_err());
}

/// Round 54g item 8 (review of #343, mutants A4, A6 and A9) and item 10: a root act on a file in alpha's tree stands on alpha's grant
/// as well, though the act only reads the file. A root Source in alpha (kept before round 54d could refuse it) is refused a bookmark
/// binding and a move's plan on the root's grant alone, and a move planned on the root's and alpha's grants is refused its apply and
/// rollback on the root's alone; a database inside alpha is refused adoption at the root, by either spelling. Each refusal names
/// alpha, and nothing refused is written.
#[test]
fn a_root_act_on_a_file_in_alpha_stands_on_alphas_grant() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::write(root.join("Work/alpha/bookmarks.db"), b"SQLite format 3\0").unwrap();
    let legacy = legacy_root_source(&world, "Work/alpha/plan.md");
    let reaches = "it reaches the Work project alpha, whose grant does not hold it";
    let database = fs::canonicalize(root.join("Work/alpha/bookmarks.db")).unwrap();
    let before = ground(&world);
    for (action, input) in [
        (
            "central.file-map.record-adopt",
            json!({"source_ref":legacy,"record_id":1,"record_revision":"r"}),
        ),
        (
            "central.file-map.adopt-db",
            json!({"database":"Work/alpha/bookmarks.db","quiesced":true}),
        ),
        (
            "central.file-map.adopt-db",
            json!({"database":database,"quiesced":true}),
        ),
    ] {
        let mut input = input;
        input["expected_revision"] = revision(&world, None).into();
        refused_in_words(
            &world,
            action,
            &input,
            Some(PERSON),
            opening(action),
            reaches,
        );
    }
    let plan = json!({"source_ref":legacy,"destination":"Control/user/plan.md"});
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &plan,
        Some(PERSON),
        "Planning a move is the person's own press: ",
        reaches,
    );
    assert_eq!(ground(&world), before, "nothing refused is written");
    assert_eq!(journals(&root), 0);
    granted_at(&world, &["control:root", "project:alpha"]);
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], true, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    granted_at(&world, &["control:root"]);
    let before = ground(&world);
    for (action, opening) in [
        (
            "central.file-map.move-apply",
            "Moving a file is the person's own press: ",
        ),
        (
            "central.file-map.move-rollback",
            "Moving a file back is the person's own press: ",
        ),
    ] {
        refused_in_words(&world, action, &carried, Some(PERSON), opening, reaches);
    }
    assert_eq!(
        ground(&world),
        before,
        "a refused apply or rollback moves nothing"
    );
}

/// Round 54g item 10 (review of #343, finding 11): a hard link carries a project's file under another name, a stated limit against
/// a writer that runs as the same user. A root Source registered at a hard link to alpha's file is the root's, and moving it renames
/// the root's name for the file alone: no act writes through a file path, so no file of alpha's is made, changed or removed. What
/// the move does make in alpha is what every move makes in every scope it runs with: the scope's two lock files, empty, in its
/// `.central` (main's behaviour before this round, pinned here as it is).
#[test]
fn a_hard_link_at_the_root_is_the_roots_and_no_file_of_alphas_is_written() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::hard_link(
        root.join("Work/alpha/plan.md"),
        root.join("Control/user/hl.md"),
    )
    .unwrap();
    let alpha = |world: &World| {
        ground(world)
            .into_iter()
            .filter(|(path, _)| path.starts_with(world.root().join("Work/alpha")))
            .collect::<Vec<_>>()
    };
    let before = alpha(&world);
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Control/user/hl.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    assert_eq!(
        registered["data"]["result"]["world_ref"], "control:root",
        "{registered}"
    );
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":registered["data"]["result"]["source_ref"],"destination":"Control/user/hl2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let applied = ctrl(
        &world,
        "central.file-map.move-apply",
        &json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true}),
        Some(PERSON),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert!(root.join("Control/user/hl2.md").is_file());
    let after = alpha(&world);
    assert!(
        before.iter().all(|kept| after.contains(kept)),
        "nothing of alpha's is changed or removed"
    );
    let made: Vec<_> = after
        .iter()
        .filter(|entry| !before.contains(entry))
        .map(|(path, bytes)| {
            let within = path.strip_prefix(root.join("Work/alpha")).unwrap();
            (within.to_path_buf(), bytes.clone())
        })
        .collect();
    let locks = [
        ".central",
        ".central/file-map.lock",
        ".central/source-mutation.lock",
    ]
    .map(|lock| (PathBuf::from(lock), Vec::new()));
    assert_eq!(
        made, locks,
        "the move makes alpha's two empty lock files and nothing else"
    );
}
