//! World inhabitation, Central lane (O-I docs/contracts/WORLD-INHABITATION-V1.md
//! §1): `central.world.here`, Position definitions, and the Workcell root /
//! child NOW horizon — exercised through the real `ctrl` binary and the
//! native continuous-work dispatcher in disposable Worlds, never private
//! Control.
use central_ctrl::agent_set_store::RelationRecordStore;
use central_ctrl::continuous_work::{execute_at, execute_with_token_at, source::Scope};
use central_ctrl::{AgentProfile, AgentProfileScope, AgentProfileStore, WorldRef};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const AGENT: &str = "controlled-test-agent-credential-not-a-real-secret";

struct World(PathBuf);

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl World {
    /// Root with two Projects (`Work/one` = project:test/one, `Work/two` =
    /// project:test/two), a bare Work member without ProjectCentral, a
    /// recognised placement/time/authority policy and a machine declaration
    /// binding `workcell:local`.
    fn new(worktree_grants: &[&str]) -> Self {
        let path = unique("central-world-inhabitation");
        central_ctrl::initialize_central(&path).unwrap();
        let world = World(fs::canonicalize(&path).unwrap());
        for name in ["one", "two"] {
            let root = world.root().join(format!("Work/{name}"));
            fs::create_dir_all(root.join("ProjectCentral/user")).unwrap();
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(
                root.join("ProjectCentral/project.json"),
                serde_json::to_vec(&central_ctrl::ProjectCentralManifest::new(format!(
                    "test/{name}"
                )))
                .unwrap(),
            )
            .unwrap();
        }
        fs::create_dir_all(world.root().join("Work/bare/src")).unwrap();
        fs::create_dir_all(world.root().join("Control/machines")).unwrap();
        fs::write(
            world.root().join("Control/machines/current.json"),
            serde_json::to_vec_pretty(&json!({
                "schema":"central.machine","version":1,"role":"current","capabilities":[],
                "requirements":{"packages":[],"configurations":[],"services":[]},
                "bindings":[{"kind":"workcell","reference":"workcell:local"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut writable = vec![
            json!({"path":"Work/one","class":"repository"}),
            json!({"path":"Work/two","class":"repository"}),
        ];
        writable.extend(
            worktree_grants
                .iter()
                .map(|grant| json!({"path":grant,"class":"worktree"})),
        );
        let grants = json!([{
            "principal_ref":"agent:test","actor_kind":"agent",
            "token_sha256":format!("{:x}",Sha256::digest(AGENT.as_bytes())),
            "scope_refs":["control:root","project:test/one","project:test/two"],
            "actions":["central.day.ensure","central.now.lifecycle"],
            "expires_at_unix_seconds":u64::MAX
        }]);
        let policies = [
            (
                "placement.json",
                "work-placement-policy",
                json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root","writable":writable,"protected":[],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}),
            ),
            (
                "time.json",
                "civil-time-policy",
                json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}),
            ),
            (
                "authority.json",
                "native-action-authority",
                json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants}),
            ),
        ];
        let scope = Scope::resolve(world.root(), None).unwrap();
        let mut relations = vec![];
        for (name, role, value) in policies {
            let path = format!("Control/user/{name}");
            fs::write(
                world.root().join(&path),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
            relations.push(json!({"ref":scope.source_ref(&path),"path":path,"roles":[role],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"explicit-controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1}));
        }
        fs::create_dir_all(world.root().join("Control/relations")).unwrap();
        fs::write(
            world.root().join("Control/relations/source-relations.json"),
            serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap(),
        )
        .unwrap();
        world
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn invoke(&self, action: &str, input: Value) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_ctrl"))
            .args([
                "--json",
                "--root",
                self.root().to_str().unwrap(),
                "action",
                "run",
                action,
                &input.to_string(),
            ])
            .env_remove("CENTRAL_NATIVE_TOKEN")
            .output()
            .unwrap();
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{error}: {} / {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }

    fn here(&self, cwd: &Path) -> Value {
        let result = self.invoke("central.world.here", json!({"cwd": cwd}));
        assert_eq!(result["ok"], true, "{result}");
        result["data"].clone()
    }

    fn write(&self, relative: &str, value: &Value) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn policy_revision(&self, project: Option<&str>) -> Value {
        execute_at(self.root(), "policy", &json!({"project":project}), 100).unwrap()["revision"]
            .clone()
    }

    fn workcell_root(&self) -> Value {
        execute_at(
            self.root(),
            "workcell_root",
            &json!({"workcell_ref":"workcell:local"}),
            100,
        )
        .unwrap()
    }

    fn child(&self, project: Option<&str>, task: &str, parent: &Value) -> io::Result<Value> {
        execute_at(
            self.root(),
            "allocate",
            &json!({"project":project,"task_ref":task,"purpose":"bounded child work","expected_policy_revision":self.policy_revision(project),"parent_now_ref":parent,"workcell_ref":"workcell:local"}),
            100,
        )
    }

    fn record_bytes(&self, project: Option<&str>, allocation: &Value) -> Vec<u8> {
        let base = match project {
            Some(member) => self.root().join("Work").join(member),
            None => self.root().to_path_buf(),
        };
        fs::read(base.join(allocation["source"]["path"].as_str().unwrap())).unwrap()
    }
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository(path: &Path) {
    git(path, &["init", "-q"]);
    git(
        path,
        &["config", "user.email", "central-test@example.invalid"],
    );
    git(path, &["config", "user.name", "Central test"]);
    fs::write(path.join("tracked.txt"), "tracked\n").unwrap();
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-q", "-m", "fixture"]);
}

#[test]
fn world_here_places_a_path_in_the_local_and_project_world() {
    let world = World::new(&["worktrees/env-2/one", "worktrees/env-2/one/nested"]);
    repository(&world.root().join("Work/one"));
    repository(&world.root().join("Work/two"));
    let checkout = world.root().join("worktrees/env-2/one");
    fs::create_dir_all(checkout.parent().unwrap()).unwrap();
    git(
        &world.root().join("Work/one"),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            checkout.to_str().unwrap(),
        ],
    );
    let nested = checkout.join("nested");
    git(
        &world.root().join("Work/two"),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            nested.to_str().unwrap(),
        ],
    );

    // A Work member with ProjectCentral: present through cwd.
    let here = world.here(&world.root().join("Work/one/src"));
    assert_eq!(here["schema"], "central.world-here/v1");
    assert_eq!(here["cwd"]["relation"], "work-member");
    let project = &here["project_world"];
    assert_eq!(project["state"], "present", "{here}");
    assert_eq!(project["ref"], "project:test/one");
    assert_eq!(project["name"], "one");
    assert_eq!(project["path"], "Work/one");
    assert_eq!(project["projectcentral"], "Work/one/ProjectCentral");
    assert_eq!(project["parent_ref"], "control:root");
    assert_eq!(project["via"], "cwd");
    assert_eq!(project["roots"]["user"], "Work/one/ProjectCentral/user");
    assert!(project["missing_roots"]
        .as_array()
        .unwrap()
        .contains(&json!("governance")));

    // The Local World: identity, roots that exist, and the time policy basis
    // callers pass back to central.day.ensure.
    let local = &here["local_world"];
    assert_eq!(local["ref"], "control:root");
    assert_eq!(local["root"], world.root().to_str().unwrap());
    assert_eq!(local["roots"]["user"], "Control/user");
    let time = execute_at(world.root(), "time_policy", &json!({}), 100).unwrap();
    assert_eq!(local["time_policy"]["state"], "present");
    assert_eq!(local["time_policy"]["revision"], time["revision"]);
    assert_eq!(local["time_policy"]["ref"], time["source_ref"]);

    // Workcells come from the machine declarations, with their root NOW.
    assert_eq!(
        here["workcells"],
        json!([{"ref":"workcell:local","declared_by":"Control/machines/current.json","role":"current",
            "root_now":{"state":"absent","now_ref":here["workcells"][0]["root_now"]["now_ref"],
            "reason":"no Workcell root NOW is allocated; central.now.workcell-root ensures it"}}])
    );
    assert_eq!(here["world_record"]["state"], "absent");

    // A registered development worktree resolves through its repository grant.
    for cwd in [checkout.clone(), checkout.join("sub")] {
        fs::create_dir_all(&cwd).unwrap();
        let here = world.here(&cwd);
        assert_eq!(here["cwd"]["relation"], "registered-worktree", "{here}");
        assert_eq!(here["project_world"]["state"], "present", "{here}");
        assert_eq!(here["project_world"]["ref"], "project:test/one");
        assert_eq!(here["project_world"]["via"], "registered-worktree");
        assert_eq!(
            here["project_world"]["worktree"]["grant"],
            "worktrees/env-2/one"
        );
    }

    // Two Work members claim a nested checkout: ambiguous, with candidates.
    let here = world.here(&nested);
    assert_eq!(here["project_world"]["state"], "ambiguous", "{here}");
    let candidates = here["project_world"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|c| c["member"] == "one"));
    assert!(candidates.iter().any(|c| c["member"] == "two"));

    // A Work member without ProjectCentral, the root itself, Control, a path
    // outside Central and a path that does not exist are all results.
    let bare = world.here(&world.root().join("Work/bare/src"));
    assert_eq!(bare["project_world"]["state"], "absent");
    assert!(bare["project_world"]["reason"]
        .as_str()
        .unwrap()
        .contains("has no ProjectCentral"));
    for cwd in [
        world.root().to_path_buf(),
        world.root().join("Control/user"),
    ] {
        let here = world.here(&cwd);
        assert_eq!(here["cwd"]["relation"], "central-root", "{here}");
        assert_eq!(here["project_world"]["state"], "absent");
        assert!(here["project_world"]["reason"].is_string());
    }
    let outside = unique("central-world-inhabitation-outside");
    fs::create_dir_all(&outside).unwrap();
    let here = world.here(&outside);
    assert_eq!(here["cwd"]["relation"], "outside");
    assert_eq!(here["project_world"]["state"], "absent");
    fs::remove_dir_all(&outside).unwrap();
    let here = world.here(&world.root().join("Work/one/missing"));
    assert_eq!(here["project_world"]["state"], "unavailable");

    // `project` wins over cwd; cwd is still reported.
    let result = world.invoke(
        "central.world.here",
        json!({"cwd": world.root().join("Work/one/src"), "project": "two"}),
    );
    assert_eq!(result["data"]["project_world"]["ref"], "project:test/two");
    assert_eq!(result["data"]["project_world"]["via"], "input");
    assert_eq!(result["data"]["cwd"]["relation"], "work-member");
    let refused = world.invoke("central.world.here", json!({"project": "../escape"}));
    assert_eq!(refused["ok"], false);

    // The World's authored record: mismatch, then present, compared to the
    // identity the manifest derives.
    world.write(
        "Work/two/ProjectCentral/relations/worlds/world-stale.json",
        &json!({"schema":"central.world-relations/v1","ref":"project:TWO","revision":"w1","parent":"control:root","sources":[]}),
    );
    let two = world.root().join("Work/two/src");
    let here = world.here(&two);
    assert_eq!(here["world_record"]["state"], "mismatch", "{here}");
    assert_eq!(here["world_record"]["ref"], "project:TWO");
    assert_eq!(here["world_record"]["expected_ref"], "project:test/two");
    fs::remove_file(
        world
            .root()
            .join("Work/two/ProjectCentral/relations/worlds/world-stale.json"),
    )
    .unwrap();
    RelationRecordStore::worlds_in_project(world.root().join("Work/two"))
        .save(
            &json!({"schema":"central.world-relations/v1","ref":"project:test/two","revision":"w1","parent":"control:root","sources":[]}),
            None,
        )
        .unwrap();
    let here = world.here(&two);
    assert_eq!(here["world_record"]["state"], "present", "{here}");
    assert_eq!(here["world_record"]["ref"], "project:test/two");

    // Once ensured, the Workcell's root NOW is reported present.
    let root_now = world.workcell_root();
    let here = world.here(&two);
    assert_eq!(here["workcells"][0]["root_now"]["state"], "present");
    assert_eq!(
        here["workcells"][0]["root_now"]["now_ref"],
        root_now["now_ref"]
    );
}

fn position(world_ref: &str, slug: &str, extra: Value) -> Value {
    let mut record = json!({
        "schema": "central.world-position/v1",
        "ref": format!("central:position:{world_ref}:{slug}"),
        "revision": "r1",
        "slug": slug,
        "label": format!("{slug} label"),
        "enclosing_world_ref": world_ref,
    });
    for (key, value) in extra.as_object().unwrap() {
        record[key] = value.clone();
    }
    record
}

#[test]
fn positions_list_and_read_under_the_strict_definition() {
    let world = World::new(&[]);
    let root_profile = AgentProfile::new(
        "profile/root-steward",
        "r1",
        "agent/root-steward",
        AgentProfileScope::Personal,
        WorldRef::new("control:root").unwrap(),
    )
    .unwrap();
    AgentProfileStore::personal(world.root())
        .save(&root_profile, None)
        .unwrap();
    let project_profile = AgentProfile::new(
        "profile/factory-guardian",
        "r1",
        "agent/factory-guardian",
        AgentProfileScope::Project,
        WorldRef::new("project:test/one").unwrap(),
    )
    .unwrap();
    AgentProfileStore::project(world.root().join("Work/one"))
        .save(&project_profile, None)
        .unwrap();

    let root_dir = "Control/relations/positions";
    let one = "project:test/one";
    let one_dir = "Work/one/ProjectCentral/relations/positions";
    world.write(
        &format!("{root_dir}/root-steward.json"),
        &position(
            "control:root",
            "root-steward",
            json!({"handle":"@root-steward","profile_ref":"profile/root-steward","eligible_agent_refs":["agent/root-steward"]}),
        ),
    );
    world.write(
        &format!("{one_dir}/factory-guardian.json"),
        &position(
            one,
            "factory-guardian",
            json!({"handle":"@factory-guardian","profile_ref":"profile/factory-guardian","role_ref":"role:product-guardian",
                "purpose":"Steward the Software Factory product.","stewards_ref":"project:Factory",
                "eligible_agent_refs":["agent/factory-guardian"],"enclosing_co_internality_ref":null,"continuity_ref":null}),
        ),
    );
    // A Project Position may resolve its profile through the root ancestry.
    world.write(
        &format!("{one_dir}/inherits-profile.json"),
        &position(
            one,
            "inherits-profile",
            json!({"profile_ref":"profile/root-steward"}),
        ),
    );
    let invalid = [
        (
            "unknown-key",
            position(one, "unknown-key", json!({"colour":"blue"})),
        ),
        ("Bad_Slug", position(one, "Bad_Slug", json!({}))),
        ("wrong-name", position(one, "other-name", json!({}))),
        (
            "wrong-world",
            position("project:test/two", "wrong-world", json!({})),
        ),
        ("dup-a", position(one, "dup-a", json!({"handle":"@dup"}))),
        ("dup-b", position(one, "dup-b", json!({"handle":"@dup"}))),
        (
            "shadow",
            position(one, "shadow", json!({"handle":"@root-steward"})),
        ),
        (
            "ghost-profile",
            position(one, "ghost-profile", json!({"profile_ref":"profile/ghost"})),
        ),
        (
            "bad-handle",
            position(one, "bad-handle", json!({"handle":"no-at"})),
        ),
        ("wrong-ref", {
            let mut record = position(one, "wrong-ref", json!({}));
            record["ref"] = json!("central:position:project:test/one:something-else");
            record
        }),
    ];
    for (name, record) in &invalid {
        world.write(&format!("{one_dir}/{name}.json"), record);
    }
    fs::write(
        world.root().join(one_dir).join("notes.md"),
        "not a Position",
    )
    .unwrap();
    fs::write(world.root().join(one_dir).join(".hidden.json"), "{").unwrap();

    let listing = world.invoke("central.position.list", json!({"project":"one"}));
    assert_eq!(listing["ok"], true, "{listing}");
    let data = &listing["data"];
    assert_eq!(data["schema"], "central.position-listing/v1");
    assert_eq!(data["world_ref"], one);
    let refs = |key: &str| -> Vec<String> {
        data[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["record"]["ref"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        refs("positions"),
        vec![
            "central:position:project:test/one:factory-guardian",
            "central:position:project:test/one:inherits-profile"
        ]
    );
    assert_eq!(
        refs("inherited"),
        vec!["central:position:control:root:root-steward"]
    );
    let mut invalid_paths: Vec<String> = data["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            assert!(entry["error"].as_str().is_some_and(|e| !e.is_empty()));
            entry["path"].as_str().unwrap().to_owned()
        })
        .collect();
    invalid_paths.sort();
    let mut expected: Vec<String> = invalid
        .iter()
        .map(|(name, _)| format!("{one_dir}/{name}.json"))
        .collect();
    expected.sort();
    assert_eq!(invalid_paths, expected);

    let root_listing = world.invoke("central.position.list", json!({}));
    assert_eq!(root_listing["data"]["world_ref"], "control:root");
    assert_eq!(
        root_listing["data"]["positions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(root_listing["data"]["inherited"], json!([]));
    assert_eq!(root_listing["data"]["invalid"], json!([]));

    // Read resolves the World from the ref itself; the record is returned as
    // authored with its source basis.
    let read = world.invoke(
        "central.position.read",
        json!({"position_ref":"central:position:project:test/one:factory-guardian"}),
    );
    assert_eq!(read["ok"], true, "{read}");
    assert_eq!(read["data"]["schema"], "central.position-reading/v1");
    assert_eq!(read["data"]["record"]["handle"], "@factory-guardian");
    assert_eq!(
        read["data"]["record"]["enclosing_co_internality_ref"],
        Value::Null
    );
    assert_eq!(
        read["data"]["source"]["ref"],
        "central:source:project:test/one:ProjectCentral/relations/positions/factory-guardian.json"
    );
    assert!(read["data"]["source"]["revision"]
        .as_str()
        .unwrap()
        .starts_with("central.content-fnv1a64/v1:"));
    let root_read = world.invoke(
        "central.position.read",
        json!({"position_ref":"central:position:control:root:root-steward"}),
    );
    assert_eq!(root_read["ok"], true, "{root_read}");

    // Refusals are three-part and coded.
    let refused = world.invoke(
        "central.position.read",
        json!({"position_ref":"central:position:project:test/one:unknown-key"}),
    );
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "central.position_invalid");
    let details = &refused["error"]["details"];
    for part in ["fact", "consequence", "action"] {
        let text = details[part].as_str().unwrap();
        assert!(!text.is_empty());
        assert!(refused["error"]["message"].as_str().unwrap().contains(text));
    }
    assert!(details["fact"].as_str().unwrap().contains("colour"));
    assert_eq!(refused["error"]["repair_hint"], details["action"]);
    let missing = world.invoke(
        "central.position.read",
        json!({"position_ref":"central:position:project:test/one:nobody"}),
    );
    assert_eq!(missing["error"]["code"], "central.position_not_found");
    assert!(missing["error"]["details"]["action"]
        .as_str()
        .unwrap()
        .contains("central.position.list"));
    let unknown_world = world.invoke(
        "central.position.read",
        json!({"position_ref":"central:position:project:absent:nobody"}),
    );
    assert_eq!(unknown_world["error"]["code"], "central.position_not_found");
    let malformed = world.invoke(
        "central.position.read",
        json!({"position_ref":"position:factory-guardian"}),
    );
    assert_eq!(malformed["ok"], false);
    assert_eq!(malformed["error"]["code"], "invalid_input");
}

#[test]
fn workcell_root_and_child_nows_form_one_horizon() {
    let world = World::new(&[]);
    let root_now = world.workcell_root();
    assert_eq!(root_now["created"], true, "{root_now}");
    let again = world.workcell_root();
    assert_eq!(again["created"], false);
    assert_eq!(again["now_ref"], root_now["now_ref"]);
    assert_eq!(again["revision"], root_now["revision"]);
    let record = &root_now["record"];
    assert_eq!(
        record["task_ref"],
        "central:task:control:root:workcell-root:workcell:local"
    );
    assert_eq!(record["horizon"], "workcell-root");
    assert_eq!(record["workcell_ref"], "workcell:local");
    assert!(record.get("parent_now_ref").is_none());
    // Through the CLI the same call converges on the same clearing.
    let cli = world.invoke(
        "central.now.workcell-root",
        json!({"workcell_ref":"workcell:local"}),
    );
    assert_eq!(cli["ok"], true, "{cli}");
    assert_eq!(cli["data"]["now_ref"], root_now["now_ref"]);
    for refused in [
        json!({"workcell_ref":"workcell:local","project":"one"}),
        json!({"workcell_ref":"local"}),
        json!({"workcell_ref":"workcell:"}),
    ] {
        let result = world.invoke("central.now.workcell-root", refused.clone());
        assert_eq!(result["ok"], false, "{refused}: {result}");
    }

    let parent = &root_now["now_ref"];
    let project_child = world
        .child(Some("one"), "task:project-child", parent)
        .unwrap();
    assert_eq!(project_child["record"]["horizon"], "child");
    assert_eq!(project_child["record"]["parent_now_ref"], *parent);
    assert_eq!(project_child["record"]["workcell_ref"], "workcell:local");
    let replay = world
        .child(Some("one"), "task:project-child", parent)
        .unwrap();
    assert_eq!(replay["created"], false);
    let root_children: Vec<Value> = (0..12)
        .map(|index| {
            world
                .child(None, &format!("task:root-child-{index}"), parent)
                .unwrap()
        })
        .collect();
    let grandchild = world
        .child(Some("one"), "task:grandchild", &project_child["now_ref"])
        .unwrap();

    // Parent law: same scope or root scope, already allocated, never itself,
    // and the allocation basis includes the parent.
    let error = world
        .child(Some("two"), "task:cross-project", &project_child["now_ref"])
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{error}");
    let absent_parent = format!("central:now:control:root:{}", "0".repeat(64));
    let error = world
        .child(None, "task:orphan", &json!(absent_parent))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound, "{error}");
    let own = format!(
        "central:now:control:root:{:x}",
        Sha256::digest("task:self".as_bytes())
    );
    let error = world.child(None, "task:self", &json!(own)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{error}");
    let error = world
        .child(
            Some("one"),
            "task:project-child",
            &root_children[0]["now_ref"],
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{error}");

    // central.now.children: uncapped, across the root register and Projects.
    let children = world.invoke("central.now.children", json!({"now_ref": parent}));
    assert_eq!(children["ok"], true, "{children}");
    let data = &children["data"];
    assert_eq!(data["schema"], "central.now-children/v1");
    assert_eq!(data["count"], 13);
    assert_eq!(data["unscanned"], json!([]));
    let listed: Vec<&Value> = data["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| &row["now_ref"])
        .collect();
    assert!(listed.contains(&&project_child["now_ref"]));
    for child in &root_children {
        assert!(listed.contains(&&child["now_ref"]));
    }
    assert!(!listed.contains(&&grandchild["now_ref"]));
    assert!(data["children"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["live"] == true && row["horizon"] == "child"));
    let grandchildren = world.invoke(
        "central.now.children",
        json!({"project":"one","now_ref": project_child["now_ref"]}),
    );
    assert_eq!(grandchildren["data"]["count"], 1, "{grandchildren}");

    // central.now.read and central.now.list expose the horizon fields.
    let read = world.invoke(
        "central.now.read",
        json!({"project":"one","now_ref": project_child["now_ref"]}),
    );
    assert_eq!(read["data"]["record"]["parent_now_ref"], *parent);
    assert_eq!(read["data"]["record"]["horizon"], "child");
    assert_eq!(read["data"]["record"]["workcell_ref"], "workcell:local");
    let standalone = execute_at(
        world.root(),
        "allocate",
        &json!({"task_ref":"task:standalone","purpose":"no horizon","expected_policy_revision":world.policy_revision(None)}),
        100,
    )
    .unwrap();
    let list = world.invoke("central.now.list", json!({}));
    for row in list["data"]["records"].as_array().unwrap() {
        if row["now_ref"] == standalone["now_ref"] {
            assert!(row.get("horizon").is_none() && row.get("parent_now_ref").is_none());
        } else if row["now_ref"] == *parent {
            assert_eq!(row["horizon"], "workcell-root");
        } else {
            assert_eq!(row["horizon"], "child");
        }
    }
}

#[test]
fn day_boundaries_carry_the_workcell_horizon_untouched() {
    let world = World::new(&[]);
    let root_now = world.workcell_root();
    let active = world
        .child(Some("one"), "task:active-child", &root_now["now_ref"])
        .unwrap();
    let resting = world
        .child(Some("one"), "task:resting-child", &root_now["now_ref"])
        .unwrap();
    let rest = execute_with_token_at(
        world.root(),
        "now_lifecycle",
        &json!({"project":"one","now_ref":resting["now_ref"],"expected_revision":resting["revision"]["revision"],
            "expected_policy_revision":world.policy_revision(Some("one")),"lifecycle":"quiescent"}),
        Some(AGENT),
        100,
    )
    .unwrap();
    assert_eq!(rest["record"]["lifecycle"], "quiescent");
    let snapshot = |world: &World| {
        [
            world.record_bytes(None, &root_now),
            world.record_bytes(Some("one"), &active),
            world.record_bytes(Some("one"), &resting),
        ]
    };
    let before = snapshot(&world);

    // Root and Project Day rollover (central.day.ensure).
    let now = 1_790_000_000;
    for project in [None, Some("one")] {
        let time = execute_at(
            world.root(),
            "time_policy",
            &json!({"project":project}),
            now,
        )
        .unwrap();
        let day = execute_with_token_at(
            world.root(),
            "day_ensure",
            &json!({"project":project,"expected_time_policy_revision":time["revision"]}),
            Some(AGENT),
            now,
        )
        .unwrap();
        assert_eq!(day["now_cleared_or_archived"], false);
        let horizon = &day["now_horizon"];
        assert_eq!(horizon["clearings_closed_completed_or_archived"], false);
        if project.is_none() {
            assert_eq!(
                horizon["carried"][0]["now_ref"], root_now["now_ref"],
                "{day}"
            );
            assert_eq!(horizon["released"], json!([]));
        } else {
            assert_eq!(horizon["carried"][0]["now_ref"], active["now_ref"], "{day}");
            assert_eq!(horizon["released"][0]["now_ref"], resting["now_ref"]);
        }
    }

    // Project day close (projectcentral.now.rollover).
    let project_root = world.root().join("Work/one");
    central_ctrl::initialize_now(&project_root).unwrap();
    let report = central_ctrl::rollover_now(&project_root, "2026-09-23", "2026-09-24").unwrap();
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(
        report["now_horizon"]["carried"][0]["now_ref"], active["now_ref"],
        "{report}"
    );
    assert_eq!(
        report["now_horizon"]["released"][0]["now_ref"],
        resting["now_ref"]
    );

    // Nothing was closed, completed or archived: every byte is unchanged.
    assert_eq!(snapshot(&world), before);
    let children = world.invoke(
        "central.now.children",
        json!({"now_ref": root_now["now_ref"]}),
    );
    let rows = children["data"]["children"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|row| row["now_ref"] == resting["now_ref"] && row["live"] == false));
}

/// The Central repository's own ProjectCentral world record must carry the
/// identity its manifest derives (`project:<project_id>`), stored under the
/// file name that identity keys — the drift `central.world.here` reports as
/// `mismatch`.
#[test]
fn repository_projectcentral_world_record_matches_its_manifest_identity() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let manifest = central_ctrl::read_project_manifest(repository).unwrap();
    let records = RelationRecordStore::worlds_in_project(repository)
        .list()
        .unwrap();
    assert!(!records.is_empty());
    for record in records {
        assert_eq!(record.ref_, format!("project:{}", manifest.project_id));
    }
}
