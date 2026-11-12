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
    ctrl_at(world, &world.0, action, input, token, variables)
}

/// [`ctrl_with`] run from the working directory `cwd`.
fn ctrl_at(
    world: &World,
    cwd: &Path,
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
        .current_dir(cwd)
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
    // On every grant the gate has nothing to refuse, and the plan itself refuses to move `Work`
    // (`a_move_of_work_itself_is_refused_at_its_plan`).
    granted_at(&world, &["control:root", "project:alpha", "project:beta"]);
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], false, "{planned}");
    assert!(
        planned["error"]["message"]
            .as_str()
            .is_some_and(|words| words.contains("Central does not move it")),
        "{planned}"
    );
    assert_eq!(journals(&root), 0);
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
/// the move made in alpha before was the scope's two lock files, empty, in its `.central`; a move now makes none in a Work
/// project it does not reach (DLW build ruling 2026-11-12), so alpha's tree is as it was, entry for entry.
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
    assert_eq!(
        made,
        Vec::<(PathBuf, Vec<u8>)>::new(),
        "the move makes nothing in alpha, a lock file no more than anything else"
    );
}

/// The files of alpha's tree, with their bytes or where they link: what the root's grant alone must leave as it was.
fn alphas_tree(world: &World) -> Vec<(PathBuf, Vec<u8>)> {
    let alpha = world.root().join("Work/alpha");
    ground_but_locks(world)
        .into_iter()
        .filter(|(path, _)| path.starts_with(&alpha))
        .collect()
}

/// A move's two carried acts, each with the words its refusal opens with.
const CARRIED: [(&str, &str); 2] = [
    (
        "central.file-map.move-apply",
        "Moving a file is the person's own press: ",
    ),
    (
        "central.file-map.move-rollback",
        "Moving a file back is the person's own press: ",
    ),
];

/// What a refusal says of a Work project whose grant is missing.
const REACHES_ALPHA: &str = "it reaches the Work project alpha, whose grant does not hold it";

/// A link the root keeps at `path` to the person's page, made as `link` at the root makes one. Answers the page's SourceRef.
fn root_link(world: &World, path: &str) -> String {
    let source = page_source(world);
    let linked = ctrl(
        world,
        "central.file-map.link",
        &json!({"source_ref":source,"path":path,"owner":"person","expected_revision":revision(world, None)}),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    source
}

/// A planned move of the person's page to `destination`, its journal then edited by `edit`. Answers the move's carried input.
fn edited_move(world: &World, destination: &str, edit: impl FnOnce(&mut Value)) -> Value {
    let planned = ctrl(
        world,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(world),"destination":destination}),
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

/// Round 54h item 1 (review of #346, F1): a link step's own path is a place the gate judges. A journal edited to add a link step
/// in the root's world at a path inside alpha is refused on the root's grant alone, in words that name alpha: alpha's `plan.md`
/// stays as it was and nothing else changes.
#[test]
fn an_edited_link_step_of_the_roots_inside_alpha_stands_on_alphas_grant() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let target = fs::symlink_metadata(root.join("Work/alpha/plan.md")).unwrap();
    let carried = edited_journal(&world, |plan| {
        let id = plan["id"].as_str().unwrap().to_owned();
        let staged = format!(".central/bkmr/moves/{id}-0-new");
        fs::write(root.join(&staged), "planted through the root's journal\n").unwrap();
        let made = fs::symlink_metadata(root.join(&staged)).unwrap();
        plan["links"].as_array_mut().unwrap().push(
            json!({"world":"control:root","path":"Work/alpha/plan.md",
            "after_path":"Work/alpha/plan.md",
            "before":{"source_ref":"x","world_ref":"control:root","owner":"person","target":"x",
                "device":target.dev(),"inode":target.ino()},
            "staged":staged,"backup":format!(".central/bkmr/moves/{id}-0-old"),
            "new_device":made.dev(),"new_inode":made.ino()}),
        );
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            REACHES_ALPHA,
        );
    }
    assert_eq!(
        fs::read_to_string(root.join("Work/alpha/plan.md")).unwrap(),
        "alpha's plan\n"
    );
    assert_eq!(ground(&world), before, "nothing is written or moved");
}

/// Round 54h item 1: a link step in alpha's own world needs alpha's grant by its own place, with no relations document of alpha's
/// in the journal to ask it (the reviewer's mutant M9).
#[test]
fn an_edited_link_step_in_alphas_world_stands_on_alphas_grant() {
    let world = granting(&every_act(), "control:root");
    let carried = edited_journal(&world, |plan| {
        let id = plan["id"].as_str().unwrap().to_owned();
        plan["links"].as_array_mut().unwrap().push(json!({"world":"project:alpha","path":"plan.md","after_path":"plan.md",
            "before":{"source_ref":"x","world_ref":"project:alpha","owner":"person","target":"x","device":0,"inode":0},
            "staged":format!(".central/bkmr/moves/{id}-0-new"),"backup":format!(".central/bkmr/moves/{id}-0-old"),
            "new_device":0,"new_inode":0}));
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            REACHES_ALPHA,
        );
    }
    assert_eq!(ground(&world), before, "nothing is written or moved");
}

/// Round 54h item 1: apply's link steps refuse a link in any folder above the step's path at their own check. A journal's link step
/// is edited to stand behind a link the root holds to alpha's folder. On the root's grant alone the gate refuses it, naming alpha,
/// since the step's place is read on the disk. On the root's grant and alpha's the gate has nothing to refuse, and apply itself
/// refuses the link in the path: alpha's `plan.md` stays as it was.
#[test]
fn a_link_step_behind_a_link_is_refused_by_the_gate_and_by_apply() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    std::os::unix::fs::symlink("../../Work/alpha", root.join("Control/user/door")).unwrap();
    let target = fs::symlink_metadata(root.join("Work/alpha/plan.md")).unwrap();
    let carried = edited_journal(&world, |plan| {
        let id = plan["id"].as_str().unwrap().to_owned();
        let staged = format!(".central/bkmr/moves/{id}-0-new");
        fs::write(root.join(&staged), "planted behind the door\n").unwrap();
        let made = fs::symlink_metadata(root.join(&staged)).unwrap();
        plan["links"].as_array_mut().unwrap().push(
            json!({"world":"control:root","path":"Control/user/door/plan.md",
            "after_path":"Control/user/door/plan.md",
            "before":{"source_ref":"x","world_ref":"control:root","owner":"person","target":"x",
                "device":target.dev(),"inode":target.ino()},
            "staged":staged,"backup":format!(".central/bkmr/moves/{id}-0-old"),
            "new_device":made.dev(),"new_inode":made.ino()}),
        );
    });
    let before = ground(&world);
    refused_in_words(
        &world,
        CARRIED[0].0,
        &carried,
        Some(PERSON),
        CARRIED[0].1,
        REACHES_ALPHA,
    );
    assert_eq!(ground(&world), before, "the gate's refusal writes nothing");
    granted_at(&world, &["control:root", "project:alpha"]);
    let before = alphas_tree(&world);
    let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains("refusing symlink source path component"),
        "{words}"
    );
    assert_eq!(
        alphas_tree(&world),
        before,
        "apply's own check writes nothing in alpha"
    );
    assert!(
        root.join("Control/user/note.md").is_file(),
        "the refusal comes before the Source moves"
    );
}

/// Round 54h item 2 (review of #346, F2): a journal's scope is spelled as its plan writes it. A journal whose project scope is
/// spelled through the moved folder with `..` reads a stand-in before the rename and alpha after it; it is refused on the root's
/// grant alone before anything is written, and alpha's relations stay unwritten.
#[test]
fn a_journal_scope_spelled_through_the_moved_folder_is_refused() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Control/user/dir")).unwrap();
    fs::write(root.join("Control/user/dir/in.md"), "in\n").unwrap();
    let stand_in = root.join("Control/Work/alpha");
    project(&stand_in, "alpha");
    fs::hard_link(root.join("Work/alpha/plan.md"), stand_in.join("plan.md")).unwrap();
    let real = fs::symlink_metadata(root.join("Work/alpha/plan.md")).unwrap();
    let dir = fs::symlink_metadata(root.join("Control/user/dir")).unwrap();
    let spelled = "Control/user/dir/../../Work/alpha";
    let carried = edited_journal(&world, |plan| {
        let id = plan["id"].as_str().unwrap().to_owned();
        let staged = format!(".central/bkmr/moves/{id}-0-new");
        fs::create_dir_all(stand_in.join(".central/bkmr/moves")).unwrap();
        fs::hard_link(root.join("Work/alpha/plan.md"), stand_in.join(&staged)).unwrap();
        plan["from"] = json!("Control/user/dir");
        plan["to"] = json!("Control/dir2");
        plan["device"] = json!(dir.dev());
        plan["inode"] = json!(dir.ino());
        for scope in plan["worlds"].as_array_mut().unwrap() {
            if scope["world"] == "project:alpha" {
                scope["path"] = json!(spelled);
            }
        }
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([{"world":"project:alpha","path":null,"before":null,
            "after":"{\"edited\":\"by someone else\"}"}]);
        plan["links"] = json!([{"world":"project:alpha","path":"plan.md","after_path":"plan.md",
            "before":{"source_ref":"x","world_ref":"project:alpha","owner":"person","target":"x","device":0,"inode":0},
            "staged":staged,"backup":format!(".central/bkmr/moves/{id}-0-old"),
            "new_device":real.dev(),"new_inode":real.ino()}]);
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_as(
            &world,
            action,
            &carried,
            Some(PERSON),
            "central.file_map_failure",
            opening,
            &format!("Invalid move journal: it spells the scope project:alpha as {spelled}"),
        );
    }
    assert_eq!(ground(&world), before, "nothing is written or moved");
    assert!(!root
        .join("Work/alpha/ProjectCentral/relations/source-relations.json")
        .exists());
}

/// Round 54h item 2, as round 54j item 1 leaves it (DLW build ruling 2026-11-11; the review of #346's third commit, C1): a journal's
/// project scope spelled, in plain names, through a link inside the moved folder (`dir/lnk -> ../Work/alpha`) reads a stand-in in
/// the root's tree before the rename and alpha after it. Round 54h let the rename happen and put it back; a scope spelled through a
/// link is now refused as a spelling, at the gate, before the rename: nothing moves and nothing is written.
#[test]
fn a_journal_scope_spelled_through_a_link_in_the_moved_folder_is_refused() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Control/user/dir")).unwrap();
    fs::write(root.join("Control/user/dir/in.md"), "in\n").unwrap();
    std::os::unix::fs::symlink("../Work/alpha", root.join("Control/user/dir/lnk")).unwrap();
    let stand_in = root.join("Control/user/Work/alpha");
    project(&stand_in, "alpha");
    let dir = identity(&root.join("Control/user/dir"));
    let carried = edited_journal(&world, |plan| {
        plan["from"] = json!("Control/user/dir");
        plan["to"] = json!("dir2");
        plan["device"] = json!(dir.0);
        plan["inode"] = json!(dir.1);
        for scope in plan["worlds"].as_array_mut().unwrap() {
            if scope["world"] == "project:alpha" {
                scope["path"] = json!("Control/user/dir/lnk");
            }
        }
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_as(
            &world,
            action,
            &carried,
            Some(PERSON),
            "central.file_map_failure",
            opening,
            "This move's journal spells the scope project:alpha as Control/user/dir/lnk, which is not that folder's own \
             spelling on the disk now",
        );
    }
    assert_eq!(ground(&world), before, "nothing is moved or written");
    assert!(
        root.join("Control/user/dir/in.md").is_file() && !root.join("dir2").exists(),
        "the folder never moved"
    );
}

/// Round 54h item 3 (review of #346, F3): a rewritten link is judged by where it stands. The root keeps a link inside alpha's
/// tree, as `link` at the root made one before round 54d refused it. A move of its Source is refused on the root's grant alone,
/// naming alpha, at its plan; and at its apply and rollback, for a plan made on both grants. On both grants it plans, applies and
/// rolls back.
#[test]
fn a_link_the_root_keeps_inside_alpha_stands_on_alphas_grant() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let source = page_source(&world);
    fs::create_dir_all(root.join("Work/alpha/links")).unwrap();
    let link = root.join("Work/alpha/links/note");
    std::os::unix::fs::symlink("../../../Control/user/note.md", &link).unwrap();
    let made = fs::symlink_metadata(&link).unwrap();
    edit_root_relations(&world, |document| {
        document["file_map"]["links"]["Work/alpha/links/note"] = json!({"source_ref":source,"world_ref":"control:root",
            "owner":"person","target":"../../../Control/user/note.md","device":made.dev(),"inode":made.ino()});
    });
    let plan = json!({"source_ref":source,"destination":"Control/user/note2.md"});
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &plan,
        Some(PERSON),
        "Planning a move is the person's own press: ",
        REACHES_ALPHA,
    );
    assert_eq!(ground(&world), before, "a refused plan writes nothing");
    assert_eq!(journals(&root), 0);
    granted_at(&world, &["control:root", "project:alpha"]);
    let planned = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(planned["data"]["result"]["links"], 1, "{planned}");
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    granted_at(&world, &["control:root"]);
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            REACHES_ALPHA,
        );
    }
    assert_eq!(
        ground(&world),
        before,
        "a refused apply or rollback moves nothing"
    );
    granted_at(&world, &["control:root", "project:alpha"]);
    let applied = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        Path::new("../../../Control/user/note2.md")
    );
    let back = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        Path::new("../../../Control/user/note.md")
    );
    assert!(root.join("Control/user/note.md").is_file());
}

/// Round 54h item 4 (the reviewer's mutants M1 and M2): a link step's staged and kept links stand only at the names its plan gives
/// them, and its path is contained in its scope. A real link step of the root's, edited in each of those, is refused as an invalid
/// journal, and nothing is written.
#[test]
fn a_link_step_edited_in_its_names_or_its_path_is_an_invalid_journal() {
    for (field, planted) in [
        ("backup", "Work/alpha/kept"),
        ("staged", "Control/user/note.md"),
        ("path", "../outside"),
        ("path", "/outside"),
    ] {
        let world = granting(&every_act(), "control:root");
        root_link(&world, "Control/user/note-link");
        let carried = edited_move(&world, "Control/user/note2.md", |plan| {
            assert_eq!(plan["links"][0]["world"], "control:root", "{plan}");
            assert_eq!(plan["links"][0]["path"], "Control/user/note-link", "{plan}");
            plan["links"][0][field] = json!(planted);
        });
        // The refusal names the step by its path, as the journal now spells it.
        let named = if field == "path" {
            planted
        } else {
            "Control/user/note-link"
        };
        let before = ground(&world);
        for (action, opening) in CARRIED {
            refused_as(
                &world,
                action,
                &carried,
                Some(PERSON),
                "central.file_map_failure",
                opening,
                &format!("Invalid move journal: its link step at {named} names a place its move does not write"),
            );
        }
        assert_eq!(
            ground(&world),
            before,
            "{field} {planted}: nothing is written"
        );
    }
}

/// Round 54h item 6 (review of #346, F5): a link whose Source is gone stops a move's plan in words that name the link and the
/// project it stands in, and say that no grant mends it. The words are the same on the root's grant alone and with alpha's: the
/// person is not sent for a grant that will not help.
#[test]
fn a_link_whose_source_is_gone_is_named_with_what_mends_it() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    granted_at(&world, &["control:root", "project:alpha"]);
    fs::write(root.join("Control/user/gone.md"), "gone\n").unwrap();
    fs::write(root.join("Control/user/other.md"), "other\n").unwrap();
    let located = |path: &str| {
        let located = ctrl(
            &world,
            "central.file-map.locate",
            &json!({"path":path}),
            None,
        );
        located["data"]["result"]["source"]["ref"]
            .as_str()
            .unwrap_or_else(|| panic!("{located}"))
            .to_owned()
    };
    let (gone, other) = (
        located("Control/user/gone.md"),
        located("Control/user/other.md"),
    );
    let linked = ctrl(
        &world,
        "central.file-map.link",
        &json!({"project":"alpha","source_ref":gone,"path":"links/gone","owner":"person",
            "expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    fs::remove_file(root.join("Control/user/gone.md")).unwrap();
    let plan = json!({"source_ref":other,"destination":"Control/user/other2.md"});
    for scopes in [
        &["control:root"][..],
        &["control:root", "project:alpha"][..],
    ] {
        granted_at(&world, scopes);
        let before = ground(&world);
        let refused = ctrl(&world, "central.file-map.move-plan", &plan, Some(PERSON));
        assert_eq!(refused["ok"], false, "{scopes:?}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.starts_with("Planning a move is the person's own press: the link links/gone in the project alpha names a Source that does not read"),
            "{scopes:?}: {words}"
        );
        assert!(
            words.contains("no grant mends this: remove the link or restore its Source"),
            "{scopes:?}: {words}"
        );
        assert!(
            !words.contains("whose grant does not hold it"),
            "{scopes:?}: {words}"
        );
        assert_eq!(ground(&world), before, "{scopes:?}: nothing is planned");
    }
}

/// Round 54h item 4 (the reviewer's mutant M10): a scope whose relations document does not read counts as one the move rewrites,
/// since the plan cannot tell. An unrelated move at the root is refused on the root's grant alone, naming alpha.
#[test]
fn a_project_whose_relations_do_not_read_is_asked_for_its_grant() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let source = page_source(&world);
    let relations = root.join("Work/alpha/ProjectCentral/relations/source-relations.json");
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    fs::write(&relations, "{ this is not a relations document").unwrap();
    let before = ground(&world);
    refused_in_words(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/note2.md"}),
        Some(PERSON),
        "Planning a move is the person's own press: ",
        REACHES_ALPHA,
    );
    assert_eq!(ground(&world), before, "nothing is planned");
}

/// Round 54h item 4 (the reviewer's mutants M4 and M7, for round 54g's item 14 and its regression note): a folder under Work that
/// holds no declaration is no project, so the root's grant alone registers a file in it; and a Work project registers at its own
/// place, by its path in the ground or its absolute path, changing nothing.
#[test]
fn an_undeclared_folder_under_work_is_the_roots_and_a_work_project_registers_where_it_stands() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/beta")).unwrap();
    fs::write(root.join("Work/beta/loose.md"), "loose\n").unwrap();
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Work/beta/loose.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let absolute = fs::canonicalize(root.join("Work/alpha")).unwrap();
    for declared in [
        json!({"name":"alpha","path":"Work/alpha"}),
        json!({"name":"alpha","path":absolute,"allow_external":true}),
    ] {
        let mut declared = declared;
        declared["expected_revision"] = revision(&world, None).into();
        let answered = ctrl(
            &world,
            "central.file-map.scope-register",
            &declared,
            Some(OWNER),
        );
        assert_eq!(answered["ok"], true, "{answered}");
        assert_eq!(
            answered["data"]["result"],
            json!({"changed":false,"world_ref":"project:alpha"}),
            "{declared}"
        );
    }
}

/// Round 54h item 5 (review of #346, F4): a Work project whose `ProjectCentral` does not open is named, with what to mend, and an
/// act that does not touch it goes on. With the folder at mode 000, and with a link to itself in its place: a register at the
/// root's own page goes through; a register of a file in alpha, and a move's plan (which reads every project's links), are refused
/// in words that name alpha and say what to mend. The same holds for a declaration that is a file with no permissions, and for
/// one that is no declaration.
#[test]
fn a_work_project_that_does_not_open_is_named_and_other_acts_go_on() {
    use std::os::unix::fs::PermissionsExt;
    // A folder's mode does not keep root out.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let source = page_source(&world);
    let central = root.join("Work/alpha/ProjectCentral");
    let closed = |tag: &str| {
        let page = format!("Control/user/{tag}.md");
        fs::write(root.join(&page), "another page\n").unwrap();
        let registered = ctrl(
            &world,
            "central.file-map.register",
            &json!({"path":page,"expected_revision":revision(&world, None)}),
            Some(PERSON),
        );
        assert_eq!(registered["ok"], true, "{tag}: {registered}");
        let mends = "the Work project alpha does not read its declaration, Work/alpha/ProjectCentral/project.json: ";
        for (action, input) in [
            (
                "central.file-map.register",
                json!({"path":"Work/alpha/plan.md","expected_revision":revision(&world, None)}),
            ),
            (
                "central.file-map.move-plan",
                json!({"source_ref":source,"destination":"Control/user/note2.md"}),
            ),
        ] {
            let refused = ctrl(&world, action, &input, Some(PERSON));
            assert_eq!(refused["ok"], false, "{tag} {action}: {refused}");
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(words.contains(mends), "{tag} {action}: {words}");
            assert!(
                words.contains("mend that file or its folder (a mode, or a link in its place) and the project stands again"),
                "{tag} {action}: {words}"
            );
        }
        assert_eq!(journals(&root), 0, "{tag}: nothing is planned");
        // The readers answer too, with the project listed as unreachable in the same words (a change beyond the review's ruling:
        // before, one Work project that did not open failed every read of the map).
        for (action, input) in [
            ("central.file-map.inspect", json!({"resources":false})),
            ("central.file-map.search", json!({"query":"page","limit":1})),
            (
                "central.file-map.locate",
                json!({"path":"Control/user/note.md"}),
            ),
        ] {
            let read = ctrl(&world, action, &input, None);
            assert_eq!(read["ok"], true, "{tag} {action}: {read}");
            let listed = read["data"]["unreachable"].to_string();
            assert!(listed.contains(mends), "{tag} {action}: {read}");
        }
        // A location inside the project is the project's to say (round 54i item 3; B3): not "unregistered".
        let located = ctrl(
            &world,
            "central.file-map.locate",
            &json!({"path":"Work/alpha/plan.md"}),
            None,
        );
        assert_eq!(located["ok"], false, "{tag}: {located}");
        let words = located["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.starts_with("Location is in Work/alpha, which is unreachable: ")
                && words.contains(mends),
            "{tag}: {words}"
        );
    };
    fs::set_permissions(&central, fs::Permissions::from_mode(0o000)).unwrap();
    closed("closed");
    fs::set_permissions(&central, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(&central, root.join("Work/alpha/kept")).unwrap();
    std::os::unix::fs::symlink("ProjectCentral", &central).unwrap();
    closed("looped");
    fs::remove_file(&central).unwrap();
    fs::rename(root.join("Work/alpha/kept"), &central).unwrap();
    let declaration = central.join("project.json");
    fs::set_permissions(&declaration, fs::Permissions::from_mode(0o000)).unwrap();
    closed("unreadable");
    fs::set_permissions(&declaration, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&declaration, "this is no declaration").unwrap();
    closed("garbled");
    // The project's own folder closed by its mode: the declaration is there and cannot be reached.
    let alpha = root.join("Work/alpha");
    fs::set_permissions(&alpha, fs::Permissions::from_mode(0o000)).unwrap();
    closed("shut");
    fs::set_permissions(&alpha, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Round 54h item 7 (review of #346, F6): a pool stops at a Work project's folder. A project scope registered over `Work`, pooled
/// on the root's grant, takes none of alpha's files as its own Sources: alpha's `plan.md` is located as alpha's or as no Source,
/// never as the other scope's. A file of that scope's own outside alpha is still pooled.
#[test]
fn a_pool_of_a_scope_that_holds_a_work_project_stops_at_it() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work"), "works");
    fs::write(root.join("Work/own.md"), "the scope's own page\n").unwrap();
    let registered = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"works","path":"Work","expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let pooled = ctrl(
        &world,
        "central.file-map.pool",
        &json!({"project":"works","enable":true,"expected_revision":revision(&world, Some("works"))}),
        Some(PERSON),
    );
    assert_eq!(pooled["ok"], true, "{pooled}");
    let own = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"project":"works","path":"own.md"}),
        None,
    );
    assert_eq!(
        own["data"]["result"]["source"]["ref"], "central:source:project:works:own.md",
        "{own}"
    );
    for asked in [
        json!({"path":"Work/alpha/plan.md"}),
        json!({"project":"works","path":"alpha/plan.md"}),
    ] {
        let located = ctrl(&world, "central.file-map.locate", &asked, None);
        let text = located.to_string();
        assert!(
            !text.contains("central:source:project:works:alpha/plan.md"),
            "{asked}: {located}"
        );
    }
}

/// Round 54h item 1: a link step is judged where its link stands after the move as well as before it. A real link step of the
/// root's, edited so the rewritten link lands inside alpha, is refused on the root's grant alone, naming alpha, and nothing is
/// written.
#[test]
fn an_edited_link_step_that_lands_inside_alpha_stands_on_alphas_grant() {
    let world = granting(&every_act(), "control:root");
    root_link(&world, "Control/user/note-link");
    let carried = edited_move(&world, "Control/user/note2.md", |plan| {
        assert_eq!(plan["links"][0]["path"], "Control/user/note-link", "{plan}");
        plan["links"][0]["after_path"] = json!("Work/alpha/landed");
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_in_words(
            &world,
            action,
            &carried,
            Some(PERSON),
            opening,
            REACHES_ALPHA,
        );
    }
    assert_eq!(ground(&world), before, "nothing is written or moved");
    assert!(!world.root().join("Work/alpha/landed").exists());
}

/// Round 54h item 5, beyond the review's ruling: a folder under Work that holds a file called `ProjectCentral` holds no
/// declaration, so it is no project. The map reads on, and the root's grant alone registers a file in it. Before, that folder
/// failed every act of the map with a bare "Not a directory".
#[test]
fn a_work_folder_with_a_file_called_projectcentral_is_no_project() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/gamma")).unwrap();
    fs::write(
        root.join("Work/gamma/ProjectCentral"),
        "a file, not a folder\n",
    )
    .unwrap();
    fs::write(root.join("Work/gamma/loose.md"), "loose\n").unwrap();
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Work/gamma/loose.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let inspected = ctrl(
        &world,
        "central.file-map.inspect",
        &json!({"resources":false}),
        None,
    );
    assert_eq!(inspected["ok"], true, "{inspected}");
    assert!(
        inspected["data"].get("unreachable").is_none(),
        "{inspected}"
    );
}

/// Round 54h items 1 and 3: a link is judged where it stands and is never read through to what it points at. The root keeps a link
/// in a folder of its own that points at alpha's folder (a Source kept before round 54d could refuse it). A move of the link's
/// folder rewrites the link where it stands, in the root's tree, so the root's grant alone plans it, applies it and rolls it back,
/// and no file of alpha's changes.
#[test]
fn a_link_that_points_into_alpha_is_judged_where_it_stands() {
    use std::os::unix::fs::MetadataExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let alphas = legacy_root_source(&world, "Work/alpha");
    fs::create_dir_all(root.join("Control/user/box")).unwrap();
    fs::write(root.join("Control/user/box/a.md"), "a\n").unwrap();
    let link = root.join("Control/user/box/door");
    std::os::unix::fs::symlink("../../../Work/alpha", &link).unwrap();
    let made = fs::symlink_metadata(&link).unwrap();
    edit_root_relations(&world, |document| {
        document["file_map"]["links"]["Control/user/box/door"] = json!({"source_ref":alphas,"world_ref":"control:root",
            "owner":"person","target":"../../../Work/alpha","device":made.dev(),"inode":made.ino()});
    });
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Control/user/box","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":registered["data"]["result"]["source_ref"],"destination":"Control/user/box2"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(planned["data"]["result"]["links"], 1, "{planned}");
    // A move takes its locks in every scope it runs with, so the plan has made alpha's `.central` by now (a stated limit).
    let before = alphas_tree(&world);
    let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
    let applied = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        fs::canonicalize(root.join("Control/user/box2/door")).unwrap(),
        fs::canonicalize(root.join("Work/alpha")).unwrap()
    );
    let back = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert!(root.join("Control/user/box/door").is_symlink());
    assert_eq!(alphas_tree(&world), before, "nothing in alpha changes");
}

/// A link step of the root's world for the journal `plan`, as `plan` writes one: at `path`, replacing the entry whose device and
/// inode are `before`, by the staged entry whose device and inode are `staged` (it stands at the plan's own staged name).
fn root_step(plan: &Value, path: &str, before: (u64, u64), staged: (u64, u64)) -> Value {
    let id = plan["id"].as_str().unwrap();
    json!({"world":"control:root","path":path,"after_path":path,
        "before":{"source_ref":"x","world_ref":"control:root","owner":"person","target":"x","device":before.0,"inode":before.1},
        "staged":format!(".central/bkmr/moves/{id}-0-new"),"backup":format!(".central/bkmr/moves/{id}-0-old"),
        "new_device":staged.0,"new_inode":staged.1})
}

/// An entry's device and inode, the entry itself and never what a link points at.
fn identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let entry = fs::symlink_metadata(path).unwrap();
    (entry.dev(), entry.ino())
}

/// Round 54i item 1 (DLW build ruling 2026-11-11; the review of #346's second commit, B1): a link step replaces a link and nothing
/// else. A journal edited to add a link step of the root's at alpha's own folder, or at `Work`, would move the whole folder into
/// the root's moves folder. On the root's grant alone the gate refuses it, naming alpha: a link place where a folder stands is
/// judged as that folder, with all it holds. Apply and rollback are both refused, and nothing changes. On the root's grant and
/// alpha's the gate has nothing to refuse, and apply itself refuses the step, in words that say what stands there: a scope's own
/// folder, or a folder above one (round 54j item 1). The folder stands where it was, with its bytes.
#[test]
fn a_link_step_at_a_folder_moves_no_folder() {
    for path in ["Work/alpha", "Work"] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        let carried = edited_journal(&world, |plan| {
            let id = plan["id"].as_str().unwrap().to_owned();
            let staged = root.join(format!(".central/bkmr/moves/{id}-0-new"));
            fs::write(&staged, "planted in the folder's place\n").unwrap();
            let step = root_step(plan, path, identity(&root.join(path)), identity(&staged));
            plan["links"].as_array_mut().unwrap().push(step);
        });
        let before = ground(&world);
        for (action, opening) in CARRIED {
            refused_in_words(
                &world,
                action,
                &carried,
                Some(PERSON),
                opening,
                REACHES_ALPHA,
            );
        }
        assert_eq!(
            ground(&world),
            before,
            "{path}: the gate's refusal changes nothing"
        );
        granted_at(&world, &["control:root", "project:alpha"]);
        let before = alphas_tree(&world);
        for (action, _) in CARRIED {
            let refused = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(refused["ok"], false, "{path} {action}: {refused}");
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(
                words.contains(&format!(
                    "Invalid move journal: its link step at {path} names {path}, which is a scope's own folder or a folder \
                     above one"
                )),
                "{path} {action}: {words}"
            );
        }
        assert!(root.join(path).is_dir(), "{path} stands where it was");
        assert_eq!(
            alphas_tree(&world),
            before,
            "{path}: alpha's files are as they were"
        );
        assert!(
            root.join("Control/user/note.md").is_file(),
            "{path}: nothing moved"
        );
    }
}

/// Round 54i item 1: the same rule where no grant is in question. A link step of the root's own, edited to stand at a file of the
/// root's, or to stage a file where its plan staged a link, is refused by apply: a file is no link. The file stays, and nothing
/// moves. So is a step edited to land at a file after the move, and one at whose kept name a file stands (round 54j item 4).
#[test]
fn a_link_step_at_a_file_or_staging_a_file_is_refused() {
    for edit in [
        "the step stands at a file",
        "the step stages a file",
        "the step lands at a file",
        "a file stands at the step's kept name",
    ] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        root_link(&world, "Control/user/note-link");
        fs::write(root.join("Control/user/other.md"), "another page\n").unwrap();
        let carried = edited_move(&world, "Control/user/note2.md", |plan| {
            let staged = root.join(plan["links"][0]["staged"].as_str().unwrap());
            if edit == "the step stands at a file" {
                let other = identity(&root.join("Control/user/other.md"));
                plan["links"][0]["path"] = json!("Control/user/other.md");
                plan["links"][0]["after_path"] = json!("Control/user/other.md");
                plan["links"][0]["before"]["device"] = json!(other.0);
                plan["links"][0]["before"]["inode"] = json!(other.1);
            } else if edit == "the step lands at a file" {
                // Only where the link stands after the move is changed (the reviewer's mutant X4).
                plan["links"][0]["after_path"] = json!("Control/user/other.md");
            } else if edit == "a file stands at the step's kept name" {
                // The reviewer's mutant X5.
                let kept = root.join(plan["links"][0]["backup"].as_str().unwrap());
                fs::write(kept, "a file where the old link is kept\n").unwrap();
            } else {
                fs::remove_file(&staged).unwrap();
                fs::write(&staged, "a file where the plan staged a link\n").unwrap();
                let made = identity(&staged);
                plan["links"][0]["new_device"] = json!(made.0);
                plan["links"][0]["new_inode"] = json!(made.1);
            }
        });
        let before = ground_but_locks(&world);
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{edit}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains("which is not a link: a link step replaces a link and nothing else"),
            "{edit}: {words}"
        );
        assert_eq!(
            ground_but_locks(&world),
            before,
            "{edit}: nothing is moved or written"
        );
    }
}

/// A journal whose scopes stop reading once its Source has moved, with no link in any spelling: a project `standin` of the root's
/// own tree is one of the journal's scopes, and the journal moves that project's `ProjectCentral` folder away, so `standin` has no
/// declaration after the rename. The folder goes to `to`. Answers the world and the move's carried input. (Until round 54j this
/// case was reached through a link inside the moved folder, which is now refused as a spelling before the rename.)
fn scopes_that_do_not_read_once_moved(to: &str) -> (World, Value) {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Control/user/standin"), "standin");
    // The stand-in is a scope the ground holds, registered at the root: a journal runs from no other (file_map_moves.rs
    // `held_by_the_ground`), so its plan lists it.
    let registered = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"standin","path":"Control/user/standin","kind":"project","expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let declared = identity(&root.join("Control/user/standin/ProjectCentral"));
    let carried = edited_journal(&world, |plan| {
        plan["from"] = json!("Control/user/standin/ProjectCentral");
        plan["to"] = json!(to);
        plan["device"] = json!(declared.0);
        plan["inode"] = json!(declared.1);
        assert!(
            plan["worlds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|listed| listed["world"] == "project:standin"),
            "{plan}"
        );
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
    });
    (world, carried)
}

/// A journal's `state`, as it stands on the disk.
fn journal_state(world: &World, carried: &Value) -> String {
    let id = carried["plan_id"].as_str().unwrap();
    let journal = world.root().join(format!(".central/bkmr/moves/{id}.json"));
    let plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
    plan["state"].as_str().unwrap().to_owned()
}

/// Round 54i item 2 (B2): the rename is put back whenever the scopes are not the ones the gate saw. A journal moves away the
/// declaration of a project it runs from, so the scopes do not read after the rename. Apply puts the folder back and writes
/// nothing. The journal is left as it was, so a second apply answers the same way, in words, and a rollback has nothing to undo.
#[test]
fn a_journal_whose_scopes_do_not_read_after_the_rename_is_put_back() {
    let (world, carried) = scopes_that_do_not_read_once_moved("Control/pc2");
    let root = world.root();
    let declaration = root.join("Control/user/standin/ProjectCentral/project.json");
    let before = alphas_tree(&world);
    for attempt in ["the first apply", "a second apply"] {
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{attempt}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains("Move journal's scopes do not read")
                && words.contains("the Source is back where it stood and nothing else was written"),
            "{attempt}: {words}"
        );
        assert!(
            declaration.is_file() && !root.join("Control/pc2").exists(),
            "{attempt}: the folder is back where it stood"
        );
        assert_eq!(journal_state(&world, &carried), "prepared", "{attempt}");
    }
    let back = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(back["ok"], true, "a rollback reads the journal: {back}");
    assert!(declaration.is_file());
    assert_eq!(alphas_tree(&world), before, "nothing is written in alpha");
}

/// Round 54j item 2 (DLW build ruling 2026-11-11; the review of #346's third commit, C2): what stands after a put-back that failed,
/// for the two tests below. `first` is the refusal of the apply whose put-back failed; `free` takes away what made it fail. The
/// refusal says where the Source stands and that it must be moved back by hand, and the journal says "renamed". With the way back
/// still closed, a rollback cannot move the Source back and says the same; an apply is refused, since the scopes do not read. Once
/// the way is free, a rollback makes the rename back and nothing else, and says so; a second rollback then finds the scopes
/// reading and has nothing left to undo. Nothing is written in alpha at any point (`before` is alpha's tree from before the apply).
fn after_a_put_back_that_failed(
    world: &World,
    carried: &Value,
    (from, to): (&Path, &Path),
    first: &str,
    before: &[(PathBuf, Vec<u8>)],
    free: impl FnOnce(),
) {
    let moved = || to.join("project.json").is_file() && !from.join("project.json").exists();
    assert!(moved(), "the Source stands where the rename put it");
    assert_eq!(journal_state(world, carried), "renamed");
    let stuck = ctrl(world, CARRIED[1].0, carried, Some(PERSON));
    assert_eq!(stuck["ok"], false, "{stuck}");
    let stuck = stuck["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let again = ctrl(world, CARRIED[0].0, carried, Some(PERSON));
    assert_eq!(again["ok"], false, "{again}");
    assert!(moved(), "neither moved it");
    free();
    let back = ctrl(world, CARRIED[1].0, carried, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert!(
        from.join("project.json").is_file() && !to.exists(),
        "the Source is back where it stood"
    );
    assert_eq!(back["data"]["result"]["state"], "rolling-back", "{back}");
    let note = back["data"]["result"]["note"].as_str().unwrap_or_default();
    assert!(
        note.contains(
            "the Source is back at Control/user/standin/ProjectCentral, and nothing else was written"
        ),
        "{back}"
    );
    let done = ctrl(world, CARRIED[1].0, carried, Some(PERSON));
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(done["data"]["result"]["state"], "rolled-back", "{done}");
    assert_eq!(alphas_tree(world), before, "nothing is written in alpha");
    // What the two refusals said of the Source: where it stands, and that a hand must move it back.
    for (said, could_not) in [
        (first, "could not be put back"),
        (stuck.as_str(), "could not be moved back"),
    ] {
        assert!(
            said.contains(could_not)
                && said.contains(to.to_str().unwrap())
                && said.contains(from.to_str().unwrap())
                && said.contains("by hand"),
            "{said}"
        );
    }
}

/// A folder that takes new entries and gives none up while this stands (the `uappnd` flag of macOS and the BSDs, which the
/// folder's own user sets and clears): a rename into it goes through and the rename back out of it is refused by the system.
#[cfg(target_os = "macos")]
struct TakesAndKeeps(PathBuf);
#[cfg(target_os = "macos")]
impl TakesAndKeeps {
    fn flags(folder: &Path, flags: u32) {
        use std::os::unix::ffi::OsStrExt;
        let folder = std::ffi::CString::new(folder.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chflags(folder.as_ptr(), flags) }, 0);
    }
    fn at(folder: PathBuf) -> Self {
        fs::create_dir_all(&folder).unwrap();
        Self::flags(&folder, libc::UF_APPEND);
        Self(folder)
    }
}
#[cfg(target_os = "macos")]
impl Drop for TakesAndKeeps {
    fn drop(&mut self) {
        Self::flags(&self.0, 0);
    }
}

/// Round 54j item 2 (C2): when the put-back itself fails, the journal is not dead ([`after_a_put_back_that_failed`]). Here the
/// put-back fails every time, with no second writer: the move's destination lies in a folder that takes entries and gives none up
/// ([`TakesAndKeeps`]), so the rename goes through and the rename back is refused by the system until the flag is cleared. macOS
/// only: Linux has no such flag a plain user may set, and there the race below is the test.
#[cfg(target_os = "macos")]
#[test]
fn a_put_back_that_fails_leaves_a_journal_that_a_rollback_still_reads() {
    let (world, carried) = scopes_that_do_not_read_once_moved("Control/kept/pc2");
    let root = world.root();
    let from = root.join("Control/user/standin/ProjectCentral");
    let to = root.join("Control/kept/pc2");
    let kept = TakesAndKeeps::at(root.join("Control/kept"));
    let before = alphas_tree(&world);
    let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let first = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(first.contains("could not be put back"), "{first}");
    after_a_put_back_that_failed(&world, &carried, (&from, &to), first, &before, || {
        drop(kept)
    });
}

/// Round 54j item 2 (C2), as the review reached it: while apply runs, another hand takes the Source's old name the moment the
/// rename frees it, so the put-back is refused by the system ([`after_a_put_back_that_failed`] is asked of every such world). The
/// other hand is two threads of this test's that make a folder at the old name as soon as it is free. Whether one gets there before
/// apply's own put-back is a race, so this test is run by name and never by the suite: it makes 20 worlds, prints how many the
/// other hand won, and fails only where a world it won answers wrongly, or where it won none.
#[test]
#[ignore = "a race by nature: another hand must take the freed name before apply's own put-back does; run by name"]
fn a_put_back_that_loses_a_race_leaves_a_journal_that_a_rollback_still_reads() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut won = 0;
    for _ in 0..20 {
        let (world, carried) = scopes_that_do_not_read_once_moved("Control/pc2");
        let root = world.root();
        let from = root.join("Control/user/standin/ProjectCentral");
        let to = root.join("Control/pc2");
        let before = alphas_tree(&world);
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let hands: Vec<_> = (0..2)
            .map(|_| {
                let (stop, from) = (stop.clone(), from.clone());
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if fs::create_dir(&from).is_ok() {
                            return;
                        }
                    }
                })
            })
            .collect();
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        stop.store(true, Ordering::Relaxed);
        for hand in hands {
            hand.join().unwrap();
        }
        assert_eq!(refused["ok"], false, "{refused}");
        let first = refused["error"]["message"].as_str().unwrap_or_default();
        if !first.contains("could not be put back") {
            // Apply's own put-back got there first: the case of the test above the last.
            assert!(from.join("project.json").is_file(), "{first}");
            continue;
        }
        won += 1;
        after_a_put_back_that_failed(&world, &carried, (&from, &to), first, &before, || {
            fs::remove_dir(&from).unwrap()
        });
    }
    println!("the other hand won {won} of 20 worlds, and each of those answered as asked");
    assert!(
        won > 0,
        "the other hand never took the name before apply's put-back in 20 worlds"
    );
}

/// Round 54i item 4 (B4): a `project.json` that is there and is no ordinary file, a link here, makes its Work folder unreachable in
/// words, as one that does not open does; a move's plan is refused in them. And the root's pool does not stop without a word at a
/// folder of the root's own whose `project.json` is a link: its files stay the root's pooled Sources.
#[test]
fn a_declaration_that_is_a_link_is_said_and_stops_no_pool_of_the_roots() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let source = page_source(&world);
    let kit = root.join("Shelf/kit");
    fs::create_dir_all(kit.join("ProjectCentral")).unwrap();
    fs::write(kit.join("a.md"), "a page of the root's\n").unwrap();
    fs::write(kit.join("kept.json"), "{}").unwrap();
    std::os::unix::fs::symlink("../kept.json", kit.join("ProjectCentral/project.json")).unwrap();
    let pooled = ctrl(
        &world,
        "central.file-map.pool",
        &json!({"enable":true,"expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(pooled["ok"], true, "{pooled}");
    let located = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"path":"Shelf/kit/a.md"}),
        None,
    );
    assert_eq!(
        located["data"]["result"]["source"]["provenance"], "pooled",
        "the root's pool goes on past a folder whose project.json is a link: {located}"
    );
    let declaration = root.join("Work/alpha/ProjectCentral/project.json");
    fs::rename(
        &declaration,
        root.join("Work/alpha/ProjectCentral/kept.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink("kept.json", &declaration).unwrap();
    let said = "the Work project alpha does not read its declaration, Work/alpha/ProjectCentral/project.json: \
        Participating Project manifest is not an ordinary file";
    let inspected = ctrl(
        &world,
        "central.file-map.inspect",
        &json!({"resources":false}),
        None,
    );
    assert_eq!(inspected["ok"], true, "{inspected}");
    assert!(
        inspected["data"]["unreachable"].to_string().contains(said),
        "{inspected}"
    );
    let refused = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(said),
        "{refused}"
    );
}

/// Round 54i item 5 (the reviewer's mutant B1m): apply reads a link step's staged and kept links where they stand, with no link in
/// a folder above them. Alpha links the root's page, and a move of the page stages alpha's new link in alpha's moves folder. That
/// folder is then put elsewhere and a link left in its place. On both grants apply refuses the link in the path, and the page
/// stays where it was.
#[test]
fn a_link_step_s_staged_link_behind_a_link_is_refused() {
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
    let carried = edited_move(&world, "Control/user/note2.md", |plan| {
        assert_eq!(plan["links"][0]["world"], "project:alpha", "{plan}");
    });
    let moves = root.join("Work/alpha/.central/bkmr/moves");
    fs::rename(&moves, root.join("Control/user/moved-moves")).unwrap();
    std::os::unix::fs::symlink("../../../../Control/user/moved-moves", &moves).unwrap();
    let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains("refusing symlink source path component"),
        "{words}"
    );
    assert!(
        root.join("Control/user/note.md").is_file(),
        "the page stays"
    );
    assert_eq!(
        fs::read_link(root.join("Work/alpha/links/note")).unwrap(),
        Path::new("../../../Control/user/note.md"),
        "alpha's link is as it was"
    );
}

/// Round 54i item 5 (the reviewer's mutants B2m and B3m): a journal's scope outside the ground is spelled by the folder's own
/// absolute path, as an external root is opened. One spelled through a link to it, or with `..` in it, is refused as an invalid
/// journal, and nothing moves.
#[test]
fn a_journal_scope_outside_the_ground_is_spelled_by_its_own_path() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    declare_outside_scopes(&world);
    let far = fs::canonicalize(world.0.join("far")).unwrap();
    std::os::unix::fs::symlink(&far, world.0.join("far-by-a-link")).unwrap();
    let by_a_link = fs::canonicalize(&world.0).unwrap().join("far-by-a-link");
    let dotted = far.parent().unwrap().join("far/../far");
    // And with `..` after a name that is not there, which no reading of the disk can make canonical.
    let dotted_past_nothing = far.parent().unwrap().join("nothing-here/../far");
    for spelled in [by_a_link, dotted, dotted_past_nothing] {
        let spelled = spelled.to_str().unwrap().to_owned();
        let carried = edited_journal(&world, |plan| {
            let scopes = plan["worlds"].as_array_mut().unwrap();
            let scope = scopes
                .iter_mut()
                .find(|scope| scope["world"] == "project:far")
                .expect("the journal lists the scope outside the ground");
            assert_eq!(scope["external"], true, "{scope}");
            scope["path"] = json!(spelled);
        });
        for (action, opening) in CARRIED {
            refused_as(
                &world,
                action,
                &carried,
                Some(PERSON),
                "central.file_map_failure",
                opening,
                &format!("Invalid move journal: it spells the scope project:far as {spelled}"),
            );
        }
        assert!(
            root.join("Control/user/note.md").is_file(),
            "{spelled}: nothing moved"
        );
    }
}

/// A link step for the journal `id`, as `plan` writes one: the `index`th, in `world_ref`, at `path`, replacing the entry whose
/// device and inode are `before` by the staged entry whose device and inode are `staged`.
fn link_step(
    world_ref: &str,
    id: &str,
    index: usize,
    path: &str,
    before: (u64, u64),
    staged: (u64, u64),
) -> Value {
    json!({"world":world_ref,"path":path,"after_path":path,
        "before":{"source_ref":"x","world_ref":world_ref,"owner":"person","target":"x","device":before.0,"inode":before.1},
        "staged":format!(".central/bkmr/moves/{id}-{index}-new"),"backup":format!(".central/bkmr/moves/{id}-{index}-old"),
        "new_device":staged.0,"new_inode":staged.1})
}

/// The four spellings of the review's route C1: how the journal spells the scope, where its link step stands, and whether the
/// scope's link leads through a second one (`door -> mid -> standin`).
const THROUGH_A_LINK: [(&str, &str, bool); 4] = [
    ("Control/user/door", "Control/user/door", false),
    ("Control/user/door", "Control/user/DOOR", false),
    ("Control/user/DOOR", "Control/user/door", false),
    ("Control/user/door", "Control/user/mid", true),
];

/// The review's route C1: a journal lists a scope `project:standin` spelled through a link of the root's own, `Control/user/door`,
/// which leads to a stand-in project in the root's tree, so the root's grant is enough for all the gate reads. A link step of the
/// root's then replaces that link (or `mid`, the link it leads through) by a staged link to another Work project, beta, and the
/// journal's relations document for `project:standin` would be written wherever the link then leads. Answers the world and the
/// carried input, or none where this volume does not read the spelling (another case, on a volume that tells cases apart).
fn scope_through_a_link(
    scope_spelled: &str,
    step_spelled: &str,
    chain: bool,
) -> Option<(World, Value)> {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work/beta"), "beta");
    fs::write(root.join("Work/beta/plan.md"), "beta's plan\n").unwrap();
    let standin = root.join("Control/user/standin");
    project(&standin, "standin");
    std::os::unix::fs::symlink("ProjectCentral/project.json", standin.join("l0")).unwrap();
    if chain {
        std::os::unix::fs::symlink("standin", root.join("Control/user/mid")).unwrap();
        std::os::unix::fs::symlink("mid", root.join("Control/user/door")).unwrap();
    } else {
        std::os::unix::fs::symlink("standin", root.join("Control/user/door")).unwrap();
    }
    if !root.join(step_spelled).exists() || !root.join(scope_spelled).exists() {
        return None;
    }
    let swapped = identity(&root.join(if chain {
        "Control/user/mid"
    } else {
        "Control/user/door"
    }));
    let kept = identity(&standin.join("l0"));
    let carried = edited_journal(&world, |plan| {
        let id = plan["id"].as_str().unwrap().to_owned();
        fs::create_dir_all(standin.join(".central/bkmr/moves")).unwrap();
        let staged = standin.join(format!(".central/bkmr/moves/{id}-0-new"));
        std::os::unix::fs::symlink("ProjectCentral/project.json", &staged).unwrap();
        let to_beta = root.join(format!(".central/bkmr/moves/{id}-1-new"));
        std::os::unix::fs::symlink("../../Work/beta", &to_beta).unwrap();
        plan["worlds"]
            .as_array_mut()
            .unwrap()
            .push(json!({"world":"project:standin",
            "path":scope_spelled,"external":false,"project":"standin"}));
        let links = plan["links"].as_array_mut().unwrap();
        links.push(link_step(
            "project:standin",
            &id,
            0,
            "l0",
            kept,
            identity(&staged),
        ));
        links.push(link_step(
            "control:root",
            &id,
            1,
            step_spelled,
            swapped,
            identity(&to_beta),
        ));
        plan["documents"].as_array_mut().unwrap().push(json!({"world":"project:standin","path":null,
            "before":null,"after":"{\"written\":\"through the root's link, on the root's grant alone\"}"}));
    });
    Some((world, carried))
}

/// Round 54j item 1 (DLW build ruling 2026-11-11; the review of #346's third commit, C1): a journal's scope in the ground is
/// spelled as the disk spells the folder it reads as, with no link in it and no other case of a name. Each spelling of the route
/// (the exact one, the step in another case, the scope in another case, and a chain of two links) is refused on the root's grant
/// alone, at the gate, as a spelling: apply and rollback both, and nothing in the world changes.
#[test]
fn a_journal_scope_spelled_through_a_link_of_the_roots_is_refused_as_a_spelling() {
    for (scope_spelled, step_spelled, chain) in THROUGH_A_LINK {
        let Some((world, carried)) = scope_through_a_link(scope_spelled, step_spelled, chain)
        else {
            continue;
        };
        let case = format!("scope {scope_spelled}, step {step_spelled}");
        let before = ground(&world);
        for (action, opening) in CARRIED {
            let refused = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(refused["ok"], false, "{case} {action}: {refused}");
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(
                words.starts_with(opening)
                    && words.contains(&format!(
                        "This move's journal spells the scope project:standin as {scope_spelled}, which is not that \
                         folder's own spelling on the disk now"
                    )),
                "{case} {action}: {words}"
            );
        }
        assert_eq!(
            ground(&world),
            before,
            "{case}: nothing is moved or written"
        );
    }
}

/// Round 54j item 1, by what is left on the disk and whatever words refuse it: for each spelling of the same route, apply is
/// refused on the root's grant alone, and after it and after a rollback nothing is written in beta, the root's link still leads to
/// the stand-in, the stand-in's own link is the one it was, and the person's page has not moved. This is the test that holds when one of the
/// guards is taken away: the spelling rule, the rule that a step's path is no scope's folder, and the scopes held again after each
/// link step each stop the route alone.
#[test]
fn a_journal_scope_spelled_through_a_link_of_the_roots_writes_nothing_in_another_project() {
    for (scope_spelled, step_spelled, chain) in THROUGH_A_LINK {
        let Some((world, carried)) = scope_through_a_link(scope_spelled, step_spelled, chain)
        else {
            continue;
        };
        let root = world.root();
        let case = format!("scope {scope_spelled}, step {step_spelled}");
        let beta = |world: &World| -> Vec<(PathBuf, Vec<u8>)> {
            let beta = world.root().join("Work/beta");
            ground_but_locks(world)
                .into_iter()
                .filter(|(path, _)| path.starts_with(&beta))
                .collect()
        };
        let before = beta(&world);
        let kept = identity(&root.join("Control/user/standin/l0"));
        for (action, _) in CARRIED {
            let answer = ctrl(&world, action, &carried, Some(PERSON));
            // Apply is refused by whichever guard meets the journal first. A rollback is refused by the spelling rule; were
            // that rule not there, it would find nothing carried and answer that nothing was undone. Either way it is held
            // to what it leaves, as apply is.
            if action == CARRIED[0].0 {
                assert_eq!(answer["ok"], false, "{case} {action}: {answer}");
            }
            assert_eq!(
                beta(&world),
                before,
                "{case} {action}: nothing is written in beta"
            );
            assert!(
                !root.join("Work/beta/ProjectCentral/relations").exists(),
                "{case} {action}"
            );
            assert_eq!(
                fs::canonicalize(root.join("Control/user/door")).unwrap(),
                fs::canonicalize(root.join("Control/user/standin")).unwrap(),
                "{case} {action}: the root's link leads where it led"
            );
            assert_eq!(
                identity(&root.join("Control/user/standin/l0")),
                kept,
                "{case} {action}: the stand-in's link is the one it was"
            );
            assert!(
                root.join("Control/user/note.md").is_file()
                    && !root.join("Control/user/note2.md").exists(),
                "{case} {action}: the page stands where it stood"
            );
        }
    }
}

/// Round 54j item 1: a journal's scope in the ground that is no project and does not stand is refused as a spelling too. Nothing can
/// be read through it now, and a link made at that name later would decide where it reads. (A project that does not stand is named
/// in its own refusal: `a_project_a_journal_runs_from_whose_declaration_does_not_read_is_named_in_the_refusal`.)
#[test]
fn a_journal_scope_in_the_ground_that_does_not_stand_is_refused() {
    let world = granting(&every_act(), "control:root");
    let carried = edited_journal(&world, |plan| {
        plan["worlds"]
            .as_array_mut()
            .unwrap()
            .push(json!({"world":"folder:nowhere",
            "path":"Control/user/nowhere","external":false,"project":"nowhere"}));
    });
    let before = ground(&world);
    for (action, opening) in CARRIED {
        refused_as(
            &world,
            action,
            &carried,
            Some(PERSON),
            "central.file_map_failure",
            opening,
            "This move's journal spells the scope folder:nowhere as Control/user/nowhere, which is not that folder's own \
             spelling on the disk now",
        );
    }
    assert_eq!(ground(&world), before, "nothing is moved or written");
}

/// Round 54j item 2: a rollback makes the rename back alone only for a journal that says its rename was carried. The same journal
/// still "prepared", its Source moved by another hand to where the move would put it, is refused as before, naming the project that
/// does not read, and the Source stays where that hand put it.
#[test]
fn a_rollback_alone_is_for_a_journal_whose_rename_was_carried() {
    let (world, carried) = scopes_that_do_not_read_once_moved("Control/pc2");
    let root = world.root();
    fs::rename(
        root.join("Control/user/standin/ProjectCentral"),
        root.join("Control/pc2"),
    )
    .unwrap();
    assert_eq!(journal_state(&world, &carried), "prepared");
    let refused = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains("its journal runs from the project standin, which does not read"),
        "{words}"
    );
    assert!(
        root.join("Control/pc2/project.json").is_file()
            && !root.join("Control/user/standin/ProjectCentral").exists(),
        "the Source stays where the other hand put it"
    );
}

/// Round 54j item 3 (the review of #346's third commit, C3): `locate` answers in a Work project's words only for a place in a member
/// of Work. The root's relations name a project `folder:x` at a folder of the root's own; a project spelled so does not stand, and
/// is listed unreachable with its root as written, relative. A Source of the root's in that folder is located as the root's, from
/// a working directory in the ground as from one outside it, with the project listed; it is not "in Work/folder:x".
#[test]
fn a_roots_source_under_a_project_that_does_not_stand_is_located_as_the_roots() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Control/user/pdir")).unwrap();
    fs::write(root.join("Control/user/pdir/p.md"), "p\n").unwrap();
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"path":"Control/user/pdir/p.md","expected_revision":revision(&world, None)}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    edit_root_relations(&world, |document| {
        document["file_map"]["scopes"]["folder:x"] = json!("Control/user/pdir");
    });
    for cwd in [world.0.clone(), root.clone()] {
        let located = ctrl_at(
            &world,
            &cwd,
            "central.file-map.locate",
            &json!({"path":"Control/user/pdir/p.md"}),
            None,
            &[],
        );
        assert_eq!(located["ok"], true, "from {}: {located}", cwd.display());
        assert!(
            located["data"]["result"]["source"]["ref"]
                .as_str()
                .is_some_and(|reference| reference.ends_with("Control/user/pdir/p.md")),
            "from {}: {located}",
            cwd.display()
        );
        assert!(
            located["data"]["unreachable"]
                .to_string()
                .contains("a project named folder:x (Control/user/pdir) does not stand"),
            "from {}: {located}",
            cwd.display()
        );
    }
}

/// Round 54j item 4 (the reviewer's mutant X7): a place in a Work project that does not read is that project's to say by the
/// folder it names, not by its spelling alone. Asked by the firmlink spelling of the ground (macOS's `/System/Volumes/Data`, which
/// stays a spelling of its own when made canonical), a file in alpha is still "in Work/alpha, which is unreachable". Where the
/// ground has no such spelling, as on Linux, there is nothing to ask.
#[test]
fn a_place_in_a_project_that_does_not_read_is_located_by_the_folder_it_names() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let Some(firm) = firmlink(&root) else {
        return;
    };
    fs::write(
        root.join("Work/alpha/ProjectCentral/project.json"),
        "this is no declaration",
    )
    .unwrap();
    let asked = firm.join("Work/alpha/plan.md");
    let located = ctrl(
        &world,
        "central.file-map.locate",
        &json!({"path":asked.to_str().unwrap()}),
        None,
    );
    assert_eq!(located["ok"], false, "{located}");
    let words = located["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.starts_with("Location is in Work/alpha, which is unreachable: "),
        "{words}"
    );
}

/// Round 54j.1 (DLW build ruling 2026-11-11; the review of #346's fourth commit): a move the person plans is never refused by the
/// spelling rule for how they spelled where it goes. A folder that holds a registered project is moved to a destination whose
/// folders are spelled otherwise than the disk spells them: in another case (`control/USER/pdir2`), on a volume that folds case;
/// and with a letter in its decomposed form (`cafe` and a combining acute, for a folder made as `caf\u{e9}`), on a volume that
/// folds form. The plan answers, and writes in its journal, the disk's own spelling; the project inside is read where it went;
/// and the move applies, rolls back and applies again. Where the volume does not read the other spelling there is nothing to ask.
#[test]
fn a_destination_spelled_in_another_case_or_form_is_planned_as_the_disk_spells_it() {
    for (made, asked, written) in [
        ("Control/user", "control/USER/pdir2", "Control/user/pdir2"),
        (
            "Control/user/caf\u{e9}",
            "Control/user/cafe\u{301}/pdir2",
            "Control/user/caf\u{e9}/pdir2",
        ),
    ] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        fs::create_dir_all(root.join(made)).unwrap();
        let other = Path::new(asked).parent().unwrap();
        if !root.join(other).is_dir() {
            continue;
        }
        project(&root.join("Control/user/pdir/proj"), "proj");
        fs::write(root.join("Control/user/pdir/proj/p.md"), "proj's page\n").unwrap();
        let declared = ctrl(
            &world,
            "central.file-map.scope-register",
            &json!({"name":"proj","path":"Control/user/pdir/proj","kind":"project","expected_revision":revision(&world, None)}),
            Some(OWNER),
        );
        assert_eq!(declared["ok"], true, "{declared}");
        let registered = ctrl(
            &world,
            "central.file-map.register",
            &json!({"path":"Control/user/pdir","expected_revision":revision(&world, None)}),
            Some(PERSON),
        );
        assert_eq!(registered["ok"], true, "{registered}");
        let planned = ctrl(
            &world,
            "central.file-map.move-plan",
            &json!({"source_ref":registered["data"]["result"]["source_ref"],"destination":asked}),
            Some(PERSON),
        );
        assert_eq!(planned["ok"], true, "{asked}: {planned}");
        // The disk's spelling, as the disk gives it for the folder that was made (a volume may keep a name in another form
        // than it was made in; the destination's own new name is as asked).
        let disk = fs::canonicalize(root.join(made)).unwrap().join("pdir2");
        let disk = disk.strip_prefix(fs::canonicalize(&root).unwrap()).unwrap();
        assert_eq!(
            planned["data"]["result"]["destination"],
            disk.to_str().unwrap(),
            "{asked}: {planned}"
        );
        if made == "Control/user" {
            assert_eq!(disk, Path::new(written));
        }
        let carried = json!({"plan_id":planned["data"]["result"]["plan_id"],"quiesced":true});
        let moved = || {
            root.join(written).join("proj/p.md").is_file()
                && !root.join("Control/user/pdir").exists()
        };
        for (action, there) in [
            (CARRIED[0].0, true),
            (CARRIED[1].0, false),
            (CARRIED[0].0, true),
        ] {
            let answer = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(answer["ok"], true, "{asked} {action}: {answer}");
            assert_eq!(moved(), there, "{asked} {action}");
        }
        assert_eq!(
            ctrl(
                &world,
                "central.file-map.inspect",
                &json!({"project":"proj","resources":false}),
                None
            )["ok"],
            true,
            "{asked}: the live map reads, with proj where it now stands"
        );
    }
}

/// A folder of the root's that holds a registered project `proj`, registered as a Source. Answers its SourceRef.
fn folder_holding_a_project(world: &World) -> Value {
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
    registered["data"]["result"]["source_ref"].clone()
}

/// Whether the live map reads, with `proj` in it.
fn the_map_reads_proj(world: &World) -> bool {
    ctrl(
        world,
        "central.file-map.inspect",
        &json!({"project":"proj","resources":false}),
        None,
    )["ok"]
        == true
}

/// Round 54j.1 (the review of #346's fourth commit, D1, its probe as an assertion): the same move, to a destination whose folders
/// are typed in another case, when it does not run straight through. Interrupted after the rename, it is finished by a second
/// apply. Left mid-move by another hand (the folder at the destination, the journal saying "renamed"), it is finished by apply.
/// After each the live map reads, with the project where it went; and a rollback then undoes the move and the map reads again.
#[test]
fn a_move_to_a_destination_spelled_in_another_case_is_finished_and_undone_from_mid_move() {
    for asked in ["control/user/pdir2", "Control/USER/pdir2"] {
        for left in [
            "interrupted after the rename",
            "left renamed by another hand",
        ] {
            let world = granting(&every_act(), "control:root");
            let root = world.root();
            if !root.join(asked).parent().unwrap().is_dir() {
                continue;
            }
            let case = format!("{asked}, {left}");
            let source = folder_holding_a_project(&world);
            let planned = ctrl(
                &world,
                "central.file-map.move-plan",
                &json!({"source_ref":source,"destination":asked}),
                Some(PERSON),
            );
            assert_eq!(planned["ok"], true, "{case}: {planned}");
            let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
            let carried = json!({"plan_id":id,"quiesced":true});
            if left == "interrupted after the rename" {
                let interrupted = ctrl_with(
                    &world,
                    CARRIED[0].0,
                    &carried,
                    Some(PERSON),
                    &[("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME", "1")],
                );
                assert_eq!(interrupted["ok"], false, "{case}: {interrupted}");
            } else {
                fs::rename(root.join("Control/user/pdir"), root.join(asked)).unwrap();
                let journal = root.join(format!(".central/bkmr/moves/{id}.json"));
                let mut plan: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
                plan["state"] = json!("renamed");
                fs::write(&journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
            }
            assert!(
                root.join("Control/user/pdir2/proj/p.md").is_file(),
                "{case}"
            );
            let finished = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
            assert_eq!(finished["ok"], true, "{case}: {finished}");
            assert!(
                the_map_reads_proj(&world),
                "{case}: the map reads after the apply"
            );
            let undone = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
            assert_eq!(undone["ok"], true, "{case}: {undone}");
            assert!(
                root.join("Control/user/pdir/proj/p.md").is_file()
                    && !root.join("Control/user/pdir2").exists(),
                "{case}: the folder is back"
            );
            assert!(
                the_map_reads_proj(&world),
                "{case}: the map reads after the rollback"
            );
        }
    }
}

/// Round 54j.1 (D1): a journal as a `ctrl` before this rule planned it, its destination and the relations it would write spelled
/// as the person typed them (`control/USER/pdir2`; the journal is edited to that here, in its `to` and in its documents). Carried
/// out, that journal leaves relations the map does not read, as it did before this rule. So apply refuses it in words that say
/// what to do and blame no one, with nothing moved; and where it was already begun (the folder at the destination, the journal
/// saying "renamed") apply refuses it the same way and a rollback goes through: the folder is back and the map reads.
#[test]
fn a_journal_planned_with_its_destination_in_another_case_is_rolled_back_and_not_carried_out() {
    for left in ["prepared", "renamed"] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        if !root.join("control/USER").is_dir() {
            return;
        }
        let source = folder_holding_a_project(&world);
        let planned = ctrl(
            &world,
            "central.file-map.move-plan",
            &json!({"source_ref":source,"destination":"Control/user/pdir2"}),
            Some(PERSON),
        );
        assert_eq!(planned["ok"], true, "{planned}");
        let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
        let journal = root.join(format!(".central/bkmr/moves/{id}.json"));
        let mut plan: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
        assert_eq!(plan["to"], "Control/user/pdir2", "{plan}");
        plan["to"] = json!("control/USER/pdir2");
        for document in plan["documents"].as_array_mut().unwrap() {
            let after = document["after"].as_str().unwrap();
            assert!(after.contains("Control/user/pdir2/proj"), "{after}");
            document["after"] = json!(after.replace("Control/user/pdir2", "control/USER/pdir2"));
        }
        if left == "renamed" {
            plan["state"] = json!("renamed");
            fs::rename(
                root.join("Control/user/pdir"),
                root.join("Control/user/pdir2"),
            )
            .unwrap();
        }
        fs::write(&journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
        let carried = json!({"plan_id":id,"quiesced":true});
        let stands = root.join(if left == "renamed" {
            "Control/user/pdir2/proj/p.md"
        } else {
            "Control/user/pdir/proj/p.md"
        });
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{left}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains(
                "This move was planned with its destination spelled control/USER/pdir2, where the disk spells it \
                 Control/user/pdir2"
            ) && words.contains("then plan it again; nothing was written")
                && !words.contains("Invalid move journal"),
            "{left}: {words}"
        );
        assert!(stands.is_file(), "{left}: apply moved nothing");
        let undone = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
        assert_eq!(undone["ok"], true, "{left}: {undone}");
        assert!(
            root.join("Control/user/pdir/proj/p.md").is_file()
                && !root.join("Control/user/pdir2").exists(),
            "{left}: the folder stands where it stood"
        );
        assert!(the_map_reads_proj(&world), "{left}: the map reads");
    }
}

/// A planned move of the person's page that writes in alpha: alpha links the page, so the move rewrites alpha's link and its
/// relations. Planned on the root's grant and alpha's, which the world is left holding. Answers the carried input and where the
/// journal and alpha's seal of it stand.
fn a_move_that_writes_in_alpha(world: &World) -> (Value, PathBuf, PathBuf) {
    let root = world.root();
    granted_at(world, &["control:root", "project:alpha"]);
    let source = page_source(world);
    let linked = ctrl(
        world,
        "central.file-map.link",
        &json!({"project":"alpha","source_ref":source,"path":"links/note","owner":"person",
            "expected_revision":revision(world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(linked["ok"], true, "{linked}");
    let planned = ctrl(
        world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    (
        json!({"plan_id":id,"quiesced":true}),
        root.join(format!(".central/bkmr/moves/{id}.json")),
        root.join(format!("Work/alpha/.central/bkmr/moves/{id}.seal.json")),
    )
}

/// The journal at `journal`, changed by `edit` as a hand that can write in the root's `.central` could change it.
fn edit_journal(journal: &Path, edit: impl FnOnce(&mut Value)) {
    let mut plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
    edit(&mut plan);
    fs::write(journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
}

/// Round 54k (DLW build ruling 2026-11-11): `move-plan` seals its plan in each Work project the move writes in. The seal is a file
/// of the project's own, `.central/bkmr/moves/<id>.seal.json`, mode 0600 in a folder 0700, holding the plan's id and digest. A
/// move that writes in no Work project seals none. The seal lives as long as its journal: it stays while the move is applied,
/// since a rollback needs it, and when the move is rolled back, since the same journal can be applied again.
#[test]
fn a_plan_is_sealed_in_each_work_project_the_move_writes_in() {
    use std::os::unix::fs::PermissionsExt;
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let (carried, journal, seal) = a_move_that_writes_in_alpha(&world);
    let plan: Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    assert_eq!(plan["schema"], "central.file-map-move/v2", "{plan}");
    let held: Value = serde_json::from_slice(&fs::read(&seal).unwrap()).unwrap();
    assert_eq!(
        (&held["plan_id"], &held["project"]),
        (&plan["id"], &json!("alpha")),
        "{held}"
    );
    assert!(
        held["digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
        "{held}"
    );
    let mode = |path: &Path| fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&seal), 0o600);
    assert_eq!(mode(seal.parent().unwrap()), 0o700);
    let applied = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        fs::canonicalize(root.join("Work/alpha/links/note")).unwrap(),
        fs::canonicalize(root.join("Control/user/note2.md")).unwrap(),
        "alpha's link follows the page"
    );
    assert!(seal.is_file(), "the seal stays while the move is applied");
    let undone = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(undone["ok"], true, "{undone}");
    assert!(
        seal.is_file(),
        "the seal stays when the move is rolled back"
    );
    let again = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(
        again["ok"], true,
        "a rolled-back move is applied again: {again}"
    );
    assert_eq!(
        fs::canonicalize(root.join("Work/alpha/links/note")).unwrap(),
        fs::canonicalize(root.join("Control/user/note2.md")).unwrap(),
        "alpha's link follows the page again"
    );
    // A move that writes in no Work project seals none.
    let plain = granting(&every_act(), "control:root");
    let planned = ctrl(
        &plain,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(&plain),"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    assert!(
        !plain.root().join("Work/alpha/.central/bkmr/moves").exists(),
        "no seal in a project the move does not write in"
    );
}

/// Round 54k: a journal that is not the plan a project holds writes nothing in that project, whatever grants stand. With the
/// root's grant and alpha's, the journal is edited where no other rule looks (the bytes alpha's relations are to become); apply
/// and rollback are refused in words that name alpha and its seal, nothing of alpha's changes, and the page does not move.
#[test]
fn an_edited_journal_is_not_the_plan_a_project_holds_and_writes_nothing_in_it() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let (carried, journal, seal) = a_move_that_writes_in_alpha(&world);
    edit_journal(&journal, |plan| {
        let document = plan["documents"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|document| document["world"] == "project:alpha")
            .expect("the plan rewrites alpha's relations");
        document["after"] = json!(format!("{} ", document["after"].as_str().unwrap()));
    });
    let before = alphas_tree(&world);
    for (action, _) in CARRIED {
        let refused = ctrl(&world, action, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{action}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains("This move's journal is not the plan the Work project alpha holds")
                && words.contains(seal.strip_prefix(&root).unwrap().to_str().unwrap())
                && words.contains("plan the move again"),
            "{action}: {words}"
        );
        assert_eq!(
            alphas_tree(&world),
            before,
            "{action}: nothing of alpha's changes"
        );
        assert!(
            root.join("Control/user/note.md").is_file(),
            "{action}: the page stands"
        );
    }
}

/// Round 54k: the seal is read as the project's own ordinary file, under the project's own folders. The plan is as `move-plan`
/// wrote it; the seal is missing, or holds another digest, or is a link to a true copy kept elsewhere, or stands in a moves folder
/// that is a link. Each time apply is refused naming alpha, with both grants, and nothing of alpha's changes.
#[test]
fn a_seal_is_the_projects_own_file_or_the_move_writes_nothing_there() {
    for shape in [
        "missing",
        "another digest",
        "a link to a copy",
        "its folder is a link",
    ] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        let (carried, _, seal) = a_move_that_writes_in_alpha(&world);
        let moves = seal.parent().unwrap().to_path_buf();
        match shape {
            "missing" => fs::remove_file(&seal).unwrap(),
            "another digest" => {
                let mut held: Value = serde_json::from_slice(&fs::read(&seal).unwrap()).unwrap();
                held["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
                fs::write(&seal, serde_json::to_vec(&held).unwrap()).unwrap();
            }
            "a link to a copy" => {
                let copy = root.join("Control/user/copied.seal.json");
                fs::rename(&seal, &copy).unwrap();
                std::os::unix::fs::symlink(&copy, &seal).unwrap();
            }
            _ => {
                let kept = root.join("Control/user/moves-kept-elsewhere");
                fs::rename(&moves, &kept).unwrap();
                std::os::unix::fs::symlink(&kept, &moves).unwrap();
            }
        }
        let before = alphas_tree(&world);
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{shape}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        // A moves folder that is a link is met first by the rule that a link step's staged link stands behind no link, which
        // refuses it in its own words; the other three are the seal's to refuse.
        assert!(
            words.contains("is not the plan the Work project alpha holds")
                || (shape == "its folder is a link"
                    && words.contains("refusing symlink source path component")),
            "{shape}: {words}"
        );
        assert_eq!(
            alphas_tree(&world),
            before,
            "{shape}: nothing of alpha's changes"
        );
        assert!(
            root.join("Control/user/note.md").is_file(),
            "{shape}: the page stands"
        );
    }
}

/// Round 54k: a journal planned before plans were sealed (schema v1, no seal) is carried out where it writes in no Work project,
/// and its apply is refused where it would write in one: the refusal names the project, says where the Source stands and what
/// goes on from there. Here the same journal is made v1 with its seal taken away, still "prepared"; then a v1 journal of a move
/// that stays in the root's own tree is applied and rolled back as before.
#[test]
fn a_journal_from_before_seals_writes_in_no_work_project_and_says_what_is_left() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let (carried, journal, seal) = a_move_that_writes_in_alpha(&world);
    edit_journal(&journal, |plan| {
        plan["schema"] = json!("central.file-map-move/v1")
    });
    fs::remove_file(&seal).unwrap();
    let before = alphas_tree(&world);
    let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains(
            "This move was planned before a plan was kept in each Work project it writes in"
        ) && words.contains("is in the Work project alpha")
            && words.contains("nothing of the move stands: plan the move again"),
        "{words}"
    );
    assert_eq!(alphas_tree(&world), before, "nothing of alpha's changes");
    assert!(
        root.join("Control/user/note.md").is_file(),
        "the page stands"
    );
    // Its rollback finds nothing carried, writes nothing in alpha and closes the journal (every state:
    // `a_journal_from_before_seals_is_rolled_back_from_every_state_and_says_what_a_hand_must_do`).
    let back = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert_eq!(back["data"]["result"]["state"], "rolled-back", "{back}");
    assert_eq!(alphas_tree(&world), before, "nothing of alpha's changes");
    let plain = granting(&every_act(), "control:root");
    let planned = ctrl(
        &plain,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(&plain),"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    edit_journal(
        &plain.root().join(format!(".central/bkmr/moves/{id}.json")),
        |plan| plan["schema"] = json!("central.file-map-move/v1"),
    );
    let carried = json!({"plan_id":id,"quiesced":true});
    for (action, there) in [(CARRIED[0].0, true), (CARRIED[1].0, false)] {
        let answer = ctrl(&plain, action, &carried, Some(PERSON));
        assert_eq!(answer["ok"], true, "{action}: {answer}");
        assert_eq!(
            plain.root().join("Control/user/note2.md").is_file(),
            there,
            "{action}"
        );
    }
}

/// Round 54k (the review of #346's fourth commit, D2): a journal edited to name a project outside the ground as the move's owner
/// is carried out no more than `move-plan` would plan it. Apply and rollback are refused in words, and nothing moves in `far`.
#[test]
fn a_journal_whose_owner_is_outside_the_ground_moves_nothing_there() {
    let world = granting(&every_act(), "control:root");
    declare_outside_scopes(&world);
    let far = fs::canonicalize(world.0.join("far")).unwrap();
    fs::create_dir_all(far.join("docs")).unwrap();
    fs::write(far.join("docs/in.md"), "far's page\n").unwrap();
    let docs = identity(&far.join("docs"));
    let carried = edited_journal(&world, |plan| {
        plan["world"] = json!("project:far");
        plan["from"] = json!("docs");
        plan["to"] = json!("docs2");
        plan["device"] = json!(docs.0);
        plan["inode"] = json!(docs.1);
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
    });
    for (action, _) in CARRIED {
        let refused = ctrl(&world, action, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{action}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains("names project:far as its owner") && words.contains("nothing was moved"),
            "{action}: {words}"
        );
        assert!(
            far.join("docs/in.md").is_file() && !far.join("docs2").exists(),
            "{action}: far's folder stands"
        );
    }
}

/// Every leaf of a journal, as a JSON pointer.
fn journal_fields(value: &Value, at: &str, fields: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                journal_fields(member, &format!("{at}/{key}"), fields);
            }
        }
        Value::Array(members) => {
            for (index, member) in members.iter().enumerate() {
                journal_fields(member, &format!("{at}/{index}"), fields);
            }
        }
        _ => fields.push(at.to_owned()),
    }
}

/// One field of a journal changed to something else of its own kind, chosen to lead toward alpha where the field can: a World
/// becomes another World (alpha's the root's, the root's the project's outside the ground, any other alpha's), a path a file of
/// alpha's, a staged or kept name the other of the two, any other text one character
/// longer, a number the next, a flag its opposite, nothing a text.
fn change_field(plan: &mut Value, field: &str) {
    let key = field.rsplit('/').next().unwrap_or_default().to_owned();
    let value = plan
        .pointer_mut(field)
        .expect("the field is in every such journal");
    *value = match &*value {
        Value::String(text) => json!(match key.as_str() {
            "world" | "world_ref" if text == "project:alpha" => "control:root".to_owned(),
            // The root becomes the project outside the ground: as the journal's owner, a scope `move-plan` plans no move in.
            "world" | "world_ref" if text == "control:root" => "project:far".to_owned(),
            "world" | "world_ref" => "project:alpha".to_owned(),
            "path" | "after_path" | "from" | "to" => "Work/alpha/plan.md".to_owned(),
            "staged" => text.replace("-new", "-old"),
            "backup" => text.replace("-old", "-new"),
            _ => format!("{text}x"),
        }),
        Value::Number(number) => json!(number.as_u64().unwrap_or_default().wrapping_add(1)),
        Value::Bool(flag) => json!(!flag),
        _ => json!("x"),
    };
}

/// The acceptance test (DLW build ruling 2026-11-11, and 2026-11-12 for what it asks): whatever one field of a valid journal is
/// changed to, and then whatever two, no entry under any Work project is new, changed or gone: no file's bytes, no link, no
/// folder, and no lock file or `.central` either (`works_tree`). This is its first family; the others are the tests below
/// that end in "under_work", and each prints how many journals it made. The journal is of a move that writes in alpha (a link step and a
/// relations document there, and the root's own), planned on both grants. Each changed journal is given to apply and to rollback
/// on the root's grant alone, and then on the root's grant and alpha's: the first is the rule this PR series set out to hold, the
/// second shows it holds by the seal and not by the missing grant. `state` is the one field a seal does not cover (apply and
/// rollback write it), so a journal changed in `state` alone is still the plan, and is asked on the root's grant only.
///
/// It makes one world for each field of the journal, and one for each of a fixed spread of pairs of fields; the count is printed.
#[test]
fn no_change_of_a_journals_fields_writes_in_a_work_project() {
    let fields = {
        let world = granting(&every_act(), "control:root");
        declare_outside_scopes(&world);
        let (_, journal, _) = a_move_that_writes_in_alpha(&world);
        let plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
        let mut fields = Vec::new();
        journal_fields(&plan, "", &mut fields);
        fields
    };
    assert!(
        fields.iter().any(|field| field.starts_with("/links/0/"))
            && fields
                .iter()
                .any(|field| field.starts_with("/documents/1/")),
        "the journal has a link step and two documents: {fields:?}"
    );
    let singles: Vec<Vec<&str>> = fields.iter().map(|field| vec![field.as_str()]).collect();
    let pairs: Vec<Vec<&str>> = (0..fields.len())
        .step_by(3)
        .map(|one| {
            let other = (one * 7 + 3) % fields.len();
            vec![fields[one].as_str(), fields[other].as_str()]
        })
        .filter(|pair| pair[0] != pair[1])
        .collect();
    let mut journals = 0;
    for changed in singles.iter().chain(&pairs) {
        let world = granting(&every_act(), "control:root");
        // A second Work project the move does not reach: it holds no `.central`, and none is made in it.
        project(&world.root().join("Work/beta"), "beta");
        // A folder scope and a project outside the ground stand in every world, so the journal lists them and a field can name them.
        declare_outside_scopes(&world);
        let (carried, journal, _) = a_move_that_writes_in_alpha(&world);
        edit_journal(&journal, |plan| {
            for field in changed {
                change_field(plan, field);
            }
        });
        journals += 1;
        let before = works_tree(&world);
        let state_alone = changed.iter().all(|field| *field == "/state");
        for grants in [vec!["control:root"], vec!["control:root", "project:alpha"]] {
            if state_alone && grants.len() == 2 {
                continue;
            }
            granted_at(&world, &grants);
            for (action, _) in CARRIED {
                let answer = ctrl(&world, action, &carried, Some(PERSON));
                assert_eq!(
                    works_changes(&world, &before),
                    Vec::<String>::new(),
                    "{changed:?}, {action} on {grants:?}: an entry under Work is new, changed or gone; the press answered \
                     {answer}"
                );
            }
        }
    }
    println!(
        "a journal alpha holds the plan of: {journals} journals, {} with one field changed, {} with two, no entry under Work \
         new, changed or gone",
        singles.len(),
        pairs.len()
    );
}

/// Round 54k (a follow-up of #346's reviews): a Work project as the move's owner, with the destination's folder typed in another
/// case. A folder of alpha's is moved inside alpha to `DOCS/sub2`, where the disk keeps `docs`. The plan answers the disk's
/// spelling, the move is sealed in alpha, and it applies, rolls back and applies again. Where the volume tells cases apart there is
/// nothing to ask.
#[test]
fn a_work_projects_own_move_to_a_destination_in_another_case_is_planned_as_the_disk_spells_it() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/alpha/docs/sub")).unwrap();
    fs::write(root.join("Work/alpha/docs/sub/in.md"), "in\n").unwrap();
    if !root.join("Work/alpha/DOCS").is_dir() {
        return;
    }
    granted_at(&world, &["control:root", "project:alpha"]);
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":"alpha","path":"docs/sub","expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"project":"alpha","source_ref":registered["data"]["result"]["source_ref"],"destination":"DOCS/sub2"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(
        planned["data"]["result"]["destination"], "docs/sub2",
        "{planned}"
    );
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    assert!(
        root.join(format!("Work/alpha/.central/bkmr/moves/{id}.seal.json"))
            .is_file(),
        "the move is sealed in alpha, where it writes"
    );
    let carried = json!({"plan_id":id,"quiesced":true});
    for (action, there) in [(CARRIED[0].0, true), (CARRIED[1].0, false)] {
        let answer = ctrl(&world, action, &carried, Some(PERSON));
        assert_eq!(answer["ok"], true, "{action}: {answer}");
        assert_eq!(
            root.join("Work/alpha/docs/sub2/in.md").is_file(),
            there,
            "{action}"
        );
    }
}

/// Everything under `Work`, strictly: every folder, every file with its bytes, every link with where it points, lock files and
/// `.central` folders too. What no press on a journal a project does not hold may change in any way.
fn works_tree(world: &World) -> Vec<(PathBuf, Vec<u8>)> {
    let work = world.root().join("Work");
    ground(world)
        .into_iter()
        .filter(|(path, _)| path.starts_with(&work))
        .collect()
}

/// What stands under `Work` now and did not in `before`, or differs, or is gone: the paths, below the ground.
fn works_changes(world: &World, before: &[(PathBuf, Vec<u8>)]) -> Vec<String> {
    let root = world.root();
    let was: std::collections::BTreeMap<_, _> = before.iter().cloned().collect();
    let is: std::collections::BTreeMap<_, _> = works_tree(world).into_iter().collect();
    let named = |sign: &str, path: &Path| {
        format!(
            "{sign}{}",
            path.strip_prefix(&root).unwrap_or(path).display()
        )
    };
    let mut changes: Vec<String> = is
        .iter()
        .filter_map(|(path, bytes)| match was.get(path) {
            None => Some(named("+", path)),
            Some(old) if old != bytes => Some(named("~", path)),
            _ => None,
        })
        .collect();
    changes.extend(
        was.keys()
            .filter(|path| !is.contains_key(*path))
            .map(|path| named("-", path)),
    );
    changes
}

/// A move of the person's page that writes in no Work project: the root keeps a link to the page, so the journal has a link step
/// and a relations document, both the root's. Planned on the root's grant alone, which the world is left holding; no project
/// holds a seal of it. Answers the carried input and where the journal stands.
fn a_move_of_the_roots_own(world: &World) -> (Value, PathBuf) {
    granted_at(world, &["control:root"]);
    let source = root_link(world, "Control/user/note-link");
    let planned = ctrl(
        world,
        "central.file-map.move-plan",
        &json!({"source_ref":source,"destination":"Control/user/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    (
        json!({"plan_id":id,"quiesced":true}),
        world.root().join(format!(".central/bkmr/moves/{id}.json")),
    )
}

/// A journal's apply, interrupted after its rename: the Source stands at its `to` and the journal says "renamed".
fn interrupt_after_the_rename(world: &World, carried: &Value) {
    let interrupted = ctrl_with(
        world,
        CARRIED[0].0,
        carried,
        Some(PERSON),
        &[("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME", "1")],
    );
    assert_eq!(interrupted["ok"], false, "{interrupted}");
    assert_eq!(journal_state(world, carried), "renamed");
}

/// The review of #349, F1 (DLW build ruling 2026-11-12): a journal's scopes are the ones its plan wrote, which are the ones the
/// ground holds. A move of the root's own, planned on the root's grant, is edited to list one more scope: a made-up folder scope
/// at a folder of alpha's, at alpha's `ProjectCentral/user`, or a project that stands in alpha's tree and was never registered;
/// or to say that a project the root does register stands at a twin of it in alpha's tree.
/// Before, apply and rollback took that scope's locks, and so made `.central` and two lock files in alpha on the root's grant
/// alone. Now each is refused in words where the gate reads the journal, on the root's grant and on every grant, apply first and
/// rollback first, from "prepared" and from an interrupted move, and nothing at all is new under `Work`: alpha keeps no
/// `.central`, since nothing of this move reaches it.
#[test]
fn a_scope_a_journal_makes_up_is_refused_and_makes_no_lock_folder_in_a_work_project() {
    for made_up in [
        json!({"world":"folder:zz","path":"Work/alpha/docs","external":false,"project":null}),
        json!({"world":"folder:zz","path":"Work/alpha/ProjectCentral/user","external":false,"project":"zz"}),
        json!({"world":"project:sub","path":"Work/alpha/vendor/sub","external":false,"project":"sub"}),
        // Not one more scope, but a scope the root does register, said to stand at a twin of it in alpha's tree.
        json!({"respelled":"project:standin","path":"Work/alpha/vendor/twin"}),
    ] {
        for interrupted in [false, true] {
            for order in [[0usize, 1], [1, 0]] {
                let world = granting(&every_act(), "control:root");
                let root = world.root();
                fs::create_dir_all(root.join("Work/alpha/docs")).unwrap();
                project(&root.join("Work/alpha/vendor/sub"), "sub");
                project(&root.join("Control/user/standin"), "standin");
                project(&root.join("Work/alpha/vendor/twin"), "standin");
                let registered = ctrl(
                    &world,
                    "central.file-map.scope-register",
                    &json!({"name":"standin","path":"Control/user/standin","kind":"project","expected_revision":revision(&world, None)}),
                    Some(OWNER),
                );
                assert_eq!(registered["ok"], true, "{registered}");
                let (carried, journal) = a_move_of_the_roots_own(&world);
                if interrupted {
                    interrupt_after_the_rename(&world, &carried);
                }
                edit_journal(&journal, |plan| {
                    let listed = plan["worlds"].as_array_mut().unwrap();
                    match made_up["respelled"].as_str() {
                        Some(respelled) => {
                            let scope = listed
                                .iter_mut()
                                .find(|scope| scope["world"] == respelled)
                                .expect("the journal lists the registered project");
                            scope["path"] = made_up["path"].clone();
                        }
                        None => listed.push(made_up.clone()),
                    }
                });
                let case = format!("{made_up}, interrupted {interrupted}, order {order:?}");
                assert!(
                    !root.join("Work/alpha/.central").exists(),
                    "{case}: a move of the root's own made no lock folder in alpha"
                );
                let before = works_tree(&world);
                for grants in [vec!["control:root"], vec!["control:root", "project:alpha"]] {
                    granted_at(&world, &grants);
                    for press in order {
                        let refused = ctrl(&world, CARRIED[press].0, &carried, Some(PERSON));
                        assert_eq!(refused["ok"], false, "{case}: {refused}");
                        let words = refused["error"]["message"].as_str().unwrap_or_default();
                        assert!(
                            words.contains("the ground holds no such scope there"),
                            "{case}: {words}"
                        );
                        assert_eq!(
                            works_changes(&world, &before),
                            Vec::<String>::new(),
                            "{case}, {} on {grants:?}: nothing is new under Work",
                            CARRIED[press].0
                        );
                    }
                }
            }
        }
    }
}

/// The review of #349, F1, the lock folder itself (DLW build ruling 2026-11-12): a move makes a lock folder in a Work project's
/// tree only where that project admitted the move. A move of the root's own is planned, applied and rolled back on the root's
/// grant in a ground with a Work project and a project nested in it and registered at the root: nothing is new under `Work`,
/// where before each of the three made `.central` and two lock files in both. A move that writes in alpha is sealed there: with
/// alpha's lock files taken away its apply makes them again, and the same journal once edited makes none.
#[test]
fn a_move_makes_a_lock_folder_in_a_work_project_only_where_the_project_holds_its_plan() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work/alpha/vendor/sub"), "sub");
    let declared = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"sub","path":"Work/alpha/vendor/sub","kind":"project","expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(declared["ok"], true, "{declared}");
    let before = works_tree(&world);
    let (carried, _) = a_move_of_the_roots_own(&world);
    assert_eq!(
        works_changes(&world, &before),
        Vec::<String>::new(),
        "the plan"
    );
    for (action, _) in CARRIED {
        let answer = ctrl(&world, action, &carried, Some(PERSON));
        assert_eq!(answer["ok"], true, "{action}: {answer}");
        assert_eq!(
            works_changes(&world, &before),
            Vec::<String>::new(),
            "{action}"
        );
    }
    // A move alpha holds the plan of makes alpha's lock files where they are not there.
    let (carried, journal, _) = a_move_that_writes_in_alpha(&world);
    let locks = [
        root.join("Work/alpha/.central/file-map.lock"),
        root.join("Work/alpha/.central/source-mutation.lock"),
    ];
    for lock in &locks {
        fs::remove_file(lock).unwrap();
    }
    let applied = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(applied["ok"], true, "{applied}");
    assert!(
        locks.iter().all(|lock| lock.is_file()),
        "a move alpha holds the plan of takes alpha's locks, and makes them"
    );
    // The same journal, edited: no longer the plan alpha holds, it makes no lock file there.
    for lock in &locks {
        fs::remove_file(lock).unwrap();
    }
    edit_journal(&journal, |plan| {
        plan["refresh_worlds"] = json!(["project:alpha"])
    });
    let before = works_tree(&world);
    let refused = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(
        works_changes(&world, &before),
        Vec::<String>::new(),
        "an edited journal makes no lock file in alpha"
    );
}

/// A world with alpha and beta in Work, a folder scope and a project outside the ground, and in alpha: a registered file (so
/// alpha has a relations document), a folder `docs` with a file, and a link of its own, `sl`. Beta holds a page and a link, and no
/// `.central`. The world is left holding the root's grant alone.
fn a_world_of_two_projects() -> World {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    project(&root.join("Work/beta"), "beta");
    fs::write(root.join("Work/beta/b.md"), "beta's page\n").unwrap();
    std::os::unix::fs::symlink("b.md", root.join("Work/beta/sl2")).unwrap();
    fs::create_dir_all(root.join("Work/alpha/docs")).unwrap();
    fs::write(root.join("Work/alpha/docs/in.md"), "in\n").unwrap();
    std::os::unix::fs::symlink("plan.md", root.join("Work/alpha/sl")).unwrap();
    declare_outside_scopes(&world);
    granted_at(&world, &["control:root", "project:alpha"]);
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":"alpha","path":"plan.md","expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    granted_at(&world, &["control:root"]);
    world
}

/// The content revision Central keeps of `bytes` (source_safety.rs `content_revision_bytes`), for a journal built by hand.
fn content_revision(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("central.content-fnv1a64/v1:{}:{hash:016x}", bytes.len())
}

/// A staged link made by hand in the root's own moves folder, as a hand that can write there could make one.
fn staged_by_hand(world: &World, id: &str, index: usize, target: &str) -> (u64, u64) {
    let staged = world
        .root()
        .join(format!(".central/bkmr/moves/{id}-{index}-new"));
    std::os::unix::fs::symlink(target, &staged).unwrap();
    identity(&staged)
}

/// What one field of a journal is changed to: values of its kind that could steer a write, the ones that lead to alpha first.
/// The review of #349 wrote these lists; the acceptance test takes the first `most` of each.
fn steering_values(world: &World, plan: &Value, field: &str, most: usize) -> Vec<Value> {
    let root = world.root();
    let key = field.rsplit('/').next().unwrap_or_default();
    let id = plan["id"].as_str().unwrap_or_default().to_owned();
    let alpha = fs::canonicalize(root.join("Work/alpha"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let inode = |path: &str| json!(identity(&root.join(path)).1);
    let texts = |values: &[&str]| {
        values
            .iter()
            .map(|value| json!(value))
            .collect::<Vec<Value>>()
    };
    let current = plan.pointer(field).cloned().unwrap_or(Value::Null);
    let next = |value: &Value| json!(value.as_u64().unwrap_or_default().wrapping_add(1));
    let mut values: Vec<Value> = match key {
        "world" | "world_ref" => texts(&[
            "project:alpha",
            "project:beta",
            "control:root",
            "project:far",
            "folder:Music",
            "project:nope",
        ]),
        "path" if field.starts_with("/worlds/") => {
            let mut paths = texts(&[
                "Work/alpha/docs",
                "Work/alpha",
                "Work/beta",
                "Work",
                "work/ALPHA",
                "",
                "Control/user",
            ]);
            paths.insert(2, json!(alpha));
            paths
        }
        "path" if field.starts_with("/documents/") => texts(&[
            ".central/flows.json",
            "ProjectCentral/relations/source-relations.json",
            "Control/relations/source-relations.json",
            "Work/alpha/plan.md",
            "plan.md",
        ]),
        "path" | "after_path" => texts(&[
            "Work/alpha/sl",
            "Work/alpha/newlink",
            "Work/alpha/plan.md",
            "Work/alpha",
            "Work/alpha/docs/newlink",
            "sl",
            "links/note",
            "Work/alpha/.central/bkmr/moves/x",
        ]),
        "from" | "to" => texts(&[
            "Work/alpha/docs",
            "Work/alpha/new.md",
            "Work/alpha/plan.md",
            "Work/alpha",
            "Work",
            "WORK/ALPHA/new2.md",
            "Work/alpha/ProjectCentral",
            "Work/beta/b.md",
        ]),
        "staged" | "backup" => {
            let text = current.as_str().unwrap_or_default();
            let swapped = if key == "staged" {
                text.replace("-new", "-old")
            } else {
                text.replace("-old", "-new")
            };
            vec![
                json!(swapped),
                json!(format!("Work/alpha/.central/bkmr/moves/{id}-0-new")),
                json!("Work/alpha/sl"),
            ]
        }
        "inode" | "new_inode" => vec![
            inode("Work/alpha/plan.md"),
            inode("Work/alpha/docs"),
            inode("Work/alpha"),
            inode("Work/alpha/sl"),
            next(&current),
        ],
        "project" => vec![
            json!("alpha"),
            json!("beta"),
            json!("far"),
            json!("x"),
            Value::Null,
        ],
        "state" => texts(&[
            "prepared",
            "renamed",
            "rolling-back",
            "applied",
            "rolled-back",
            "x",
        ]),
        "schema" => texts(&["central.file-map-move/v1", "central.file-map-move/v2x"]),
        "before" | "after" => vec![
            json!(format!("{} ", current.as_str().unwrap_or_default())),
            json!("{}"),
            if key == "before" {
                Value::Null
            } else {
                json!("")
            },
        ],
        "id" => vec![json!(format!("{id}0")), json!("abc")],
        _ => vec![match &current {
            Value::String(text) => json!(format!("{text}x")),
            Value::Number(_) => next(&current),
            Value::Bool(flag) => json!(!flag),
            _ => json!("x"),
        }],
    };
    values.retain(|value| *value != current);
    values.truncate(most);
    values
}

/// Where a changed journal names a thing that stands, its identity is made to follow, so the change is not refused for the
/// numbers alone: the Source at `from`, and the link at the first link step's path.
fn identities_follow(world: &World, plan: &mut Value, changed: &[&str]) {
    let root = world.root();
    if changed.contains(&"/from") {
        if let Some(from) = plan["from"].as_str().map(str::to_owned) {
            let at = root.join(&from);
            if fs::symlink_metadata(&at).is_ok() {
                let (device, inode) = identity(&at);
                plan["device"] = json!(device);
                plan["inode"] = json!(inode);
            }
        }
    }
    if changed.contains(&"/links/0/path") {
        if let Some(path) = plan["links"][0]["path"].as_str().map(str::to_owned) {
            let at = root.join(&path);
            if fs::symlink_metadata(&at).is_ok_and(|entry| entry.file_type().is_symlink()) {
                let (device, inode) = identity(&at);
                plan["links"][0]["before"]["device"] = json!(device);
                plan["links"][0]["before"]["inode"] = json!(inode);
            }
        }
    }
}

/// Apply and rollback of the journal in hand, in `order`, on the root's grant alone and then (`every`) on the root's, alpha's and
/// beta's. After each press nothing under `Work` is new, changed or gone. Answers how many presses were carried out.
fn pressed_and_nothing_under_work_changes(
    world: &World,
    carried: &Value,
    label: &str,
    order: [usize; 2],
    every: bool,
) -> usize {
    let before = works_tree(world);
    let mut sets: Vec<Vec<&str>> = vec![vec!["control:root"]];
    if every {
        sets.push(vec!["control:root", "project:alpha", "project:beta"]);
    }
    let mut carried_out = 0;
    for grants in sets {
        granted_at(world, &grants);
        for press in order {
            let action = CARRIED[press].0;
            let answer = ctrl(world, action, carried, Some(PERSON));
            if answer["ok"] == true {
                carried_out += 1;
            }
            assert_eq!(
                works_changes(world, &before),
                Vec::<String>::new(),
                "{label}, {action} on {grants:?}: an entry under Work is new, changed or gone; the press answered {answer}"
            );
        }
    }
    carried_out
}

/// One family's count, printed for the receipt: `cargo test -- --nocapture` shows it.
fn family(name: &str, journals: usize, carried_out: usize) {
    println!("{name}: {journals} journals, {carried_out} presses carried out, no entry under Work new, changed or gone");
}

/// The acceptance test's second family (DLW build ruling 2026-11-12; the review of #349's generator): a journal no project holds
/// a seal of, the root's own move, so no change is refused merely because a seal no longer answers. Each field is changed to the
/// first two of its steering values, one journal each; the move is `interrupted` after its rename first, or not.
fn the_roots_own_journal_one_field_changed(interrupted: bool, shard: usize, shards: usize) {
    let cases: Vec<(String, usize)> = {
        let world = a_world_of_two_projects();
        let (_, journal) = a_move_of_the_roots_own(&world);
        let plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
        let mut fields = Vec::new();
        journal_fields(&plan, "", &mut fields);
        fields
            .into_iter()
            .flat_map(|field| {
                let most = if interrupted { 1 } else { 2 };
                (0..steering_values(&world, &plan, &field, most).len())
                    .map(|index| (field.clone(), index))
                    .collect::<Vec<_>>()
            })
            .enumerate()
            // Of an interrupted move, every second field: the count is kept to what a CI run carries.
            .filter(|(number, _)| !interrupted || number % 2 == 0)
            .map(|(_, case)| case)
            .collect()
    };
    let (mut journals, mut carried_out) = (0, 0);
    for (number, (field, index)) in cases.iter().enumerate() {
        if number % shards != shard {
            continue;
        }
        let world = a_world_of_two_projects();
        let (carried, journal) = a_move_of_the_roots_own(&world);
        if interrupted {
            interrupt_after_the_rename(&world, &carried);
        }
        let mut label = String::new();
        edit_journal(&journal, |plan| {
            let value = steering_values(&world, plan, field, 2)[*index].clone();
            label = format!("interrupted {interrupted}, {field}={value}");
            *plan.pointer_mut(field).unwrap() = value;
        });
        journals += 1;
        let order = if number % 2 == 0 { [0, 1] } else { [1, 0] };
        carried_out += pressed_and_nothing_under_work_changes(
            &world,
            &carried,
            &label,
            order,
            field != "/state",
        );
    }
    family(
        if interrupted {
            "the root's own journal, interrupted, one field changed"
        } else {
            "the root's own journal, one field changed"
        },
        journals,
        carried_out,
    );
}

#[test]
fn no_change_of_one_field_of_the_roots_own_journal_makes_an_entry_under_work_0() {
    the_roots_own_journal_one_field_changed(false, 0, 2);
}
#[test]
fn no_change_of_one_field_of_the_roots_own_journal_makes_an_entry_under_work_1() {
    the_roots_own_journal_one_field_changed(false, 1, 2);
}
#[test]
fn no_change_of_one_field_of_an_interrupted_journal_of_the_roots_makes_an_entry_under_work() {
    the_roots_own_journal_one_field_changed(true, 0, 1);
}

/// The acceptance test's third family: two of the fields that name places changed at once, each to its first steering value, on
/// the root's own journal, with identities made to follow. The count is kept to what a CI run carries.
#[test]
fn no_change_of_two_place_fields_of_the_roots_own_journal_makes_an_entry_under_work() {
    let fields: Vec<String> = {
        let world = a_world_of_two_projects();
        let (_, journal) = a_move_of_the_roots_own(&world);
        let plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
        let alpha = plan["worlds"]
            .as_array()
            .unwrap()
            .iter()
            .position(|listed| listed["world"] == "project:alpha")
            .expect("the journal lists alpha");
        assert!(
            plan["links"][0].is_object() && plan["documents"][0].is_object(),
            "{plan}"
        );
        vec![
            "/world".to_owned(),
            "/from".to_owned(),
            "/to".to_owned(),
            format!("/worlds/{alpha}/path"),
            format!("/worlds/{alpha}/external"),
            format!("/worlds/{alpha}/world"),
            "/links/0/world".to_owned(),
            "/links/0/path".to_owned(),
            "/links/0/after_path".to_owned(),
            "/documents/0/world".to_owned(),
            "/documents/0/path".to_owned(),
        ]
    };
    let (mut journals, mut carried_out) = (0, 0);
    for one in 0..fields.len() {
        // Every second pair: 28 of the 55, each field with five or six of the others.
        for two in (one + 1..fields.len()).filter(|two| (one + two) % 2 == 1) {
            let world = a_world_of_two_projects();
            let (carried, journal) = a_move_of_the_roots_own(&world);
            let chosen = [fields[one].as_str(), fields[two].as_str()];
            let mut label = String::new();
            edit_journal(&journal, |plan| {
                let original = plan.clone();
                for field in chosen {
                    let value = steering_values(&world, &original, field, 1)[0].clone();
                    label.push_str(&format!("{field}={value} "));
                    *plan.pointer_mut(field).unwrap() = value;
                }
                identities_follow(&world, plan, &chosen);
            });
            journals += 1;
            carried_out +=
                pressed_and_nothing_under_work_changes(&world, &carried, &label, [0, 1], true);
        }
    }
    family(
        "the root's own journal, two place fields changed",
        journals,
        carried_out,
    );
}

/// The journals the review of #349 built for the purpose, each the most a hand in the root's `.central` could write toward a
/// Work project, with the identities right and the staged links made in the root.
const BUILT: [&str; 20] = [
    "c01 a root link step at alpha's own link, staged in the root",
    "c02 a root link step that makes a new link in alpha (kept link made by hand)",
    "c03 the root's own link step, to stand in alpha afterwards",
    "c04 a made-up folder scope at alpha/docs",
    "c05 a made-up folder scope at alpha/ProjectCentral/user",
    "c07 alpha made the owner: a file of alpha's moved inside alpha, alpha's relations rewritten",
    "c08 the same, alpha spelled as a project outside the ground",
    "c09 the page's destination in alpha",
    "c10 a folder of alpha's moved out to the root",
    "c11 Work itself moved",
    "c12 alpha's whole folder moved out",
    "c13 rollback alone of a renamed journal whose Source is a folder of alpha's",
    "c14 a journal that refreshes alpha and beta",
    "c15 before seals: c01",
    "c16 before seals: c07",
    "c17 before seals: c10",
    "c18 alpha the owner, its retired flows registry written",
    "c19 rollback writes alpha's relations (before bytes) with alpha the owner",
    "c20 rollback moves a folder of alpha's out (the Source stands at its to, in alpha)",
    "c21 alpha's relations written by way of a link step of alpha's with its staged link missing",
];

/// Alpha made a journal's owner: a file of alpha's moved inside alpha, and one document of alpha's written (its relations, or
/// `path`).
fn alpha_the_owner(world: &World, plan: &mut Value, path: Option<&str>) {
    let root = world.root();
    let relations =
        fs::read_to_string(root.join("Work/alpha/ProjectCentral/relations/source-relations.json"))
            .ok();
    let (device, inode) = identity(&root.join("Work/alpha/docs/in.md"));
    plan["world"] = json!("project:alpha");
    plan["from"] = json!("docs/in.md");
    plan["to"] = json!("docs/out.md");
    plan["device"] = json!(device);
    plan["inode"] = json!(inode);
    plan["revision"] = json!(content_revision(b"in\n"));
    plan["links"] = json!([]);
    plan["refresh_worlds"] = json!([]);
    plan["documents"] = match path {
        Some(path) => {
            json!([{"world":"project:alpha","path":path,"before":null,"after":"{\"evil\":true}"}])
        }
        None => {
            json!([{"world":"project:alpha","path":null,"before":relations,"after":"{\"evil\":true}"}])
        }
    };
}

/// One of the [`BUILT`] journals, written over the root's own journal `plan`.
fn built(name: &str, world: &World, plan: &mut Value) {
    let root = world.root();
    let id = plan["id"].as_str().unwrap().to_owned();
    let number = &name[..3];
    let push_world =
        |plan: &mut Value, listed: Value| plan["worlds"].as_array_mut().unwrap().push(listed);
    match number {
        "c01" => {
            let staged = staged_by_hand(world, &id, 1, "../../Control/user/note.md");
            let step = link_step(
                "control:root",
                &id,
                1,
                "Work/alpha/sl",
                identity(&root.join("Work/alpha/sl")),
                staged,
            );
            plan["links"].as_array_mut().unwrap().push(step);
        }
        "c02" => {
            let staged = staged_by_hand(world, &id, 1, "evil");
            let kept = root.join(format!(".central/bkmr/moves/{id}-1-old"));
            std::os::unix::fs::symlink("was", &kept).unwrap();
            let step = link_step(
                "control:root",
                &id,
                1,
                "Work/alpha/newlink",
                identity(&kept),
                staged,
            );
            plan["links"].as_array_mut().unwrap().push(step);
        }
        "c03" => plan["links"][0]["after_path"] = json!("Work/alpha/newlink"),
        "c04" => push_world(
            plan,
            json!({"world":"folder:zz","path":"Work/alpha/docs","external":false,"project":null}),
        ),
        "c05" => push_world(
            plan,
            json!({"world":"folder:zz","path":"Work/alpha/ProjectCentral/user","external":false,"project":null}),
        ),
        "c07" => alpha_the_owner(world, plan, None),
        "c08" => {
            alpha_the_owner(world, plan, None);
            let alpha = fs::canonicalize(root.join("Work/alpha")).unwrap();
            for listed in plan["worlds"].as_array_mut().unwrap() {
                if listed["world"] == "project:alpha" {
                    listed["external"] = json!(true);
                    listed["path"] = json!(alpha);
                }
            }
        }
        "c09" => plan["to"] = json!("Work/alpha/note2.md"),
        "c10" | "c11" | "c12" => {
            let (from, to) = match number {
                "c10" => ("Work/alpha/docs", "Control/user/docs-out"),
                "c11" => ("Work", "Work2"),
                _ => ("Work/alpha", "Control/user/alpha-out"),
            };
            let (device, inode) = identity(&root.join(from));
            plan["from"] = json!(from);
            plan["to"] = json!(to);
            plan["device"] = json!(device);
            plan["inode"] = json!(inode);
            plan["links"] = json!([]);
            plan["documents"] = json!([]);
            plan["refresh_worlds"] = json!([]);
        }
        "c13" | "c20" => {
            let (device, inode) = identity(&root.join("Work/alpha/docs"));
            plan["from"] = json!("Control/user/docs-absent");
            plan["to"] = json!("Work/alpha/docs");
            plan["device"] = json!(device);
            plan["inode"] = json!(inode);
            plan["links"] = json!([]);
            plan["documents"] = json!([]);
            plan["refresh_worlds"] = json!([]);
            if number == "c13" {
                plan["state"] = json!("renamed");
                push_world(
                    plan,
                    json!({"world":"project:ghost","path":"Control/user/ghost","external":false,"project":"ghost"}),
                );
            } else {
                plan["state"] = json!("applied");
            }
        }
        "c14" => plan["refresh_worlds"] = json!(["project:alpha", "project:beta"]),
        "c15" => {
            built("c01", world, plan);
            plan["schema"] = json!("central.file-map-move/v1");
        }
        "c16" => {
            built("c07", world, plan);
            plan["schema"] = json!("central.file-map-move/v1");
        }
        "c17" => {
            built("c10", world, plan);
            plan["schema"] = json!("central.file-map-move/v1");
        }
        "c18" => alpha_the_owner(world, plan, Some(".central/flows.json")),
        "c19" => {
            // The Source stands at its `to`, and alpha's relations read as the journal's `after`: a rollback writes `before`.
            let relations = fs::read_to_string(
                root.join("Work/alpha/ProjectCentral/relations/source-relations.json"),
            )
            .unwrap();
            alpha_the_owner(world, plan, None);
            plan["from"] = json!("docs/absent.md");
            plan["to"] = json!("docs/in.md");
            plan["state"] = json!("applied");
            plan["documents"] = json!([{"world":"project:alpha","path":null,"before":"{\"evil\":true}","after":relations}]);
        }
        "c21" => {
            let relations = fs::read_to_string(
                root.join("Work/alpha/ProjectCentral/relations/source-relations.json"),
            )
            .ok();
            let step = link_step(
                "project:alpha",
                &id,
                1,
                "sl",
                identity(&root.join("Work/alpha/sl")),
                (0, 0),
            );
            plan["links"].as_array_mut().unwrap().push(step);
            plan["documents"].as_array_mut().unwrap().push(
                json!({"world":"project:alpha","path":null,"before":relations,"after":"{\"evil\":true}"}),
            );
        }
        _ => panic!("no such built journal: {name}"),
    }
}

/// The acceptance test's fourth family: the built journals, each apply first and rollback first; and each again for a move
/// interrupted after its rename, the two orders in turn.
fn the_built_journals(interrupted: bool) {
    let (mut journals, mut carried_out) = (0, 0);
    for (number, name) in BUILT.iter().enumerate() {
        let orders: Vec<[usize; 2]> = if interrupted {
            vec![if number % 2 == 0 { [0, 1] } else { [1, 0] }]
        } else {
            vec![[0, 1], [1, 0]]
        };
        for order in orders {
            let world = a_world_of_two_projects();
            let (carried, journal) = a_move_of_the_roots_own(&world);
            if interrupted {
                interrupt_after_the_rename(&world, &carried);
            }
            edit_journal(&journal, |plan| {
                let state = plan["state"].clone();
                built(name, &world, plan);
                // A built journal that sets no state of its own keeps the one the move was left in.
                if plan["state"] == "prepared" {
                    plan["state"] = state;
                }
            });
            journals += 1;
            carried_out += pressed_and_nothing_under_work_changes(
                &world,
                &carried,
                &format!("{name}, interrupted {interrupted}, order {order:?}"),
                order,
                true,
            );
        }
    }
    family(
        if interrupted {
            "the built journals, interrupted"
        } else {
            "the built journals"
        },
        journals,
        carried_out,
    );
}

#[test]
fn no_journal_built_toward_a_work_project_makes_an_entry_under_work() {
    the_built_journals(false);
}
#[test]
fn no_journal_built_toward_a_work_project_makes_an_entry_under_work_once_interrupted() {
    the_built_journals(true);
}

/// The acceptance test's fifth family: the seal itself, and journals presented with a seal that is not theirs. The journal is one
/// alpha holds a seal of; the orders go in turn.
#[test]
fn no_seal_that_is_not_the_journals_own_lets_it_make_an_entry_under_work() {
    const SHAPES: [&str; 10] = [
        "s1 the seal's digest edited",
        "s2 the seal a link to a true copy",
        "s3 the seal removed",
        "s4 the seal's folder a link",
        "s5 the seal of another move, copied over",
        "s6 the seal of another move, its id rewritten",
        "s7 the journal of another move in this id's file",
        "s8 the journal of another move, its id rewritten",
        "s9 alpha's .central a link to a copy",
        "s10 alpha's seal copied into beta, and the journal aimed at beta",
    ];
    let read = |path: &Path| -> Value { serde_json::from_slice(&fs::read(path).unwrap()).unwrap() };
    let (mut journals, mut carried_out) = (0, 0);
    for (number, shape) in SHAPES.iter().enumerate() {
        let world = a_world_of_two_projects();
        let root = world.root();
        let (carried, journal, seal) = a_move_that_writes_in_alpha(&world);
        let id = carried["plan_id"].as_str().unwrap().to_owned();
        let moves = seal.parent().unwrap().to_path_buf();
        // A second move of the same page, planned on the same grants: another journal, and another seal in alpha.
        let second = |world: &World| -> (PathBuf, PathBuf) {
            let planned = ctrl(
                world,
                "central.file-map.move-plan",
                &json!({"source_ref":page_source(world),"destination":"Control/user/note3.md"}),
                Some(PERSON),
            );
            assert_eq!(planned["ok"], true, "{planned}");
            let other = planned["data"]["result"]["plan_id"].as_str().unwrap();
            (
                world
                    .root()
                    .join(format!(".central/bkmr/moves/{other}.json")),
                world
                    .root()
                    .join(format!("Work/alpha/.central/bkmr/moves/{other}.seal.json")),
            )
        };
        match shape.split(' ').next().unwrap() {
            "s1" => {
                let mut held = read(&seal);
                held["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
                fs::write(&seal, serde_json::to_vec(&held).unwrap()).unwrap();
            }
            "s2" => {
                let copy = root.join("Control/user/copied.seal.json");
                fs::rename(&seal, &copy).unwrap();
                std::os::unix::fs::symlink(&copy, &seal).unwrap();
            }
            "s3" => fs::remove_file(&seal).unwrap(),
            "s4" => {
                let kept = root.join("Control/user/moves-kept-elsewhere");
                fs::rename(&moves, &kept).unwrap();
                std::os::unix::fs::symlink(&kept, &moves).unwrap();
            }
            "s5" => {
                let (_, other_seal) = second(&world);
                fs::copy(&other_seal, &seal).unwrap();
            }
            "s6" => {
                let (_, other_seal) = second(&world);
                let mut held = read(&other_seal);
                held["plan_id"] = json!(id);
                fs::write(&seal, serde_json::to_vec(&held).unwrap()).unwrap();
            }
            "s7" => {
                let (other_journal, _) = second(&world);
                fs::copy(&other_journal, &journal).unwrap();
            }
            "s8" => {
                let (other_journal, _) = second(&world);
                let mut plan = read(&other_journal);
                plan["id"] = json!(id);
                fs::write(&journal, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
            }
            "s9" => {
                let kept = root.join("Control/user/alpha-central");
                fs::rename(root.join("Work/alpha/.central"), &kept).unwrap();
                std::os::unix::fs::symlink(&kept, root.join("Work/alpha/.central")).unwrap();
            }
            _ => {
                let betas = root.join("Work/beta/.central/bkmr/moves");
                fs::create_dir_all(&betas).unwrap();
                fs::copy(&seal, betas.join(seal.file_name().unwrap())).unwrap();
                edit_journal(&journal, |plan| {
                    let staged = staged_by_hand(&world, &id, 1, "../../Control/user/note.md");
                    let step = link_step(
                        "control:root",
                        &id,
                        1,
                        "Work/beta/sl2",
                        identity(&root.join("Work/beta/sl2")),
                        staged,
                    );
                    plan["links"].as_array_mut().unwrap().push(step);
                });
            }
        }
        journals += 1;
        let order = if number % 2 == 0 { [0, 1] } else { [1, 0] };
        carried_out += pressed_and_nothing_under_work_changes(&world, &carried, shape, order, true);
    }
    family(
        "a seal that is not the journal's own",
        journals,
        carried_out,
    );
}

/// A journal's first relations document of `world`, as text: what the move found there (`before`) and what it writes (`after`).
fn journal_document(journal: &Path, world: &str) -> (Option<String>, String) {
    let plan: Value = serde_json::from_slice(&fs::read(journal).unwrap()).unwrap();
    let document = plan["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["world"] == world)
        .unwrap_or_else(|| panic!("the journal writes the relations of {world}"));
    (
        document["before"].as_str().map(str::to_owned),
        document["after"].as_str().unwrap().to_owned(),
    )
}

/// The review of #349, F2 (DLW build ruling 2026-11-12): a journal from before seals never leaves a person without a press. The
/// move writes in alpha (alpha links the page) and in the root (the root links it too). It is left in each of its five states,
/// then made a journal from before seals (schema v1, no seal).
///
/// Apply is refused, since it would write in alpha, and nothing changes; its words say where the Source stands and what goes on
/// from there: "plan the move again" where nothing of the move stands, "roll the move back" where something does. Rollback is
/// carried out from every state: the page is back, the root's link leads to it and the root's relations are as the plan found
/// them. Where a step in alpha stood carried ("applied", "rolling-back"), nothing of alpha's is written, the journal stays
/// "rolling-back", and the answer names each step for a hand, in order. This test then makes those steps as the words give
/// them, and a second rollback closes the journal. From there the map reads, alpha's link leads to the page, and the move can
/// be planned again.
#[test]
fn a_journal_from_before_seals_is_rolled_back_from_every_state_and_says_what_a_hand_must_do() {
    for state in [
        "prepared",
        "renamed",
        "applied",
        "rolling-back",
        "rolled-back",
    ] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        root_link(&world, "Control/user/note-link");
        let (carried, journal, seal) = a_move_that_writes_in_alpha(&world);
        let id = carried["plan_id"].as_str().unwrap().to_owned();
        let interrupt = [("CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME", "1")];
        let press = |index: usize, variables: &[(&str, &str)]| {
            ctrl_with(&world, CARRIED[index].0, &carried, Some(PERSON), variables)
        };
        match state {
            "prepared" => {}
            "renamed" => {
                press(0, &interrupt);
            }
            "applied" => {
                press(0, &[]);
            }
            "rolling-back" => {
                press(0, &[]);
                press(1, &interrupt);
            }
            _ => {
                press(0, &[]);
                press(1, &[]);
            }
        }
        assert_eq!(journal_state(&world, &carried), state);
        edit_journal(&journal, |plan| {
            plan["schema"] = json!("central.file-map-move/v1")
        });
        fs::remove_file(&seal).unwrap();
        let page = root.join("Control/user/note.md");
        let moved = root.join("Control/user/note2.md");
        let at_from = page.is_file();
        let canonical = |path: &Path| fs::canonicalize(path).unwrap();
        let (roots_before, _) = journal_document(&journal, "control:root");
        let (alphas_before, _) = journal_document(&journal, "project:alpha");
        let alphas_relations =
            root.join("Work/alpha/ProjectCentral/relations/source-relations.json");

        // Apply: refused, nothing changes, and the words lead to a press that is not refused.
        let before = ground(&world);
        let refused = press(0, &[]);
        assert_eq!(refused["ok"], false, "{state}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains(
                "This move was planned before a plan was kept in each Work project it writes in"
            ) && words.contains("`ctrl` writes nothing of this move there"),
            "{state}: {words}"
        );
        let stands = if at_from { &page } else { &moved };
        assert!(
            words.contains(&format!(
                "Its Source stands at {}",
                canonical(stands).display()
            )),
            "{state}: {words}"
        );
        if matches!(state, "prepared" | "rolled-back") {
            assert!(
                words.contains("nothing of the move stands: plan the move again")
                    && !words.contains("roll"),
                "{state}: {words}"
            );
        } else {
            assert!(words.contains("roll the move back"), "{state}: {words}");
        }
        assert_eq!(
            ground(&world),
            before,
            "{state}: a refused apply changes nothing"
        );

        // Rollback: carried out so far as it writes in no Work project.
        let alpha_before = alphas_tree(&world);
        let back = press(1, &[]);
        assert_eq!(back["ok"], true, "{state}: {back}");
        assert_eq!(
            alphas_tree(&world),
            alpha_before,
            "{state}: a rollback of a journal from before seals writes nothing in alpha"
        );
        assert!(
            page.is_file() && !moved.exists(),
            "{state}: the page is back"
        );
        assert_eq!(
            canonical(&root.join("Control/user/note-link")),
            canonical(&page),
            "{state}: the root's link leads to the page"
        );
        assert_eq!(
            fs::read_to_string(root.join("Control/relations/source-relations.json")).ok(),
            roots_before,
            "{state}: the root's relations are as the plan found them"
        );
        let left: Vec<String> = back["data"]["result"]["left_for_a_hand"]
            .as_array()
            .map(|left| {
                left.iter()
                    .map(|step| step.as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        if matches!(state, "applied" | "rolling-back") {
            let kept = format!("Work/alpha/.central/bkmr/moves/{id}-");
            assert_eq!(left.len(), 2, "{state}: {back}");
            assert!(
                left[0].starts_with("in alpha, remove the link Work/alpha/links/note, then move ")
                    && left[0].contains(&kept)
                    && left[0].ends_with("-old to Work/alpha/links/note"),
                "{state}: {left:?}"
            );
            assert_eq!(
                left[1],
                format!(
                    "in alpha, write Work/alpha/ProjectCentral/relations/source-relations.json back as this move's journal \
                     keeps it (the text at `documents[1].before` in .central/bkmr/moves/{id}.json)"
                ),
                "{state}"
            );
            assert_eq!(back["data"]["result"]["state"], "rolling-back", "{state}");
            let note = back["data"]["result"]["note"].as_str().unwrap_or_default();
            assert!(
                note.contains("`ctrl` writes nothing of it there")
                    && note.contains(&left[0])
                    && note.contains(&left[1]),
                "{state}: {note}"
            );
            // A hand makes the steps as the words give them.
            let link = root.join("Work/alpha/links/note");
            let kept_link = fs::read_dir(root.join("Work/alpha/.central/bkmr/moves"))
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .find(|path| {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    name.starts_with(&format!("{id}-")) && name.ends_with("-old")
                })
                .expect("the kept link stands in alpha's moves folder");
            assert!(left[0].contains(
                kept_link
                    .strip_prefix(canonical(&root))
                    .or_else(|_| kept_link.strip_prefix(&root))
                    .unwrap()
                    .to_str()
                    .unwrap()
            ));
            fs::remove_file(&link).unwrap();
            fs::rename(&kept_link, &link).unwrap();
            fs::write(&alphas_relations, alphas_before.as_deref().unwrap()).unwrap();
            let closed = press(1, &[]);
            assert_eq!(closed["ok"], true, "{state}: {closed}");
            assert!(
                closed["data"]["result"]["left_for_a_hand"].is_null(),
                "{state}: {closed}"
            );
        } else {
            assert!(left.is_empty(), "{state}: {back}");
        }
        assert_eq!(journal_state(&world, &carried), "rolled-back", "{state}");
        assert_eq!(
            canonical(&root.join("Work/alpha/links/note")),
            canonical(&page),
            "{state}: alpha's link leads to the page"
        );
        assert_eq!(
            fs::read_to_string(&alphas_relations).ok(),
            alphas_before,
            "{state}: alpha's relations are as the plan found them"
        );
        for scope in [None, Some("alpha")] {
            let inspected = ctrl(
                &world,
                "central.file-map.inspect",
                &json!({"project":scope,"resources":false}),
                None,
            );
            assert_eq!(inspected["ok"], true, "{state}: {inspected}");
        }
        let again = ctrl(
            &world,
            "central.file-map.move-plan",
            &json!({"source_ref":page_source(&world),"destination":"Control/user/note2.md"}),
            Some(PERSON),
        );
        assert_eq!(
            again["ok"], true,
            "{state}: the move is planned again: {again}"
        );
    }
}

/// The review of #349, F2, where the rollback's own rename would write in a Work project (DLW build ruling 2026-11-12): a
/// journal from before seals of a move inside alpha, interrupted after its rename. Its rollback cannot move the folder back,
/// since that writes in alpha. The refusal says where the Source stands and the one step a hand makes; nothing changes. Once the
/// hand has made it, the rollback is carried out and the journal closes.
#[test]
fn a_journal_from_before_seals_whose_source_is_in_a_work_project_says_how_a_hand_moves_it_back() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/alpha/docs/sub")).unwrap();
    fs::write(root.join("Work/alpha/docs/sub/in.md"), "in\n").unwrap();
    granted_at(&world, &["control:root", "project:alpha"]);
    let registered = ctrl(
        &world,
        "central.file-map.register",
        &json!({"project":"alpha","path":"docs/sub","expected_revision":revision(&world, Some("alpha"))}),
        Some(PERSON),
    );
    assert_eq!(registered["ok"], true, "{registered}");
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"project":"alpha","source_ref":registered["data"]["result"]["source_ref"],"destination":"docs/sub2"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    let carried = json!({"plan_id":id,"quiesced":true});
    interrupt_after_the_rename(&world, &carried);
    edit_journal(
        &root.join(format!(".central/bkmr/moves/{id}.json")),
        |plan| plan["schema"] = json!("central.file-map-move/v1"),
    );
    fs::remove_file(root.join(format!("Work/alpha/.central/bkmr/moves/{id}.seal.json"))).unwrap();
    let alpha = fs::canonicalize(root.join("Work/alpha")).unwrap();
    let (from, to) = (alpha.join("docs/sub"), alpha.join("docs/sub2"));
    let before = ground(&world);
    let refused = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains(&format!("Its Source stands at {}", to.display()))
            && words.contains(&format!(
                "a hand does it: move {} to {}",
                to.display(),
                from.display()
            ))
            && words.contains("A rollback then puts back the steps outside Work's projects"),
        "{words}"
    );
    assert_eq!(ground(&world), before, "a refused rollback changes nothing");
    fs::rename(&to, &from).unwrap();
    let back = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert_eq!(journal_state(&world, &carried), "rolled-back");
    assert!(root.join("Work/alpha/docs/sub/in.md").is_file());
}

/// The review of #349, F3 (DLW build ruling 2026-11-12): the rollback that makes the rename back alone asks the seal itself. It
/// is the one write the asking before the rename does not stand before. A journal no project holds says its rename was carried
/// and that its Source is a folder of alpha's, standing in alpha; a project it runs from does not read, so its rollback is the
/// rename back alone. On the root's grant and alpha's it is refused in the seal's words, and alpha's folder is not moved out.
#[test]
fn a_rollback_that_makes_the_rename_back_alone_asks_the_projects_seal() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/alpha/docs")).unwrap();
    fs::write(root.join("Work/alpha/docs/in.md"), "in\n").unwrap();
    project(&root.join("Control/user/standin"), "standin");
    let declared = ctrl(
        &world,
        "central.file-map.scope-register",
        &json!({"name":"standin","path":"Control/user/standin","kind":"project","expected_revision":revision(&world, None)}),
        Some(OWNER),
    );
    assert_eq!(declared["ok"], true, "{declared}");
    let docs = identity(&root.join("Work/alpha/docs"));
    let carried = edited_journal(&world, |plan| {
        plan["from"] = json!("Control/user/docs-absent");
        plan["to"] = json!("Work/alpha/docs");
        plan["device"] = json!(docs.0);
        plan["inode"] = json!(docs.1);
        plan["state"] = json!("renamed");
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
    });
    // The project the journal runs from no longer reads: the rollback is the rename back alone.
    fs::remove_file(root.join("Control/user/standin/ProjectCentral/project.json")).unwrap();
    granted_at(&world, &["control:root", "project:alpha"]);
    let before = works_tree(&world);
    let refused = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains("This move's journal is not the plan the Work project alpha holds"),
        "{words}"
    );
    assert_eq!(
        works_changes(&world, &before),
        Vec::<String>::new(),
        "alpha's folder is not moved out"
    );
    assert!(!root.join("Control/user/docs-absent").exists());
}

/// The review of #349, F3: where the Source stands, where it goes and whose it is are under the seal. A journal alpha holds the
/// plan of is changed in one field alone: its `to`, to a new name in alpha; its `from`, to a second name of the same page (a
/// hard link) that stands in alpha; its owner (`world`), to alpha, where a second name of the page stands at the same path
/// below alpha as the page's below the root. In each the Source's identity still answers, and no other rule refuses the
/// journal, so each would move an entry of alpha's. On the root's grant and alpha's each is refused in the seal's words, and
/// nothing under `Work` changes.
#[test]
fn where_a_source_stands_where_it_goes_and_whose_it_is_are_under_the_seal() {
    for field in ["to", "from", "world"] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        fs::create_dir_all(root.join("Work/alpha/Control/user")).unwrap();
        for second_name in [
            "Work/alpha/second-name.md",
            "Work/alpha/Control/user/note.md",
        ] {
            fs::hard_link(root.join("Control/user/note.md"), root.join(second_name)).unwrap();
        }
        // The root links the page too, so the journal keeps a link step of the root's and may write the root's relations
        // whoever its owner is said to be.
        root_link(&world, "Control/user/note-link");
        let (carried, journal, _) = a_move_that_writes_in_alpha(&world);
        edit_journal(&journal, |plan| {
            plan[field] = json!(match field {
                "to" => "Work/alpha/new.md",
                "from" => "Work/alpha/second-name.md",
                _ => "project:alpha",
            })
        });
        let before = works_tree(&world);
        for (action, _) in CARRIED {
            let refused = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(refused["ok"], false, "{field}, {action}: {refused}");
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(
                words.contains("This move's journal is not the plan the Work project alpha holds"),
                "{field}, {action}: {words}"
            );
            assert_eq!(
                works_changes(&world, &before),
                Vec::<String>::new(),
                "{field}, {action}"
            );
            assert!(
                root.join("Control/user/note.md").is_file()
                    && !root.join("Control/user/note2.md").exists(),
                "{field}, {action}: the page stands"
            );
        }
    }
}

/// The review of #349, F3: a seal is read with no link in the folders above it, and that line is reached by a move that
/// reaches a project by its destination alone. Such a move stages no link in the project's moves folder, so the rule that a
/// staged link stands behind no link does not speak first. The page is to go into alpha, on a plan alpha holds; alpha's moves
/// folder is then kept elsewhere, with the true seal in it, and a link put in its place. Apply is refused in the seal's words
/// on both grants, the page does not move, and nothing of alpha's changes.
#[test]
fn a_seal_read_through_a_moves_folder_that_is_a_link_is_no_seal_for_a_move_into_the_project() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    granted_at(&world, &["control:root", "project:alpha"]);
    let planned = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":page_source(&world),"destination":"Work/alpha/note2.md"}),
        Some(PERSON),
    );
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["data"]["result"]["plan_id"].as_str().unwrap();
    let carried = json!({"plan_id":id,"quiesced":true});
    let moves = root.join("Work/alpha/.central/bkmr/moves");
    assert!(
        moves.join(format!("{id}.seal.json")).is_file(),
        "alpha holds the plan"
    );
    assert_eq!(
        fs::read_dir(&moves).unwrap().count(),
        1,
        "and the move stages no link in alpha"
    );
    let kept = root.join("Control/user/moves-kept-elsewhere");
    fs::rename(&moves, &kept).unwrap();
    std::os::unix::fs::symlink(&kept, &moves).unwrap();
    let before = works_tree(&world);
    let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains("This move's journal is not the plan the Work project alpha holds"),
        "{words}"
    );
    assert_eq!(works_changes(&world, &before), Vec::<String>::new());
    assert!(
        root.join("Control/user/note.md").is_file(),
        "the page stands"
    );
}

/// The review of #349, F4 (DLW build ruling 2026-11-12): an index kept in a Work project is refreshed by a move only where the
/// project holds the move's plan. A move of the root's own is edited to list alpha among the scopes it refreshes, and alpha
/// keeps an index (here a file standing where one is kept; no bkmr runs in this world). The move is carried out, since it is the
/// root's own; alpha's index is left alone, and the answer names the scope. Before, apply went on to refresh alpha's index.
#[test]
fn a_move_refreshes_an_index_in_a_work_project_only_where_the_project_holds_its_plan() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    fs::create_dir_all(root.join("Work/alpha/.central/bkmr")).unwrap();
    fs::write(
        root.join("Work/alpha/.central/bkmr/index.db"),
        "an index stands here",
    )
    .unwrap();
    let (carried, journal) = a_move_of_the_roots_own(&world);
    edit_journal(&journal, |plan| {
        plan["refresh_worlds"] = json!(["project:alpha"])
    });
    let before = works_tree(&world);
    for grants in [vec!["control:root"], vec!["control:root", "project:alpha"]] {
        granted_at(&world, &grants);
        for (action, _) in CARRIED {
            let answer = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(answer["ok"], true, "{action} on {grants:?}: {answer}");
            assert_eq!(
                answer["data"]["result"]["indexes_not_refreshed"],
                json!(["project:alpha"]),
                "{action} on {grants:?}: {answer}"
            );
            assert_eq!(
                works_changes(&world, &before),
                Vec::<String>::new(),
                "{action} on {grants:?}"
            );
        }
    }
}

/// The review of #349, F5 (DLW build ruling 2026-11-12): `Work` itself is not moved. A registration of `Work` at the root, as one
/// made before the gate refused it, is planned to `Work2` on every grant: refused in words, and no journal is written.
#[test]
fn a_move_of_work_itself_is_refused_at_its_plan() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    granted_at(&world, &["control:root", "project:alpha"]);
    let work = legacy_root_source(&world, "Work");
    let refused = ctrl(
        &world,
        "central.file-map.move-plan",
        &json!({"source_ref":work,"destination":"Work2"}),
        Some(PERSON),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    let words = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        words.contains(
            "Work is Work itself, or a folder that holds it, and Central does not move it"
        ) && words.contains("Move a project of Work, or a folder inside one"),
        "{words}"
    );
    assert_eq!(journals(&root), 0, "no journal is written");
    assert!(root.join("Work/alpha/plan.md").is_file() && !root.join("Work2").exists());
}

/// What every act of the person's answers on a ground whose `Work` is away (file_map.rs `work_away`), after its opening.
fn work_is_away(root: &Path) -> String {
    format!(
        "Work does not stand at {}, where the ground keeps it, and no act reads a ground without it: a hand puts Work back there \
         before any press. If a move took it away, that move's journal says in its `to` where it stands, and the move is rolled \
         back once Work is back",
        root.join("Work").display()
    )
}

/// The review of #349, F5, and of #352, F1: a move of `Work` that an older `ctrl` planned and began. The journal is as such a
/// `ctrl` left it (from before seals), "renamed" or "applied", and `Work` stands at `Work2`. No act's grant can be asked of
/// that ground. The apply and the rollback of that journal are refused in words that say `Work` does not stand where the
/// ground keeps it and that a hand puts it back, and nothing moves. The words come from the ground alone: they do not name
/// where the journal says `Work` went. A caller with no credential is told nothing of it. Once a hand has moved `Work` back,
/// the rollback is an ordinary one: it asks alpha's grant, since the journal's Source holds alpha, and closes the journal.
/// Then the map reads.
#[test]
fn a_move_of_work_left_half_done_by_an_older_ctrl_says_how_a_hand_mends_it() {
    for state in ["renamed", "applied"] {
        let world = granting(&every_act(), "control:root");
        let root = world.root();
        let work = identity(&root.join("Work"));
        let carried = edited_journal(&world, |plan| {
            plan["schema"] = json!("central.file-map-move/v1");
            plan["from"] = json!("Work");
            plan["to"] = json!("Work2");
            plan["device"] = json!(work.0);
            plan["inode"] = json!(work.1);
            plan["state"] = json!(state);
            plan["refresh_worlds"] = json!([]);
            plan["documents"] = json!([]);
            plan["links"] = json!([]);
        });
        fs::rename(root.join("Work"), root.join("Work2")).unwrap();
        let before = ground(&world);
        for (action, opening) in CARRIED {
            let unknown = ctrl(&world, action, &carried, None);
            let words = unknown["error"]["message"].as_str().unwrap_or_default();
            assert!(
                words.contains("missing bounded CENTRAL_NATIVE_TOKEN") && !words.contains("Work"),
                "{state} {action}: {words}"
            );
            let refused = ctrl(&world, action, &carried, Some(PERSON));
            assert_eq!(refused["ok"], false, "{state} {action}: {refused}");
            assert_eq!(
                refused["error"]["code"], "central.file_map_not_found",
                "{state} {action}: {refused}"
            );
            assert_eq!(
                refused["error"]["message"].as_str().unwrap_or_default(),
                format!("{opening}{}; nothing was moved", work_is_away(&root)),
                "{state} {action}"
            );
        }
        assert_eq!(
            ground(&world),
            before,
            "{state}: nothing moves on a refused press"
        );
        // A hand puts `Work` back.
        fs::rename(root.join("Work2"), root.join("Work")).unwrap();
        refused_in_words(
            &world,
            CARRIED[1].0,
            &carried,
            Some(PERSON),
            CARRIED[1].1,
            REACHES_ALPHA,
        );
        granted_at(&world, &["control:root", "project:alpha"]);
        let closed = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
        assert_eq!(closed["ok"], true, "{state}: {closed}");
        assert_eq!(journal_state(&world, &carried), "rolled-back", "{state}");
        for scope in [None, Some("alpha")] {
            let inspected = ctrl(
                &world,
                "central.file-map.inspect",
                &json!({"project":scope,"resources":false}),
                None,
            );
            assert_eq!(inspected["ok"], true, "{state}: {inspected}");
        }
    }
}

/// The review of #352, F1 (DLW build ruling 2026-11-12): on a ground whose `Work` is away nothing is read on the word of a
/// caller no grant names. The authority reads the ground before it matches a credential, so there every caller with a
/// credential of valid length is answered alike, granted or not. Two journals stand: the one that took `Work` to
/// `Work-taken-there`, and another, of the root's own page, interrupted. For a credential no grant names, an agent's and the
/// person's, the apply and the rollback answer the same words whichever id they name (the journal that took `Work`, the
/// other, an id no journal has, none), and the words hold no place from any journal. Every other act of the person's answers
/// them too. A caller with no credential, or one too short, is told only that. Nothing is written. Once `Work` is back the
/// other journal's rollback is carried out; and a "not found" that is not about `Work` keeps its own words.
#[test]
fn a_ground_whose_work_is_away_reads_no_journal_for_any_caller() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    let work = identity(&root.join("Work"));
    let took_work = edited_journal(&world, |plan| {
        plan["schema"] = json!("central.file-map-move/v1");
        plan["from"] = json!("Work");
        plan["to"] = json!("Work-taken-there");
        plan["device"] = json!(work.0);
        plan["inode"] = json!(work.1);
        plan["state"] = json!("renamed");
        plan["refresh_worlds"] = json!([]);
        plan["documents"] = json!([]);
        plan["links"] = json!([]);
    });
    let (other, _) = a_move_of_the_roots_own(&world);
    interrupt_after_the_rename(&world, &other);
    let acts = every_act();
    authority(
        &world,
        vec![
            grant(PERSON, "human", &["control:root"], &acts, u64::MAX),
            grant(AGENT, "agent", &["control:root"], &acts, u64::MAX),
        ],
    );
    fs::rename(root.join("Work"), root.join("Work-taken-there")).unwrap();
    let before = ground(&world);
    let named = [
        took_work.clone(),
        other.clone(),
        json!({"plan_id":"abc123","quiesced":true}),
        json!({"quiesced":true}),
    ];
    for token in [UNKNOWN, AGENT, PERSON] {
        for (action, opening) in CARRIED {
            for input in &named {
                let refused = ctrl(&world, action, input, Some(token));
                assert_eq!(refused["ok"], false, "{action} {input}: {refused}");
                assert_eq!(
                    refused["error"]["code"], "central.file_map_not_found",
                    "{action} {input}: {refused}"
                );
                let words = refused["error"]["message"].as_str().unwrap_or_default();
                assert!(
                    !words.contains("Work-taken-there") && !words.contains("note2"),
                    "a place from a journal: {action} {input}: {words}"
                );
                assert_eq!(
                    words,
                    format!("{opening}{}; nothing was moved", work_is_away(&root)),
                    "{action} {input}"
                );
            }
        }
        for (action, input) in [
            (
                "central.file-map.move-plan",
                json!({"source_ref":"unread","destination":"Control/user/n3.md"}),
            ),
            (
                "central.file-map.register",
                json!({"path":"Control/user/note.md"}),
            ),
        ] {
            let refused = ctrl(&world, action, &input, Some(token));
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(
                refused["ok"] == false && words.contains(&work_is_away(&root)),
                "{action}: {refused}"
            );
        }
    }
    for token in [None, Some("too-short")] {
        for (action, _) in CARRIED {
            let refused = ctrl(&world, action, &took_work, token);
            let words = refused["error"]["message"].as_str().unwrap_or_default();
            assert!(
                words.contains("missing bounded CENTRAL_NATIVE_TOKEN") && !words.contains("Work"),
                "{action}: {words}"
            );
        }
    }
    assert_eq!(ground(&world), before, "nothing is written");
    fs::rename(root.join("Work-taken-there"), root.join("Work")).unwrap();
    let back = ctrl(&world, CARRIED[1].0, &other, Some(PERSON));
    assert_eq!(back["ok"], true, "{back}");
    assert!(root.join("Control/user/note.md").is_file());
    // `Work` stands, and the authority Source is gone: that "not found" is not about `Work`, and is passed on as it came.
    fs::remove_file(root.join("Control/user/file-map-moves-authority.json")).unwrap();
    let refused = ctrl(&world, CARRIED[1].0, &other, Some(PERSON));
    assert_eq!(
        refused["error"]["code"], "central.file_map_not_found",
        "{refused}"
    );
    assert!(
        !refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Work does not stand"),
        "{refused}"
    );
}

/// The digest `move-plan` seals a journal by (file_map_moves.rs `digest`), for a journal built by hand: SHA-256 over the
/// journal written compactly in the order `move-plan` keeps its fields, with `state` blank. For a journal with no link step.
fn seal_digest(plan: &Value) -> String {
    let fields = |value: &Value, names: &[&str]| {
        let members: Vec<String> = names
            .iter()
            .map(|name| format!("{}:{}", json!(name), value[name]))
            .collect();
        format!("{{{}}}", members.join(","))
    };
    let each = |list: &Value, names: &[&str]| {
        let members: Vec<String> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|member| fields(member, names))
            .collect();
        format!("[{}]", members.join(","))
    };
    assert_eq!(plan["links"], json!([]), "a journal with no link step");
    let written = format!(
        "{{\"worlds\":{},\"refresh_worlds\":{},{},\"documents\":{},\"links\":[],\"state\":\"\"}}",
        each(&plan["worlds"], &["world", "path", "external", "project"]),
        plan["refresh_worlds"],
        fields(
            plan,
            &[
                "schema",
                "id",
                "world",
                "source_ref",
                "from",
                "to",
                "device",
                "inode",
                "revision"
            ]
        )
        .trim_matches(|c| c == '{' || c == '}'),
        each(&plan["documents"], &["world", "path", "before", "after"]),
    );
    format!("sha256:{:x}", Sha256::digest(written.as_bytes()))
}

/// The review of #352, F2 (DLW build ruling 2026-11-12): a move of `Work` is carried out no more than it is planned. Between
/// the seals and the rule that refuses such a plan, `ctrl` planned and sealed one. The journal here is built as that `ctrl`
/// wrote it: `central.file-map-move/v2`, the root's Source `Work` to `Work2`, the root's relations document rewritten, alpha
/// to refresh, and alpha holding its seal. On every grant its apply was carried out and took `Work` away, leaving a ground no
/// act reads. Now the apply is refused in words, whatever seals it holds, and nothing moves; the rollback closes the journal,
/// which shows the seal is the journal's own; an apply after that is refused alike, and the map reads.
#[test]
fn a_sealed_journal_of_a_move_of_work_is_not_carried_out() {
    let world = granting(&every_act(), "control:root");
    let root = world.root();
    granted_at(&world, &["control:root", "project:alpha"]);
    let source = legacy_root_source(&world, "Work");
    let work = identity(&root.join("Work"));
    let relations =
        fs::read_to_string(root.join("Control/relations/source-relations.json")).unwrap();
    let mut after: Value = serde_json::from_str(&relations).unwrap();
    after["file_map"]["resources"][&source]["path"] = json!("Work2");
    let carried = edited_journal(&world, |plan| {
        assert_eq!(plan["schema"], "central.file-map-move/v2", "{plan}");
        plan["source_ref"] = json!(source);
        plan["from"] = json!("Work");
        plan["to"] = json!("Work2");
        plan["device"] = json!(work.0);
        plan["inode"] = json!(work.1);
        plan["refresh_worlds"] = json!(["project:alpha"]);
        plan["documents"] = json!([{"world":"control:root","path":null,"before":relations,
            "after":serde_json::to_string_pretty(&after).unwrap()}]);
        plan["links"] = json!([]);
    });
    let id = carried["plan_id"].as_str().unwrap();
    let plan: Value = serde_json::from_slice(
        &fs::read(root.join(format!(".central/bkmr/moves/{id}.json"))).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(root.join("Work/alpha/.central/bkmr/moves")).unwrap();
    fs::write(
        root.join(format!("Work/alpha/.central/bkmr/moves/{id}.seal.json")),
        serde_json::to_vec_pretty(
            &json!({"schema":"central.file-map-move-seal/v1","plan_id":id,
            "project":"alpha","digest":seal_digest(&plan)}),
        )
        .unwrap(),
    )
    .unwrap();
    let before = ground_but_locks(&world);
    let refused_here = |when: &str| {
        let refused = ctrl(&world, CARRIED[0].0, &carried, Some(PERSON));
        assert_eq!(refused["ok"], false, "{when}: {refused}");
        let words = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(
            words.contains(
                "This move's journal moves Work, which is Work itself, or a folder that holds it, and Central does not \
                 carry such a move out"
            ) && words.contains("Roll the move back to close its journal; nothing was moved"),
            "{when}: {words}"
        );
        assert!(
            root.join("Work/alpha/plan.md").is_file() && !root.join("Work2").exists(),
            "{when}: Work stands where it stood"
        );
    };
    refused_here("the apply");
    assert_eq!(ground_but_locks(&world), before, "nothing moves");
    let closed = ctrl(&world, CARRIED[1].0, &carried, Some(PERSON));
    assert_eq!(closed["ok"], true, "{closed}");
    assert_eq!(journal_state(&world, &carried), "rolled-back");
    refused_here("an apply after the rollback");
    for scope in [None, Some("alpha")] {
        let inspected = ctrl(
            &world,
            "central.file-map.inspect",
            &json!({"project":scope,"resources":false}),
            None,
        );
        assert_eq!(inspected["ok"], true, "{inspected}");
    }
}

/// The review of #352, F5: a lock that stands in a Work project that did not admit the move is still taken, and held while the
/// move runs. Beta's `file-map.lock` stands, as an act of beta's own left it. A plan of a move of the root's own, which reaches
/// no project, takes the file-map locks of its scopes and then the ground's `source-mutation.lock`, which this test holds: so
/// the plan stands still with whatever it has taken. Beta's lock is then asked for as another writer in beta would ask, and is
/// refused: the plan holds it. That is the lock's own answer, whatever the machine's load; the bound on the asking only ends
/// the test where the plan never takes the lock. Then the ground's lock is let go, the plan answers, and it made nothing in
/// beta.
#[test]
fn a_lock_that_stands_in_a_project_that_did_not_admit_the_move_is_still_taken() {
    use std::os::unix::io::AsRawFd;
    let world = a_world_of_two_projects();
    let root = world.root();
    let source = root_link(&world, "Control/user/note-link");
    fs::create_dir(root.join("Work/beta/.central")).unwrap();
    let betas = fs::File::create(root.join("Work/beta/.central/file-map.lock")).unwrap();
    let grounds = fs::OpenOptions::new()
        .write(true)
        .open(root.join(".central/source-mutation.lock"))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(grounds.as_raw_fd(), libc::LOCK_EX) },
        0
    );
    let in_beta = |world: &World| -> Vec<PathBuf> {
        works_tree(world)
            .into_iter()
            .map(|(path, _)| path)
            .filter(|path| path.starts_with(world.root().join("Work/beta")))
            .collect()
    };
    let before = in_beta(&world);
    let (held, planned) = std::thread::scope(|scope| {
        let planning = scope.spawn(|| {
            ctrl(
                &world,
                "central.file-map.move-plan",
                &json!({"source_ref":source,"destination":"Control/user/note2.md"}),
                Some(PERSON),
            )
        });
        let asked = std::time::Instant::now();
        let held = loop {
            if unsafe { libc::flock(betas.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                break std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK);
            }
            unsafe { libc::flock(betas.as_raw_fd(), libc::LOCK_UN) };
            if planning.is_finished() || asked.elapsed() > std::time::Duration::from_secs(20) {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(
            unsafe { libc::flock(grounds.as_raw_fd(), libc::LOCK_UN) },
            0
        );
        (held, planning.join().unwrap())
    });
    assert!(held, "the plan holds beta's lock while it runs: {planned}");
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(in_beta(&world), before, "the move makes nothing in beta");
}

/// The review of #352, the mutants it did not run: a scope the ground holds outside itself is held to the place the ground
/// keeps for it. The ground declares a folder scope, Music, and registers a project, far, both outside the ground. A move of
/// the root's own is planned, and its journal edited to say that Music, or far, stands at another folder outside the ground
/// (for far, a folder that holds a project of the same id). Apply and rollback are refused in words on the root's grant and
/// on every grant, and nothing is made in that folder or under `Work`.
#[test]
fn a_scope_outside_the_ground_that_a_journal_sets_elsewhere_is_refused() {
    for (respelled, elsewhere) in [("folder:Music", "Music2"), ("project:far", "far2")] {
        let world = granting(&every_act(), "control:root");
        declare_outside_scopes(&world);
        let other = world.0.join(elsewhere);
        project(&other, "far");
        let other = fs::canonicalize(&other).unwrap();
        let (carried, journal) = a_move_of_the_roots_own(&world);
        edit_journal(&journal, |plan| {
            let scope = plan["worlds"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|scope| scope["world"] == respelled)
                .expect("the journal lists the scope outside the ground");
            assert_eq!(scope["external"], true, "{scope}");
            scope["path"] = json!(other);
        });
        let holds = |folder: &Path| {
            let mut entries = Vec::new();
            let mut stack = vec![folder.to_path_buf()];
            while let Some(folder) = stack.pop() {
                for entry in fs::read_dir(&folder).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        stack.push(path.clone());
                    }
                    entries.push(path);
                }
            }
            entries.sort();
            entries
        };
        let (before, there) = (works_tree(&world), holds(&other));
        for grants in [vec!["control:root"], vec!["control:root", "project:alpha"]] {
            granted_at(&world, &grants);
            for (action, _) in CARRIED {
                let refused = ctrl(&world, action, &carried, Some(PERSON));
                assert_eq!(refused["ok"], false, "{respelled} {action}: {refused}");
                let words = refused["error"]["message"].as_str().unwrap_or_default();
                assert!(
                    words.contains("the ground holds no such scope there"),
                    "{respelled} {action}: {words}"
                );
                assert_eq!(
                    works_changes(&world, &before),
                    Vec::<String>::new(),
                    "{respelled} {action} on {grants:?}"
                );
                assert_eq!(holds(&other), there, "{respelled} {action} on {grants:?}");
            }
        }
    }
}
