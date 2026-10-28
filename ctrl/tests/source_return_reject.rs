//! Deny is always the person's (DLW §19.14, O-I #805): Central admits `projectcentral.source.return_reject` only on the
//! host's native credential (`CENTRAL_NATIVE_TOKEN`) for a human grant naming that exact Action and Project, as
//! `return_accept`'s commissioned maintenance does. An agent calling `ctrl` directly is refused, whatever it declares.
//! A comment on a Return (Approvals' Changes) keeps a declared `commented_by_ref` and stays open to a declared reviewer.
//! Test credentials and authority fixtures exist only in this disposable world.
use central_ctrl::continuous_work::source::Scope;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command};

const REJECT: &str = "projectcentral.source.return_reject";
const PERSON: &str = "return-reject-person-controlled-test-not-a-live-secret";
const AGENT: &str = "return-reject-agent-controlled-test-not-a-live-secret";
const ACCEPT_ONLY: &str = "return-reject-accept-only-controlled-test-not-a-live-secret";
const ELSEWHERE: &str = "return-reject-other-scope-controlled-test-not-a-live-secret";
const PROJECT_ID: &str = "return-reject-proof";

struct World(PathBuf);
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One `ctrl action run` as a separate process, with the credential (or none) in its environment only.
fn ctrl(world: &World, action: &str, input: &Value, token: Option<&str>) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ctrl"));
    command
        .args([
            "--json",
            "--root",
            world.0.to_str().unwrap(),
            "action",
            "run",
            action,
            &input.to_string(),
        ])
        .env("CENTRAL_ROOT", &world.0);
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

fn good(world: &World, action: &str, input: Value) -> Value {
    let result = ctrl(world, action, &input, None);
    assert_eq!(result["ok"], true, "{action}: {result}");
    result["data"].clone()
}

/// A Central root with one Project and the authored native-action-authority source the person's host credential is
/// checked against: the person's grant for Deny, an agent's grant for the same act, the person's grant for Approve only,
/// and the person's grant for Deny in another scope.
fn world() -> World {
    let world =
        World(std::env::temp_dir().join(format!("central-return-reject-{}", uuid::Uuid::new_v4())));
    central_ctrl::initialize_central(&world.0).unwrap();
    fs::create_dir(world.0.join("Work/Proof")).unwrap();
    good(
        &world,
        "projectcentral.init",
        json!({"project":"Proof","project_id":PROJECT_ID}),
    );
    let project_scope = format!("project:{PROJECT_ID}");
    let grant = |token: &str, kind: &str, action: &str, scope: &str| {
        json!({"principal_ref":format!("{kind}:return-reject-test"),"actor_kind":kind,
            "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":[scope],
            "actions":[action],"expires_at_unix_seconds":u64::MAX})
    };
    let grants = [
        grant(PERSON, "human", REJECT, &project_scope),
        grant(AGENT, "agent", REJECT, &project_scope),
        grant(
            ACCEPT_ONLY,
            "human",
            "projectcentral.source.return_accept",
            &project_scope,
        ),
        grant(ELSEWHERE, "human", REJECT, "control:root"),
    ];
    let authority = "Control/user/return-reject-authority.json";
    fs::write(
        world.0.join(authority),
        serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants})).unwrap(),
    )
    .unwrap();
    let relations = world.0.join("Control/relations/source-relations.json");
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    let mut document: Value = fs::read(&relations)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[]}));
    let scope = Scope::resolve(&world.0, None).unwrap();
    document["relations"].as_array_mut().unwrap().push(json!({
        "ref":scope.source_ref(authority),"path":authority,"roles":["native-action-authority"],
        "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
        "recognition":"controlled-test-only-not-personal-adoption","recorded_at_unix_seconds":1}));
    fs::write(&relations, serde_json::to_vec(&document).unwrap()).unwrap();
    world
}

/// An agent's pending Return over the Project's wiki notes; the return_ref and the record that keeps it.
fn pending_return(world: &World) -> (String, PathBuf) {
    let notes = world
        .0
        .join("Work/Proof/ProjectCentral/agents/wiki/notes.md");
    fs::create_dir_all(notes.parent().unwrap()).unwrap();
    fs::write(&notes, "the person's words\n").unwrap();
    let horizon = good(
        world,
        "projectcentral.change.horizon",
        json!({"project":"Proof"}),
    );
    let source = horizon["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["binding"]["path"] == "ProjectCentral/agents/wiki/notes.md")
        .expect("the notes participate in the Project's horizon")
        .clone();
    let proposal = good(
        world,
        "projectcentral.source.return",
        json!({"project":"Proof","source_ref":source["binding"]["ref"],
            "expected_revision":source["revision"]["revision"],"proposed_content":"an agent's words\n",
            "reason":"deny is the person's","evidence_refs":[],"agent_session_ref":"agent-session:return-reject"}),
    );
    let reference = proposal["proposal"]["return_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = reference.rsplit(':').next().unwrap();
    let record = world.0.join(format!(
        "Work/Proof/.central/source-returns/return-{id}.json"
    ));
    assert!(record.is_file(), "{}", record.display());
    (reference, record)
}

fn status(world: &World, reference: &str) -> Value {
    good(
        world,
        "projectcentral.source.return_read",
        json!({"project":"Proof","return_ref":reference}),
    )["proposal"]["status"]
        .clone()
}

#[test]
fn only_the_persons_native_credential_denies_a_return() {
    let world = world();
    let (reference, record) = pending_return(&world);
    let notes = world
        .0
        .join("Work/Proof/ProjectCentral/agents/wiki/notes.md");
    let deny = json!({"project":"Proof","return_ref":reference});

    // An agent calling ctrl directly: no credential, or one the person's grant does not cover. Each is refused in Central's
    // words, and the Return still waits, its record byte for byte.
    let before = fs::read(&record).unwrap();
    let declared = json!({"project":"Proof","return_ref":reference,"token":PERSON,"actor_kind":"human",
        "principal":{"actor_kind":"human"},"rejected_by_ref":"human:return-reject-test"});
    for (case, input, token) in [
        ("no credential", &deny, None),
        ("a declared person in the payload", &declared, None),
        ("an agent's credential for Deny", &deny, Some(AGENT)),
        (
            "the person's credential for Approve only",
            &deny,
            Some(ACCEPT_ONLY),
        ),
        (
            "the person's credential in another scope",
            &deny,
            Some(ELSEWHERE),
        ),
    ] {
        let refused = ctrl(&world, REJECT, input, token);
        assert_eq!(refused["ok"], false, "{case}: {refused}");
        assert_eq!(
            refused["status"], "unavailable_capability",
            "{case}: {refused}"
        );
        assert_eq!(
            fs::read(&record).unwrap(),
            before,
            "{case}: the record is untouched"
        );
        assert_eq!(
            status(&world, &reference),
            "pending",
            "{case}: the Return still waits"
        );
    }

    // Changes stays the declared reviewer's: a comment without the credential is kept, and the Return still waits.
    let commented = ctrl(
        &world,
        "projectcentral.source.return_comment",
        &json!({"project":"Proof","return_ref":reference,"comment":"Name the second source.",
            "commented_by_ref":"agent:return-reject-reviewer"}),
        None,
    );
    assert_eq!(commented["ok"], true, "{commented}");
    assert_eq!(commented["data"]["proposal"]["status"], "pending");

    // The person's own press: the credential the host holds, for this act and this Project.
    let denied = ctrl(&world, REJECT, &deny, Some(PERSON));
    assert_eq!(denied["ok"], true, "{denied}");
    assert_eq!(denied["data"]["proposal"]["status"], "rejected");
    assert_eq!(denied["data"]["authored_source_mutated"], false);
    assert_eq!(status(&world, &reference), "rejected");
    assert_eq!(fs::read_to_string(&notes).unwrap(), "the person's words\n");

    // A decided Return is not denied twice.
    let again = ctrl(&world, REJECT, &deny, Some(PERSON));
    assert_eq!(again["ok"], false, "{again}");
    assert_eq!(again["status"], "invalid_input", "{again}");
}
