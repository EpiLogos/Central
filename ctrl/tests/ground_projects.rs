//! A project the person brings into a ground Setup founded (O-I #740, lane D): on the held Accept that makes it, their grant covers
//! `project:<id>` with exactly the Actions that take a project scope, and the project's Work member — nothing else — becomes writable.
//! Only the authority and placement Setup founded are extended; one recognised otherwise (the owner's own) is refused by name and
//! nothing is written. Disposable grounds; the test token is no live credential.
use central_ctrl::continuous_work::authority::{
    self, AUTHENTICATED_ACTIONS, HOST_AUTHENTICATED_ACTIONS, PROJECT_AUTHENTICATED_ACTIONS,
};
use central_ctrl::continuous_work::{execute_with_token_at, source::Scope};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const TOKEN: &str = "ground-projects-test-token-not-a-live-credential-0001";
const OTHER: &str = "another-credential-that-is-not-the-persons-0000000000";
const PRINCIPAL: &str = "human:scratch-world";
const FOUNDING: &str = "oi-setup-adoption:review-token-under-test";
const PROJECTS: &str = "oi-setup-projects:field-notes-under-test";
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
fn made() -> Home {
    let path = std::env::temp_dir().join(format!(
        "central-ground-projects-{}-{}-{}",
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
/// A folder of Work with something in it, made a project (projectcentral.init, as Setup's Create presses it).
fn project(home: &Home, name: &str) {
    let folder = home.ground().join("Work").join(name);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("README.md"), "# notes\n").unwrap();
    let made = cli(
        home,
        "projectcentral.init",
        json!({"project":name,"project_id":name}),
    );
    assert_eq!(made["ok"], true, "{made}");
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}
/// The writable destinations a scope's effective placement grants, relative to the ground.
fn writable(home: &Home, project: Option<&str>) -> Vec<String> {
    let policy = execute_with_token_at(
        &home.ground(),
        "policy",
        &json!({ "project": project }),
        None,
        now(),
    )
    .unwrap();
    policy["writable_destinations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            Path::new(d["path"].as_str().unwrap())
                .strip_prefix(home.ground())
                .unwrap()
                .display()
                .to_string()
        })
        .collect()
}
/// Whether an agent's repository write at `destination` is placed, under a NOW allocated in `project`.
fn repository_write(home: &Home, project: &str, destination: &str) -> Value {
    let policy = execute_with_token_at(
        &home.ground(),
        "policy",
        &json!({ "project": project }),
        None,
        now(),
    )
    .unwrap();
    let now_record = execute_with_token_at(
        &home.ground(),
        "allocate",
        &json!({"project":project,"task_ref":format!("central:task:project:{project}:lane-d"),"purpose":"write in the project",
            "expected_policy_revision":policy["revision"]}),
        None,
        now(),
    )
    .unwrap();
    execute_with_token_at(
        &home.ground(),
        "validate",
        &json!({"project":project,"now_ref":now_record["now_ref"],"expected_now_revision":now_record["revision"]["revision"],
            "expected_policy_revision":policy["revision"],"destination":destination}),
        None,
        now(),
    )
    .unwrap()
}
fn day_in(home: &Home, project: &str, token: &str) -> Result<Value, String> {
    let time = execute_with_token_at(
        &home.ground(),
        "time_policy",
        &json!({ "project": project }),
        None,
        now(),
    )
    .unwrap();
    execute_with_token_at(
        &home.ground(),
        "day_ensure",
        &json!({"project":project,"expected_time_policy_revision":time["revision"]}),
        Some(token),
        now(),
    )
    .map_err(|e| e.to_string())
}

#[test]
fn a_project_grant_holds_exactly_the_authenticated_actions_that_name_a_project_and_the_host_checked(
) {
    // Derived from the code: the authenticated Actions whose registered descriptor takes a `project`.
    let registry = central_ctrl::cli::create_runtime_action_registry();
    let listed = registry.list();
    let derived: Vec<&str> = AUTHENTICATED_ACTIONS
        .iter()
        .copied()
        .filter(|action| {
            listed
                .iter()
                .find(|row| row.id == *action)
                .unwrap_or_else(|| panic!("{action} is no registered Action"))
                .inputs
                .iter()
                .any(|input| input.name == "project")
        })
        .collect();
    assert_eq!(
        derived, PROJECT_AUTHENTICATED_ACTIONS,
        "the project list is the code's"
    );
    // A host-checked Action is no Action of Central's, and Central never authenticates it.
    for action in HOST_AUTHENTICATED_ACTIONS {
        assert!(listed.iter().all(|row| row.id != *action), "{action}");
        assert!(!AUTHENTICATED_ACTIONS.contains(action), "{action}");
    }
    assert_eq!(
        central_ctrl::ground_projects::project_actions(),
        [PROJECT_AUTHENTICATED_ACTIONS, HOST_AUTHENTICATED_ACTIONS].concat()
    );
}

/// A grant covering both the root and a project, of every Action named: written straight into a disposable ground.
fn granted(home: &Home, scopes: &[&str], actions: &[&str], recognition: &str) {
    let root = home.ground();
    let scope = Scope::resolve(&root, None).unwrap();
    let source = "Control/user/native-action-authority.json";
    fs::write(root.join(source), serde_json::to_vec(&json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
        {"principal_ref":PRINCIPAL,"actor_kind":"human","token_sha256":sha(TOKEN),"scope_refs":scopes,"actions":actions,"expires_at_unix_seconds":4_000_000_000u64}]})).unwrap()).unwrap();
    fs::create_dir_all(root.join("Control/relations")).unwrap();
    let path = root.join("Control/relations/source-relations.json");
    let mut relations: Value = fs::read(&path)
        .ok()
        .map(|raw| serde_json::from_slice(&raw).unwrap())
        .unwrap_or_else(|| json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[]}));
    relations["relations"].as_array_mut().unwrap().push(json!({"ref":scope.source_ref(source),"path":source,"roles":["native-action-authority"],
        "provenance":"human-adopted","standing":"architecture-contract","treatment":"control-user","recognition":recognition,"recorded_at_unix_seconds":1}));
    fs::write(path, serde_json::to_vec(&relations).unwrap()).unwrap();
}

#[test]
fn an_action_central_authenticates_only_at_the_root_never_authenticates_at_a_project() {
    let home = made();
    fs::create_dir_all(home.ground().join("Work/field-notes")).unwrap();
    central_ctrl::projectcentral_ops::initialize_projectcentral(
        &home.ground(),
        &home.ground().join("Work/field-notes"),
        "field-notes",
    )
    .unwrap();
    let named: Vec<&str> = AUTHENTICATED_ACTIONS
        .iter()
        .chain(HOST_AUTHENTICATED_ACTIONS)
        .copied()
        .collect();
    granted(
        &home,
        &["control:root", "project:field-notes"],
        &named,
        "controlled-test-only",
    );
    let root = Scope::resolve(&home.ground(), None).unwrap();
    let project = Scope::resolve(&home.ground(), Some("field-notes")).unwrap();
    for action in AUTHENTICATED_ACTIONS {
        assert!(
            authority::authenticate(&root, Some(TOKEN), action, None, 1_800_000_000).is_ok(),
            "{action} at the root"
        );
        let at_project =
            authority::authenticate(&project, Some(TOKEN), action, None, 1_800_000_000);
        if PROJECT_AUTHENTICATED_ACTIONS.contains(action) {
            assert!(at_project.is_ok(), "{action} at the project");
        } else {
            let refused = at_project.unwrap_err().to_string();
            assert!(
                refused.contains("PROJECT_AUTHENTICATED_ACTIONS"),
                "{action}: {refused}"
            );
        }
    }
    // The host's own Action is never Central's to authenticate, wherever a grant names it.
    for scope in [&root, &project] {
        for action in HOST_AUTHENTICATED_ACTIONS {
            let refused = authority::authenticate(scope, Some(TOKEN), action, None, 1_800_000_000)
                .unwrap_err()
                .to_string();
            assert!(refused.contains("AUTHENTICATED_ACTIONS"), "{refused}");
        }
    }
}

#[test]
fn a_project_made_in_a_setup_ground_is_the_person_s_to_act_in_and_its_member_alone_becomes_writable(
) {
    let home = founded();
    fs::create_dir_all(home.ground().join("Work/field-notes")).unwrap();
    fs::write(
        home.ground().join("Work/field-notes/README.md"),
        "# notes\n",
    )
    .unwrap();
    // Previewed before Accept: the folder is no project yet, so the preview names the id the Accept makes it with.
    let preview = cli(
        &home,
        "central.ground.project.preview",
        json!({"project":"field-notes","project_id":"field-notes"}),
    );
    assert_eq!(preview["ok"], true, "{preview}");
    let data = &preview["data"];
    assert_eq!(data["schema"], "central.ground-project-preview/v1");
    assert_eq!(data["state"], "absent");
    assert_eq!(data["project_made"], false);
    assert_eq!(data["scope_ref"], "project:field-notes");
    assert_eq!(data["grant"]["principal_ref"], PRINCIPAL);
    assert_eq!(
        data["grant"]["actions"],
        json!([PROJECT_AUTHENTICATED_ACTIONS, HOST_AUTHENTICATED_ACTIONS].concat())
    );
    assert_eq!(
        data["writable"],
        json!({"path":"Work/field-notes","class":"repository"})
    );
    assert!(
        !preview.to_string().contains(&sha(TOKEN)),
        "no credential digest"
    );
    let digest = data["digest"].as_str().unwrap().to_owned();

    // The same held Accept makes the project, then extends to it.
    project(&home, "field-notes");
    project(&home, "elsewhere");
    // Before: nothing the person presses there authenticates; no repository write is placed; nothing is writable.
    assert!(day_in(&home, "field-notes", TOKEN)
        .unwrap_err()
        .contains("lacks this exact scope/action"));
    assert_eq!(
        repository_write(&home, "field-notes", "Work/field-notes/README.md")["allowed"],
        false
    );
    assert!(writable(&home, None).is_empty());
    let authority_before = read(
        &home
            .ground()
            .join("Control/user/native-action-authority.json"),
    );

    let kept = cli(
        &home,
        "central.ground.project.adopt",
        accept(
            PROJECTS,
            json!({"project":"field-notes","project_id":"field-notes","expected_digest":digest}),
        ),
    );
    assert_eq!(kept["ok"], true, "{kept}");
    assert_eq!(kept["data"]["adopted"], true);
    assert_eq!(kept["data"]["writable"], json!(["Work/field-notes"]));
    assert!(
        !kept.to_string().contains(&sha(TOKEN)),
        "no credential digest"
    );
    assert_ne!(
        read(
            &home
                .ground()
                .join("Control/user/native-action-authority.json")
        ),
        authority_before
    );

    // After: the person acts as themselves there, and no other credential does.
    let day = day_in(&home, "field-notes", TOKEN).unwrap();
    assert_eq!(day["created"], true);
    assert!(day_in(&home, "field-notes", OTHER)
        .unwrap_err()
        .contains("not authorised"));
    // Its agents may write inside it, and nowhere else: the root and every other project grant only this member.
    assert_eq!(
        repository_write(&home, "field-notes", "Work/field-notes/README.md")["allowed"],
        true
    );
    assert_eq!(writable(&home, None), ["Work/field-notes"]);
    assert_eq!(writable(&home, Some("field-notes")), ["Work/field-notes"]);
    assert!(writable(&home, Some("elsewhere")).is_empty());
    assert_eq!(
        repository_write(&home, "elsewhere", "Work/elsewhere/README.md")["allowed"],
        false
    );
    assert!(day_in(&home, "elsewhere", TOKEN)
        .unwrap_err()
        .contains("lacks this exact scope/action"));
    // Each source keeps every Accept that shaped it.
    let relations: Value = serde_json::from_slice(&read(
        &home
            .ground()
            .join("Control/relations/source-relations.json"),
    ))
    .unwrap();
    for path in [
        "Control/user/native-action-authority.json",
        "Control/user/placement.json",
    ] {
        let relation = relations["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["path"] == path)
            .unwrap();
        assert_eq!(
            relation["recognition"],
            format!("setup-ground-acceptance:{FOUNDING};setup-ground-acceptance:{PROJECTS}"),
            "{path}"
        );
    }
    // Extended once: the preview says so, and a second adopt writes nothing.
    let again = cli(
        &home,
        "central.ground.project.preview",
        json!({"project":"field-notes"}),
    );
    assert_eq!(again["data"]["state"], "present", "{again}");
    assert!(again["data"]["digest"].is_null());
    let before = read(
        &home
            .ground()
            .join("Control/user/native-action-authority.json"),
    );
    let refused = cli(
        &home,
        "central.ground.project.adopt",
        accept(
            PROJECTS,
            json!({"project":"field-notes","expected_digest":digest}),
        ),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(
        read(
            &home
                .ground()
                .join("Control/user/native-action-authority.json")
        ),
        before
    );
}

#[test]
fn an_authority_or_placement_setup_did_not_found_is_refused_by_name_and_nothing_is_written() {
    // The owner's shape: a hand-authored grant at the root and several projects, recognised by the owner's own commission.
    let home = made();
    project(&home, "field-notes");
    granted(
        &home,
        &["control:root", "project:Factory", "project:O-I"],
        AUTHENTICATED_ACTIONS,
        "owner-commissioned-native-day-enablement",
    );
    let scope = Scope::resolve(&home.ground(), None).unwrap();
    let placement = "Control/user/placement.json";
    fs::write(home.ground().join(placement), serde_json::to_vec(&json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root",
        "writable":[],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":3600})).unwrap()).unwrap();
    let relations_path = home
        .ground()
        .join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&read(&relations_path)).unwrap();
    relations["relations"].as_array_mut().unwrap().push(json!({"ref":scope.source_ref(placement),"path":placement,"roles":["work-placement-policy"],
        "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
        "recognition":"owner-commissioned-convergence-adoption","recorded_at_unix_seconds":1}));
    fs::write(&relations_path, serde_json::to_vec(&relations).unwrap()).unwrap();
    let kept = |path: &str| read(&home.ground().join(path));
    let before = [
        kept("Control/user/native-action-authority.json"),
        kept(placement),
        read(&relations_path),
    ];
    let preview = cli(
        &home,
        "central.ground.project.preview",
        json!({"project":"field-notes"}),
    );
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
        "central.ground.project.adopt",
        accept(
            PROJECTS,
            json!({"project":"field-notes","expected_digest":"0".repeat(64)}),
        ),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("changed only by its owner"));
    assert_eq!(
        [
            kept("Control/user/native-action-authority.json"),
            kept(placement),
            read(&relations_path)
        ],
        before
    );
    // A Setup authority beside a hand-authored placement: the placement is named, and the authority is not touched either.
    let other = founded();
    project(&other, "field-notes");
    let placed = other
        .ground()
        .join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&read(&placed)).unwrap();
    for relation in relations["relations"].as_array_mut().unwrap() {
        if relation["path"] == placement {
            relation["recognition"] = json!(format!(
                "setup-ground-acceptance:{FOUNDING};owner-amended-by-hand"
            ));
        }
    }
    fs::write(&placed, serde_json::to_vec(&relations).unwrap()).unwrap();
    let authority = read(
        &other
            .ground()
            .join("Control/user/native-action-authority.json"),
    );
    let preview = cli(
        &other,
        "central.ground.project.preview",
        json!({"project":"field-notes"}),
    );
    assert_eq!(preview["data"]["state"], "hand-authored", "{preview}");
    assert!(preview["data"]["reason"]
        .as_str()
        .unwrap()
        .contains("work-placement-policy"));
    assert_eq!(
        read(
            &other
                .ground()
                .join("Control/user/native-action-authority.json")
        ),
        authority
    );
}

#[test]
fn the_extension_needs_the_person_s_acceptance_a_made_project_and_the_previewed_digest() {
    let home = founded();
    fs::create_dir_all(home.ground().join("Work/field-notes")).unwrap();
    let preview = cli(
        &home,
        "central.ground.project.preview",
        json!({"project":"field-notes","project_id":"field-notes"}),
    );
    let digest = preview["data"]["digest"].as_str().unwrap().to_owned();
    let good = json!({"project":"field-notes","project_id":"field-notes","expected_digest":digest});
    let sources = || {
        [
            read(
                &home
                    .ground()
                    .join("Control/user/native-action-authority.json"),
            ),
            read(&home.ground().join("Control/user/placement.json")),
        ]
    };
    let before = sources();
    // Not made yet: nothing is extended to a folder that is no project.
    let unmade = cli(
        &home,
        "central.ground.project.adopt",
        accept(PROJECTS, good.clone()),
    );
    assert_eq!(unmade["ok"], false, "{unmade}");
    assert!(unmade["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no project yet"));
    project(&home, "field-notes");
    let mut cases = vec![
        json!({"actor_kind":"agent","acceptance":"human-accepted","acceptance_ref":PROJECTS}),
        json!({"actor_kind":"human","acceptance_ref":PROJECTS}),
        json!({"actor_kind":"human","acceptance":"human-accepted"}),
    ]
    .into_iter()
    .map(|gate| {
        let mut input = good.clone();
        for (k, v) in gate.as_object().unwrap() {
            input[k] = v.clone();
        }
        input
    })
    .collect::<Vec<_>>();
    for (field, value) in [
        ("expected_digest", json!("0".repeat(64))),
        ("project_id", json!("another-id")),
        ("project", json!("../Control")),
        ("project", json!("Work/field-notes")),
        ("project", json!("elsewhere")),
    ] {
        let mut input = accept(PROJECTS, good.clone());
        input[field] = value;
        cases.push(input);
    }
    let mut session = accept(PROJECTS, good.clone());
    session["agent_session_ref"] = json!("session:1");
    cases.push(session);
    for input in cases {
        let refused = cli(&home, "central.ground.project.adopt", input.clone());
        assert_eq!(refused["ok"], false, "{input}: {refused}");
    }
    assert_eq!(sources(), before, "nothing was written");
    // The previewed digest, pressed once the project is made, extends it.
    let kept = cli(
        &home,
        "central.ground.project.adopt",
        accept(PROJECTS, good),
    );
    assert_eq!(kept["ok"], true, "{kept}");
}

#[test]
fn the_owner_s_grant_shape_still_authorises_every_act_it_did() {
    // Fixture of the owner's grant shape (never the real file): one human grant at the root and seven projects, of every Action
    // Central authenticated before this lane but the host's own. The project gate takes nothing from it.
    let owner_projects = [
        "Actuation",
        "Central",
        "Factory",
        "O-I",
        "Workcell",
        "ai-kit",
        "quaternal-logic",
    ];
    let home = made();
    for name in owner_projects {
        fs::create_dir_all(home.ground().join("Work").join(name)).unwrap();
        central_ctrl::projectcentral_ops::initialize_projectcentral(
            &home.ground(),
            &home.ground().join("Work").join(name),
            name,
        )
        .unwrap();
    }
    let actions: Vec<&str> = AUTHENTICATED_ACTIONS
        .iter()
        .copied()
        .filter(|a| *a != "central.flow.append")
        .collect();
    let scopes: Vec<String> = std::iter::once("control:root".to_owned())
        .chain(owner_projects.iter().map(|p| format!("project:{p}")))
        .collect();
    granted(
        &home,
        &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
        &actions,
        "owner-commissioned-native-day-enablement",
    );
    let root = Scope::resolve(&home.ground(), None).unwrap();
    for action in &actions {
        assert!(
            authority::authenticate(&root, Some(TOKEN), action, None, 1_800_000_000).is_ok(),
            "{action} at the root"
        );
    }
    for name in owner_projects {
        let project = Scope::resolve(&home.ground(), Some(name)).unwrap();
        for action in actions
            .iter()
            .filter(|a| PROJECT_AUTHENTICATED_ACTIONS.contains(a))
        {
            assert!(
                authority::authenticate(&project, Some(TOKEN), action, None, 1_800_000_000).is_ok(),
                "{action} at project:{name}"
            );
        }
    }
    // Every project Action the owner's grant lists is one it may still take at its projects: none was listed only at the root.
    assert!(actions
        .iter()
        .filter(|a| !PROJECT_AUTHENTICATED_ACTIONS.contains(a))
        .all(|a| *a == "control.apply-proposal"));
}
