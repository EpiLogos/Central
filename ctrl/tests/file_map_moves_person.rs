//! A move in the file map is the person's own act (DLW build ruling 2026-11-09, round 53 item 2; O-I 0G's review): Central admits
//! `central.file-map.move-plan`, `central.file-map.move-apply` and `central.file-map.move-rollback` only on the host's native
//! credential (`CENTRAL_NATIVE_TOKEN`) for a human grant naming that exact Action at the root, as its other owner-authority acts
//! do. Anything that runs `ctrl` directly with no credential, a credential no grant names, or an agent's credential is refused in
//! Central's words, and nothing is planned or moved. `move-plan` is no read: Central classes it locally-mutating, and it writes the
//! durable journal that names the place a file goes, which apply and rollback then carry out by its id alone.
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
    let world = World(
        std::env::temp_dir().join(format!("central-file-map-moves-{}", uuid::Uuid::new_v4())),
    );
    fs::create_dir_all(world.0.join("home")).unwrap();
    let root = world.root();
    central_ctrl::initialize_central(&root).unwrap();
    fs::write(root.join("Control/user/note.md"), "the person's page\n").unwrap();
    let grant = |token: &str, kind: &str| {
        json!({"principal_ref":format!("{kind}:file-map-moves-test"),"actor_kind":kind,
            "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":["control:root"],
            "actions":MOVES,"expires_at_unix_seconds":u64::MAX})
    };
    let authority = "Control/user/file-map-moves-authority.json";
    fs::write(
        root.join(authority),
        serde_json::to_vec(
            &json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root",
            "grants":[grant(PERSON, "human"), grant(AGENT, "agent")]}),
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
