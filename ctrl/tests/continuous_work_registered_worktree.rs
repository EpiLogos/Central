use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct World(PathBuf);

impl World {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "central-registered-worktree-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        central_ctrl::initialize_central(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn repository(&self, name: &str) -> PathBuf {
        let repository = self.root().join("Work").join(name);
        fs::create_dir_all(&repository).unwrap();
        git(&repository, &["init"]);
        git(
            &repository,
            &["config", "user.email", "central-test@example.invalid"],
        );
        git(&repository, &["config", "user.name", "Central test"]);
        fs::write(repository.join("tracked.txt"), "tracked\n").unwrap();
        git(&repository, &["add", "tracked.txt"]);
        git(&repository, &["commit", "-m", "fixture"]);
        repository
    }

    fn worktree(&self, repository: &Path, name: &str) -> PathBuf {
        let checkout = self.root().join("worktrees/env-2").join(name);
        fs::create_dir_all(checkout.parent().unwrap()).unwrap();
        git(
            repository,
            &["worktree", "add", "--detach", checkout.to_str().unwrap()],
        );
        checkout
    }

    fn policy(&self, writable: Value) {
        let path = "Control/user/placement.json";
        fs::write(
            self.root().join(path),
            serde_json::to_vec_pretty(&json!({
                "schema":"central.work-placement-policy/v1",
                "scope_ref":"control:root",
                "authority_refs":[],
                "writable":writable,
                "protected":[],
                "enforcement":"native-actions",
                "required_coverage":["file-content","file-creation"],
                "lease_seconds":300
            }))
            .unwrap(),
        )
        .unwrap();
        let scope =
            central_ctrl::continuous_work::source::Scope::resolve(self.root(), None).unwrap();
        fs::create_dir_all(self.root().join("Control/relations")).unwrap();
        fs::write(
            self.root().join("Control/relations/source-relations.json"),
            serde_json::to_vec_pretty(&json!({
                "schema":scope.relations_schema,
                "project_id":scope.relations_id,
                "relations":[{
                    "ref":scope.source_ref(path),
                    "path":path,
                    "roles":["work-placement-policy"],
                    "provenance":"human-adopted",
                    "standing":"architecture-contract",
                    "treatment":"projectcentral-user",
                    "recognition":"explicit-controlled-test-fixture-not-personal-adoption",
                    "recorded_at_unix_seconds":1
                }]
            }))
            .unwrap(),
        )
        .unwrap();
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
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(repository: &Path, arguments: &[&str]) -> Output {
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
    output
}

#[test]
fn native_cli_admits_only_the_registered_worktree_of_an_authorised_work_repository() {
    let world = World::new();
    let repository = world.repository("owned");
    let checkout = world.worktree(&repository, "owned");
    world.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"worktrees/env-2/owned","class":"worktree"}
    ]));

    let policy = world.invoke("central.work.policy", json!({}));
    assert_eq!(policy["ok"], true, "{policy}");
    let admitted = policy["data"]["writable_destinations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|grant| grant["class"] == "worktree")
        .expect("registered checkout is present in the effective policy");
    let admitted_path = PathBuf::from(admitted["path"].as_str().unwrap());
    assert_eq!(
        fs::canonicalize(&admitted_path).unwrap(),
        fs::canonicalize(&checkout).unwrap()
    );
    let protected = policy["data"]["protected_paths"].as_array().unwrap();
    for expected in [
        admitted_path.join(".git"),
        admitted_path.join(".central"),
        admitted_path.join("ProjectCentral"),
        fs::canonicalize(repository.join(".git")).unwrap(),
    ] {
        assert!(
            protected
                .iter()
                .any(|path| path == expected.to_str().unwrap()),
            "effective policy omitted protection for {}: {policy}",
            expected.display()
        );
    }

    let allocation = world.invoke(
        "central.now.allocate",
        json!({
            "task_ref":"task:registered-worktree",
            "purpose":"write through the explicitly registered lane",
            "expected_policy_revision":policy["data"]["revision"]
        }),
    );
    assert_eq!(allocation["ok"], true, "{allocation}");
    let validate = |destination: &str| {
        world.invoke(
            "central.work.validate",
            json!({
                "destination":destination,
                "now_ref":allocation["data"]["now_ref"],
                "expected_now_revision":allocation["data"]["revision"]["revision"],
                "expected_policy_revision":policy["data"]["revision"]
            }),
        )
    };
    let ordinary = validate("worktrees/env-2/owned/src/new.rs");
    assert_eq!(ordinary["ok"], true, "{ordinary}");
    let metadata = validate("worktrees/env-2/owned/.git");
    assert_eq!(metadata["ok"], false, "{metadata}");
    let protected_ground = validate("worktrees/env-2/owned/ProjectCentral/user/position.md");
    assert_eq!(protected_ground["ok"], false, "{protected_ground}");
    let private_ground = validate("worktrees/env-2/owned/.central/owner-state");
    assert_eq!(private_ground["ok"], false, "{private_ground}");
    let sibling = validate("worktrees/env-2/sibling/src/new.rs");
    assert_eq!(sibling["ok"], false, "{sibling}");
}

#[test]
fn native_cli_refuses_unregistered_and_foreign_repository_worktree_grants() {
    let unregistered = World::new();
    unregistered.repository("owned");
    fs::create_dir_all(unregistered.root().join("worktrees/env-2/not-a-worktree")).unwrap();
    unregistered.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"worktrees/env-2/not-a-worktree","class":"worktree"}
    ]));
    let refusal = unregistered.invoke("central.work.policy", json!({}));
    assert_eq!(refusal["ok"], false, "{refusal}");
    assert!(refusal
        .to_string()
        .contains("registered linked Git checkout"));

    let foreign = World::new();
    foreign.repository("owned");
    let foreign_repository = foreign.repository("foreign");
    foreign.worktree(&foreign_repository, "foreign");
    foreign.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"worktrees/env-2/foreign","class":"worktree"}
    ]));
    let refusal = foreign.invoke("central.work.policy", json!({}));
    assert_eq!(refusal["ok"], false, "{refusal}");
    assert!(refusal
        .to_string()
        .contains("not an authorised Work repository"));
}

#[cfg(unix)]
#[test]
fn native_cli_refuses_a_git_marker_symlink_to_a_registered_sibling() {
    use std::os::unix::fs::symlink;

    let world = World::new();
    let repository = world.repository("owned");
    let registered = world.worktree(&repository, "registered");
    let alias = world.root().join("worktrees/env-2/alias");
    fs::create_dir_all(&alias).unwrap();
    symlink(registered.join(".git"), alias.join(".git")).unwrap();
    world.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"worktrees/env-2/alias","class":"worktree"}
    ]));

    let refusal = world.invoke("central.work.policy", json!({}));
    assert_eq!(refusal["ok"], false, "{refusal}");
    assert!(refusal
        .to_string()
        .contains("invalid native Git registration"));
}

#[cfg(unix)]
#[test]
fn native_cli_refuses_a_symlink_component_cancelled_by_parent_traversal() {
    use std::os::unix::fs::symlink;

    let world = World::new();
    let repository = world.repository("owned");
    let registered = world.worktree(&repository, "registered");
    let ordinary_marker = fs::read_to_string(registered.join(".git")).unwrap();
    let admin = PathBuf::from(ordinary_marker.trim().strip_prefix("gitdir: ").unwrap());
    let admin_name = admin.file_name().unwrap().to_str().unwrap();
    symlink("objects", repository.join(".git/cancelled-link")).unwrap();
    fs::write(
        registered.join(".git"),
        format!(
            "gitdir: {}/.git/cancelled-link/../worktrees/{admin_name}\n",
            repository.display()
        ),
    )
    .unwrap();
    world.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"worktrees/env-2/registered","class":"worktree"}
    ]));

    let refusal = world.invoke("central.work.policy", json!({}));
    assert_eq!(refusal["ok"], false, "{refusal}");
    assert!(refusal
        .to_string()
        .contains("registration contains a symlink"));
}

#[test]
fn native_cli_does_not_turn_worktree_class_into_an_arbitrary_root_exception() {
    let world = World::new();
    let repository = world.repository("owned");
    let checkout = world.root().join("scratch/owned");
    fs::create_dir_all(checkout.parent().unwrap()).unwrap();
    git(
        &repository,
        &["worktree", "add", "--detach", checkout.to_str().unwrap()],
    );
    world.policy(json!([
        {"path":"Work/owned","class":"repository"},
        {"path":"scratch/owned","class":"worktree"}
    ]));
    let refusal = world.invoke("central.work.policy", json!({}));
    assert_eq!(refusal["ok"], false, "{refusal}");
    assert!(refusal
        .to_string()
        .contains("root engineering grant must name a Work member"));
}
