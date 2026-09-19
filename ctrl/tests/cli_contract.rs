use central_ctrl::{initialize_central, run_cli, CliEnvironment, ResultStatus};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-cli-contract-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn environment(root: &PathBuf) -> CliEnvironment {
    CliEnvironment {
        configured_root: Some(root.clone()),
        home: None,
    }
}

#[test]
fn generic_action_run_invokes_registered_actions_with_structured_input() {
    let root = temporary_directory("generic").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work/example-project")).unwrap();

    let list = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(list.result.status, ResultStatus::Success);
    assert_eq!(list.result.action.as_deref(), Some("action.list"));

    let search = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "work.search".to_owned(),
            r#"{"query":"example"}"#.to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(search.result.status, ResultStatus::Success);
    assert_eq!(search.result.action.as_deref(), Some("work.search"));
    assert_eq!(
        search.result.data.as_ref().unwrap()["matches"][0]["name"],
        "example-project"
    );
}

#[test]
fn work_reveal_has_a_stock_cli_projection_even_when_no_provider_is_available() {
    let root = temporary_directory("reveal").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir_all(root.join("Work/example-project")).unwrap();

    let result = run_cli(
        &[
            "--json".to_owned(),
            "work".to_owned(),
            "reveal".to_owned(),
            "example-project".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(result.result.action.as_deref(), Some("work.reveal"));
    assert_eq!(result.result.status, ResultStatus::UnavailableCapability);
}

#[test]
fn generic_action_run_rejects_non_object_or_extra_input() {
    let root = temporary_directory("invalid").join("Central");
    initialize_central(&root).unwrap();

    let scalar = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
            "[]".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(scalar.result.status, ResultStatus::InvalidInput);
    assert!(scalar.output.contains("must be a JSON object"));

    let extra = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "action.list".to_owned(),
            "{}".to_owned(),
            "extra".to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(extra.result.status, ResultStatus::InvalidInput);
    assert!(extra.output.contains("at most one JSON object"));
}

#[test]
fn git_census_reports_worktrees_branches_and_attention_for_a_fixture_repo() {
    use std::process::Command;

    let root = temporary_directory("git-census").join("Central");
    initialize_central(&root).unwrap();
    let repo = root.join("Work/example");
    fs::create_dir_all(&repo).unwrap();

    let git = |args: &[&str], cwd: &std::path::Path| {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--initial-branch=main"], &repo);
    git(&["config", "user.email", "contract@example.invalid"], &repo);
    git(&["config", "user.name", "Contract"], &repo);
    fs::write(repo.join("seed.txt"), "seed\n").unwrap();
    git(&["add", "-A"], &repo);
    git(&["commit", "-m", "seed"], &repo);
    git(
        &[
            "worktree",
            "add",
            "-b",
            "lane/unpushed",
            "../../example-lane",
        ],
        &repo,
    );
    fs::write(root.join("example-lane").join("lane.txt"), "lane\n").unwrap();
    git(&["add", "-A"], &root.join("example-lane"));
    git(&["commit", "-m", "lane work"], &root.join("example-lane"));
    fs::write(repo.join("dirty.txt"), "uncommitted\n").unwrap();

    let json = run_cli(
        &[
            "--json".to_owned(),
            "action".to_owned(),
            "run".to_owned(),
            "central.git.census".to_owned(),
            r#"{"project":"example"}"#.to_owned(),
        ],
        &environment(&root),
    );
    assert_eq!(json.result.status, ResultStatus::Success);
    let repos = json.result.data.as_ref().unwrap()["repos"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(repos.len(), 1);
    let repo_doc = &repos[0];
    assert_eq!(repo_doc["repo"], "Work/example");
    assert_eq!(repo_doc["head_branch"], "main");
    assert_eq!(repo_doc["worktrees"].as_array().unwrap().len(), 2);
    // The fixture has no remote at all, so every branch tip — main included —
    // is reachable from no remote ref: both are reported local-only.
    assert_eq!(repo_doc["unmerged_tips"].as_array().unwrap().len(), 2);
    // The lane lives outside .aikit/tasks and no return claims it yet.
    let lanes: Vec<&str> = repo_doc["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["lane"].as_str())
        .collect();
    assert!(lanes.contains(&"unattributed"));

    let attention: Vec<String> = json.result.data.as_ref().unwrap()["attention"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["kind"].as_str().unwrap().to_owned())
        .collect();
    assert!(attention.contains(&"local_only_branch".to_owned()));
    assert!(attention.contains(&"unattributed_worktree".to_owned()));
    assert_eq!(
        attention
            .iter()
            .filter(|kind| *kind == "local_only_branch")
            .count(),
        2
    );

    let tree = run_cli(
        &["git".to_owned(), "tree".to_owned(), "example".to_owned()],
        &environment(&root),
    );
    assert_eq!(tree.result.status, ResultStatus::Success);
    assert!(tree.output.contains("wt example @ main"));
    // The lane is checked out, so it renders as a worktree; the tree's
    // branch section lists parked (not checked out) branches only.
    assert!(tree.output.contains("wt example-lane @ lane/unpushed"));

    let graph = run_cli(&["git".to_owned(), "graph".to_owned()], &environment(&root));
    assert_eq!(graph.result.status, ResultStatus::Success);
    assert!(graph.output.contains("flowchart LR"));
    assert!(graph.output.contains("LOCAL-ONLY"));

    let bad = run_cli(&["git".to_owned(), "shove".to_owned()], &environment(&root));
    assert_eq!(bad.result.status, ResultStatus::InvalidInput);
}
