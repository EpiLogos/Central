//! Folders of work that stand outside the ground, and bringing one into Work (O-I #738): `work.candidates` finds them under
//! the person's home, read only; `work.bring.preview` lists the copy; `work.bring` makes it on the preview's digest, the
//! original kept.
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ActionResult,
    ConnectorContext, ConnectorRegistry, FilesystemWorkConnector, MutationClass, ResultStatus,
    RootOptions,
};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-work-bring-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}

/// A person's home with its Central ground at `Central`, as a world's Setup makes it.
fn home_with_ground(label: &str) -> (PathBuf, PathBuf) {
    let home = scratch(label);
    let root = home.join("Central");
    initialize_central(&root).unwrap();
    (home, root)
}

fn execute(home: Option<&Path>, root: &Path, action: &str, input: Value) -> ActionResult {
    let registry = create_core_action_registry();
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FilesystemWorkConnector::new()).unwrap();
    let connector_context = ConnectorContext {
        platform: "test".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.to_path_buf()),
        configured_root: None,
        home: home.map(Path::to_path_buf),
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    registry.execute(action, &input, &context)
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", repository)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// A small repository with its own agent instructions, an executable script, a read-only file and an inner link.
fn repository(path: &Path) {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "mira@example.invalid"]);
    git(path, &["config", "user.name", "Mira"]);
    write(&path.join("AGENTS.md"), "# Working here\n");
    write(&path.join("log.py"), "print('log')\n");
    write(&path.join("bin/run"), "#!/bin/sh\necho run\n");
    fs::set_permissions(path.join("bin/run"), fs::Permissions::from_mode(0o755)).unwrap();
    write(&path.join("docs/fixed.txt"), "kept as it is\n");
    fs::set_permissions(
        path.join("docs/fixed.txt"),
        fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    symlink("../log.py", path.join("docs/log-link.py")).unwrap();
    git(path, &["add", "-A"]);
    git(
        path,
        &["commit", "-q", "-m", "Field notes: a listening log"],
    );
    write(&path.join("draft.md"), "not committed yet\n");
}

fn okf_wiki(path: &Path) {
    write(
        path,
        &json!({"objects":[{"object":"space","profile":"okf-wiki/v1","ref":"okf-wiki/spaces:notes"}]}).to_string(),
    );
}

/// Every entry of a tree, relative, with its kind, bytes, mode and (for links) target: what a faithful copy must match.
fn tree(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            let meta = fs::symlink_metadata(&path).unwrap();
            let mode = meta.permissions().mode() & 0o7777;
            if meta.file_type().is_symlink() {
                out.push(format!(
                    "link {relative} -> {}",
                    fs::read_link(&path).unwrap().display()
                ));
            } else if meta.is_dir() {
                out.push(format!("dir {relative} {mode:o}"));
                walk(root, &path, out);
            } else {
                out.push(format!(
                    "file {relative} {mode:o} {:?}",
                    fs::read(&path).unwrap()
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

fn relatives(result: &ActionResult) -> Vec<(String, Vec<String>)> {
    result.data.as_ref().unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["relative"].as_str().unwrap().to_owned(),
                item["signals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|signal| signal.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}

fn message(result: &ActionResult) -> String {
    result
        .error
        .as_ref()
        .map(|error| error.message.clone())
        .unwrap_or_default()
}

#[test]
fn the_catalogue_classes_candidates_and_preview_as_reads_and_bring_as_the_persons_previewed_write()
{
    let registry = create_core_action_registry();
    let candidates = registry
        .get("work.candidates")
        .expect("work.candidates is registered");
    assert_eq!(candidates.mutation_class, MutationClass::ReadOnly);
    assert!(candidates.inputs.iter().all(|input| !input.required));
    let preview = registry
        .get("work.bring.preview")
        .expect("work.bring.preview is registered");
    assert_eq!(preview.mutation_class, MutationClass::ReadOnly);
    let bring = registry
        .get("work.bring")
        .expect("work.bring is registered");
    assert_eq!(bring.mutation_class, MutationClass::LocallyMutating);
    assert!(
        bring.preview_supported,
        "Accept makes what the preview listed"
    );
    for (descriptor, required) in [
        (preview, vec!["source_path"]),
        (bring, vec!["expected_digest", "source_path"]),
    ] {
        let mut named: Vec<&str> = descriptor
            .inputs
            .iter()
            .filter(|input| input.required)
            .map(|input| input.name.as_str())
            .collect();
        named.sort();
        assert_eq!(named, required, "{}", descriptor.id);
        assert!(descriptor
            .inputs
            .iter()
            .any(|input| input.name == "name" && !input.required));
    }
}

#[test]
fn candidates_are_the_folders_of_work_under_home_outside_the_ground_and_the_products_homes() {
    let (home, root) = home_with_ground("candidates");
    repository(&home.join("code/field-notes"));
    // A repository inside a candidate is part of it: the scan stops at the first folder of work on each path.
    fs::create_dir_all(home.join("code/field-notes/vendor/inner/.git")).unwrap();
    fs::create_dir_all(home.join("code/tide-sketches/.git")).unwrap();
    write(&home.join("code/tide-sketches/AGENTS.md"), "# Sketchbook\n");
    write(&home.join("Vault/.obsidian/app.json"), "{}\n");
    write(&home.join("Vault/Home.md"), "# Home\n");
    write(&home.join("helpers/CLAUDE.md"), "@AGENTS.md\n");
    okf_wiki(&home.join("essays/wiki.json"));
    // A wiki.json that is not an OKF wiki is no signal; plain notes are not a folder of work.
    write(&home.join("settings/wiki.json"), "{\"theme\":\"dark\"}\n");
    write(&home.join("Notes/Ideas.md"), "# Ideas\n");
    // Four levels down is still found; five is beyond the bound.
    fs::create_dir_all(home.join("a/b/c/four/.git")).unwrap();
    fs::create_dir_all(home.join("a/b/c/d/five/.git")).unwrap();
    // Never entered: the ground, the products' homes, hidden folders, Library and dependency trees.
    fs::create_dir_all(root.join("Work/inside/.git")).unwrap();
    for hidden in [
        ".aikit/x/.git",
        ".oi/x/.git",
        ".config/x/.git",
        ".central/x/.git",
        ".claude/skills/s/.git",
        "Library/x/.git",
        "web/node_modules/pkg/.git",
    ] {
        fs::create_dir_all(home.join(hidden)).unwrap();
    }
    // A link is never followed: the repository it leads to is found once, where it stands.
    symlink(home.join("code/field-notes"), home.join("linked-notes")).unwrap();

    let before = tree(&home);
    let result = execute(Some(&home), &root, "work.candidates", json!({}));
    assert_eq!(result.status, ResultStatus::Success, "{}", message(&result));
    let data = result.data.as_ref().unwrap();
    assert_eq!(data["home"], json!(home));
    assert_eq!(data["ground"], json!(root));
    assert_eq!(data["truncated"], json!(false));
    assert_eq!(
        relatives(&result),
        vec![
            ("Vault".to_owned(), vec!["obsidian".to_owned()]),
            ("a/b/c/four".to_owned(), vec!["git".to_owned()]),
            (
                "code/field-notes".to_owned(),
                vec!["git".to_owned(), "agents".to_owned()]
            ),
            (
                "code/tide-sketches".to_owned(),
                vec!["git".to_owned(), "agents".to_owned()]
            ),
            ("essays".to_owned(), vec!["wiki".to_owned()]),
            ("helpers".to_owned(), vec!["claude".to_owned()]),
        ]
    );
    let first = &data["items"][2];
    assert_eq!(first["name"], "field-notes");
    assert_eq!(first["path"], json!(home.join("code/field-notes")));
    assert_eq!(tree(&home), before, "reading candidates changes nothing");
}

#[test]
fn candidates_need_a_home_and_find_nothing_when_the_home_lies_inside_the_ground() {
    let (home, root) = home_with_ground("no-home");
    let refused = execute(None, &root, "work.candidates", json!({}));
    assert_eq!(refused.status, ResultStatus::InvalidInput);
    assert!(message(&refused).contains("home"), "{}", message(&refused));

    fs::create_dir_all(root.join("Work/project/.git")).unwrap();
    let inside = execute(
        Some(&root.join("Work")),
        &root,
        "work.candidates",
        json!({}),
    );
    assert_eq!(inside.status, ResultStatus::Success, "{}", message(&inside));
    assert_eq!(relatives(&inside), vec![]);
    let _ = home;
}

#[test]
fn bring_copies_a_folder_into_work_with_its_history_modes_and_links_and_keeps_the_original() {
    let (home, root) = home_with_ground("bring");
    let source = home.join("code/field-notes");
    repository(&source);
    let original = tree(&source);
    let history = git(&source, &["log", "--format=%H %s"]);

    let preview = execute(
        Some(&home),
        &root,
        "work.bring.preview",
        json!({"source_path": source}),
    );
    assert_eq!(
        preview.status,
        ResultStatus::Success,
        "{}",
        message(&preview)
    );
    let plan = preview.data.clone().unwrap();
    assert_eq!(plan["name"], "field-notes");
    assert_eq!(plan["target_ref"], "Work/field-notes");
    assert_eq!(plan["target"], json!(root.join("Work/field-notes")));
    assert_eq!(plan["git"], json!(true));
    assert_eq!(plan["entries"]["links"], json!(1));
    let digest = plan["digest"].as_str().unwrap().to_owned();
    assert_eq!(digest.len(), 64);
    let operations = plan["operations"].as_array().unwrap();
    assert!(operations
        .iter()
        .any(|line| line.as_str().unwrap().contains("Work/field-notes")));
    assert!(operations
        .iter()
        .any(|line| line.as_str().unwrap().contains("original stays")));
    assert!(
        !root.join("Work/field-notes").exists(),
        "a preview makes nothing"
    );

    let wrong = execute(
        Some(&home),
        &root,
        "work.bring",
        json!({"source_path": source, "expected_digest": "0".repeat(64)}),
    );
    assert_eq!(
        wrong.status,
        ResultStatus::VerificationFailure,
        "{}",
        message(&wrong)
    );
    assert!(!root.join("Work/field-notes").exists());

    let brought = execute(
        Some(&home),
        &root,
        "work.bring",
        json!({"source_path": source, "expected_digest": digest}),
    );
    assert_eq!(
        brought.status,
        ResultStatus::Success,
        "{}",
        message(&brought)
    );
    let receipt = brought.data.clone().unwrap();
    assert_eq!(receipt["target_ref"], "Work/field-notes");
    assert_eq!(receipt["verified"], json!(true));
    let target = root.join("Work/field-notes");
    assert_eq!(
        tree(&target),
        original,
        "every file, mode and link as it was"
    );
    assert_eq!(
        git(&target, &["log", "--format=%H %s"]),
        history,
        "the history came with it"
    );
    assert_eq!(git(&target, &["status", "--porcelain"]), "?? draft.md\n");
    assert_eq!(tree(&source), original, "the original stays as it was");
    let leftovers: Vec<_> = fs::read_dir(root.join("Work"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        leftovers,
        vec!["field-notes".to_owned()],
        "nothing else stands in Work"
    );

    let listed = execute(Some(&home), &root, "work.list", json!({}));
    assert_eq!(
        listed.data.as_ref().unwrap()["items"][0]["name"],
        "field-notes"
    );

    let again = execute(
        Some(&home),
        &root,
        "work.bring.preview",
        json!({"source_path": source}),
    );
    assert_eq!(again.status, ResultStatus::InvalidInput);
    assert!(
        message(&again).contains("Work/field-notes is there already"),
        "{}",
        message(&again)
    );
    let named = execute(
        Some(&home),
        &root,
        "work.bring.preview",
        json!({"source_path": source, "name": "listening-log"}),
    );
    assert_eq!(named.status, ResultStatus::Success, "{}", message(&named));
    assert_eq!(
        named.data.as_ref().unwrap()["target_ref"],
        "Work/listening-log"
    );
}

#[test]
fn bring_refuses_a_folder_that_changed_since_its_preview_and_leaves_nothing_in_work() {
    let (home, root) = home_with_ground("changed");
    let source = home.join("Vault");
    write(&source.join(".obsidian/app.json"), "{}\n");
    write(&source.join("Home.md"), "# Home\n");
    let preview = execute(
        Some(&home),
        &root,
        "work.bring.preview",
        json!({"source_path": source}),
    );
    let digest = preview.data.as_ref().unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    write(&source.join("Estuary.md"), "# Estuary\n");
    let refused = execute(
        Some(&home),
        &root,
        "work.bring",
        json!({"source_path": source, "expected_digest": digest}),
    );
    assert_eq!(refused.status, ResultStatus::VerificationFailure);
    assert!(
        message(&refused).contains("changed since its preview"),
        "{}",
        message(&refused)
    );
    assert_eq!(fs::read_dir(root.join("Work")).unwrap().count(), 0);
}

#[test]
fn bring_refuses_what_would_lead_out_of_the_ground_or_into_it_twice() {
    let (home, root) = home_with_ground("refusals");
    let refuse = |input: Value, words: &str| {
        let result = execute(Some(&home), &root, "work.bring.preview", input.clone());
        assert_eq!(
            result.status,
            ResultStatus::InvalidInput,
            "{input}: {}",
            message(&result)
        );
        assert!(
            message(&result).contains(words),
            "{input}: {}",
            message(&result)
        );
    };
    write(&home.join("elsewhere/secret.txt"), "not this folder's\n");
    write(&home.join("escapes/notes.md"), "# Notes\n");
    symlink("../elsewhere/secret.txt", home.join("escapes/secret.txt")).unwrap();
    refuse(
        json!({"source_path": home.join("escapes")}),
        "leads out of the folder",
    );
    write(&home.join("absolute/notes.md"), "# Notes\n");
    symlink(
        home.join("absolute/notes.md"),
        home.join("absolute/again.md"),
    )
    .unwrap();
    refuse(
        json!({"source_path": home.join("absolute")}),
        "leads out of the folder",
    );
    symlink(home.join("elsewhere"), home.join("a-link")).unwrap();
    refuse(json!({"source_path": home.join("a-link")}), "is a link");
    refuse(
        json!({"source_path": home.join("missing")}),
        "Nothing stands at",
    );
    refuse(json!({"source_path": "code/field-notes"}), "full path");
    refuse(
        json!({"source_path": home.join("elsewhere/secret.txt")}),
        "not a folder",
    );
    fs::create_dir_all(root.join("Work/inside")).unwrap();
    refuse(
        json!({"source_path": root.join("Work/inside")}),
        "in your ground already",
    );
    refuse(json!({"source_path": home}), "holds your ground");
    // A git worktree's `.git` names another repository, and borrowed objects live in another store: the copy would share them.
    write(
        &home.join("lane/.git"),
        &format!(
            "gitdir: {}/.git/worktrees/lane\n",
            home.join("elsewhere").display()
        ),
    );
    write(&home.join("lane/notes.md"), "# Lane\n");
    refuse(
        json!({"source_path": home.join("lane")}),
        "shares another repository",
    );
    fs::create_dir_all(home.join("borrowed/.git/objects/info")).unwrap();
    write(
        &home.join("borrowed/.git/objects/info/alternates"),
        "/elsewhere/objects\n",
    );
    refuse(
        json!({"source_path": home.join("borrowed")}),
        "shares another repository",
    );
    // A submodule's pointer that stays inside the folder comes with it.
    write(
        &home.join("with-module/.git/HEAD"),
        "ref: refs/heads/main\n",
    );
    write(
        &home.join("with-module/module/.git"),
        "gitdir: ../.git/modules/module\n",
    );
    let inside = execute(
        Some(&home),
        &root,
        "work.bring.preview",
        json!({"source_path": home.join("with-module")}),
    );
    assert_eq!(inside.status, ResultStatus::Success, "{}", message(&inside));
    for name in [".hidden", "a/b", "..", ""] {
        refuse(
            json!({"source_path": home.join("elsewhere"), "name": name}),
            "is not a name",
        );
    }
    assert_eq!(
        fs::read_dir(root.join("Work")).unwrap().count(),
        1,
        "only Work/inside, made above"
    );
}
