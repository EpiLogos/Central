//! The floor's conformance (O-I #1005): every world here is a disposable Central root with its own governance, relations and
//! credentials; none of it touches a real ground or a real floor.
use super::*;
use sha2::{Digest, Sha256};

const HUMAN: &str = "floor-conformance-human-token-not-a-live-credential";
const AGENT: &str = "floor-conformance-agent-token-not-a-live-credential";

/// The person's own floor as another renderer leaves it: a header naming that renderer, one statement named by its path, one by
/// its topic as a heading.
const FOREIGN: &str = "# Central — session ground (derived; do not edit)\n\n<!-- derived, generated-derived: edit the sources, then re-render\nrenderer: central-session-strap/render-context.py@2; 39 sources, revision central.content-fnv1a64/v1:4125:e0849a21cf0785b5\n-->\n\n## Plain words\n\nSay it plainly.\n\nFrom `Control/agents/governance/world/shape.md`.\n";

struct Ground {
    _temp: crate::TempDir,
    root: std::path::PathBuf,
}

/// A root with three statements: two adopted (durable), one a draft, and one written but never related; and the person's grant
/// for `control.law.adopt`, a human's and an agent's.
fn ground() -> Ground {
    let temp = crate::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for dir in [
        "Control/user",
        "Control/relations",
        "Control/agents/governance/world",
        "Control/agents/governance/craft",
        "Work",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let statements = [
        (
            "craft/plain.md",
            "# Plain words\n\nSay it plainly.\n",
            Some("durable-source"),
        ),
        (
            "world/shape.md",
            "# Shape of the world\n\nOne tree, one ground.\n",
            Some("durable-source"),
        ),
        (
            "craft/draft.md",
            "# A draft\n\nNot law yet.\n",
            Some("draft-source"),
        ),
        (
            "craft/unrelated.md",
            "# Written, not adopted\n\nWaits for the person.\n",
            None,
        ),
    ];
    let scope = source::Scope::resolve(&root, None).unwrap();
    let mut relations = Vec::new();
    for (file, text, standing) in statements {
        let path = format!("{GOVERNANCE_DIR}/{file}");
        fs::write(root.join(&path), text).unwrap();
        if let Some(standing) = standing {
            relations.push(json!({"ref": scope.source_ref(&path), "path": path, "roles": [GOVERNANCE_ROLE],
                "provenance": "human-authored", "standing": standing, "treatment": "retain-native-in-place",
                "recognition": "controlled-test-only", "recorded_at_unix_seconds": 1}));
        }
    }
    let grant = |token: &str, principal: &str, kind: &str| {
        json!({"principal_ref": principal, "actor_kind": kind, "token_sha256": format!("{:x}", Sha256::digest(token.as_bytes())),
            "scope_refs": ["control:root"], "actions": [LAW_ADOPT], "expires_at_unix_seconds": 4_000_000_000u64})
    };
    let authority = "Control/user/authority.json";
    fs::write(
        root.join(authority),
        serde_json::to_vec(
            &json!({"schema": "central.native-action-authority/v1", "scope_ref": "control:root",
            "grants": [grant(HUMAN, "human:test", "human"), grant(AGENT, "agent:test", "agent")]}),
        )
        .unwrap(),
    )
    .unwrap();
    relations.push(json!({"ref": scope.source_ref(authority), "path": authority, "roles": ["native-action-authority"],
        "provenance": "human-adopted", "standing": "architecture-contract", "treatment": "control-user",
        "recognition": "controlled-test-only", "recorded_at_unix_seconds": 1}));
    fs::write(
        root.join("Control/relations/source-relations.json"),
        serde_json::to_vec(
            &json!({"schema": "central.control.ground-relations/v1", "project_id": "control:root",
            "relations": relations}),
        )
        .unwrap(),
    )
    .unwrap();
    Ground { _temp: temp, root }
}

fn floor(ground: &Ground) -> String {
    fs::read_to_string(ground.root.join(FLOOR_FILE)).unwrap()
}

fn files(names: &[&str]) -> Value {
    json!({"files": names})
}

#[test]
fn render_carries_each_adopted_statement_under_its_topic_and_links_the_harnesses() {
    let ground = ground();
    let receipt = render(&ground.root, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap();
    assert_eq!(receipt["schema"], FLOOR_RENDER_SCHEMA);
    assert_eq!(receipt["changed"], true);
    let text = floor(&ground);
    // Its standing and its renderer, in its header comment.
    assert!(text.contains("generated-derived"), "{text}");
    assert!(
        text.contains(&format!("renderer: {FLOOR_RENDERER}")),
        "{text}"
    );
    // Each durable statement under its topic as a heading, with its source path and its words.
    for (topic, path, words) in [
        (
            "Plain words",
            "Control/agents/governance/craft/plain.md",
            "Say it plainly.",
        ),
        (
            "Shape of the world",
            "Control/agents/governance/world/shape.md",
            "One tree, one ground.",
        ),
    ] {
        let at = text
            .find(&format!("## {topic}\n"))
            .unwrap_or_else(|| panic!("no heading {topic}: {text}"));
        assert!(text[at..].contains(&format!("Source: `{path}`")), "{text}");
        assert!(text[at..].contains(words), "{text}");
    }
    // A draft and a statement never adopted are not law.
    assert!(
        !text.contains("Not law yet") && !text.contains("Waits for the person"),
        "{text}"
    );
    // The harnesses' files are links to the floor.
    for name in ["CLAUDE.md", "GEMINI.md"] {
        assert_eq!(
            fs::read_link(ground.root.join(name)).unwrap(),
            Path::new(FLOOR_FILE)
        );
    }
    // The same law renders the same bytes, and nothing is written again.
    let again = render(&ground.root, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(again["revision"], receipt["revision"]);
    assert_eq!(floor(&ground), text);
    assert_eq!(again["links"][0]["state"], "kept");
}

#[test]
fn read_says_what_each_floor_carries() {
    let ground = ground();
    render(&ground.root, &files(&["CLAUDE.md"])).unwrap();
    let reading = read(
        &ground.root,
        &files(&["CLAUDE.md", "AGENTS.md", "GEMINI.md"]),
    )
    .unwrap();
    assert_eq!(reading["schema"], FLOOR_READING_SCHEMA);
    assert_eq!(reading["root"], ground.root.to_string_lossy().as_ref());
    let floors = reading["floors"].as_array().unwrap();
    for floor in &floors[..2] {
        assert_eq!(floor["state"], "rendered", "{floor}");
        assert_eq!(floor["renderer"], FLOOR_RENDERER);
        let carried: Vec<&str> = floor["statements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|statement| statement["file"].as_str().unwrap())
            .collect();
        assert_eq!(carried, ["craft/plain.md", "world/shape.md"], "{floor}");
        assert_eq!(floor["statements"][0]["topic"], "Plain words");
        assert_eq!(floor["statements"][0]["standing"], "durable");
    }
    assert_eq!(floors[2]["file"], "GEMINI.md");
    assert_eq!(floors[2]["state"], "absent");
    assert_eq!(floors[2]["statements"], json!([]));
    // A reading takes the names it reads.
    assert_eq!(
        read(&ground.root, &json!({})).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        read(&ground.root, &files(&["../AGENTS.md"]))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn a_floor_another_hand_rendered_is_read_and_never_overwritten() {
    let ground = ground();
    fs::write(ground.root.join(FLOOR_FILE), FOREIGN).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(FLOOR_FILE, ground.root.join("CLAUDE.md")).unwrap();
    let refusal = render(&ground.root, &files(&["CLAUDE.md"])).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    let words = refusal.to_string();
    assert!(words.contains("was not rendered by Central"), "{words}");
    assert!(
        words.contains("central-session-strap/render-context.py@2"),
        "{words}"
    );
    assert!(words.contains("Nothing was written"), "{words}");
    assert_eq!(floor(&ground), FOREIGN);
    let reading = read(&ground.root, &files(&["CLAUDE.md"])).unwrap();
    let foreign = &reading["floors"][0];
    assert_eq!(foreign["state"], "foreign", "{foreign}");
    assert_eq!(
        foreign["renderer"],
        "central-session-strap/render-context.py@2"
    );
    // Named by its path, and by its topic as a heading.
    let named: Vec<&str> = foreign["statements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|statement| statement["file"].as_str().unwrap())
        .collect();
    assert_eq!(named, ["craft/plain.md", "world/shape.md"], "{foreign}");
    assert!(foreign["why"].as_str().unwrap().contains("not by Central"));
}

#[test]
fn an_instruction_file_of_its_own_is_left_as_it_is() {
    let ground = ground();
    fs::write(ground.root.join("GEMINI.md"), "the person's own words\n").unwrap();
    let refusal = render(&ground.root, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    assert!(refusal.to_string().contains("GEMINI.md"), "{refusal}");
    // Nothing was written: no floor, no link.
    assert!(!ground.root.join(FLOOR_FILE).exists());
    assert!(fs::symlink_metadata(ground.root.join("CLAUDE.md")).is_err());
    assert_eq!(
        fs::read_to_string(ground.root.join("GEMINI.md")).unwrap(),
        "the person's own words\n"
    );
}

#[test]
fn past_its_budget_the_floor_is_refused_naming_what_it_could_not_carry() {
    let ground = ground();
    render(&ground.root, &json!({})).unwrap();
    let before = floor(&ground);
    // A long adopted statement: the floor would pass its budget.
    let long = format!(
        "# The long one\n\n{}\n",
        "Every word of this statement counts. ".repeat(140)
    );
    fs::write(
        ground.root.join("Control/agents/governance/world/shape.md"),
        long,
    )
    .unwrap();
    let refusal = render(&ground.root, &json!({})).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    let words = refusal.to_string();
    assert!(
        words.contains(&format!("budget of {FLOOR_BUDGET_BYTES}")),
        "{words}"
    );
    assert!(
        words.contains("The long one (Control/agents/governance/world/shape.md)"),
        "{words}"
    );
    assert!(!words.contains("Plain words ("), "{words}");
    assert_eq!(floor(&ground), before, "nothing was written");
}

#[test]
fn adopting_a_statement_declares_it_durable_and_renders_the_floor_again() {
    let ground = ground();
    render(&ground.root, &files(&["CLAUDE.md"])).unwrap();
    let path = "Control/agents/governance/craft/unrelated.md";
    let revision =
        crate::source_safety::content_revision_bytes(&fs::read(ground.root.join(path)).unwrap());
    let input = json!({"statement": "craft/unrelated.md", "expected_revision": revision});
    // Not on an agent's credential, nor on no credential, nor on a revision the person did not read.
    assert_eq!(
        adopt_with_token_at(&ground.root, &input, Some(AGENT), 10)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        adopt_with_token_at(&ground.root, &input, None, 10)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    let stale = json!({"statement": "craft/unrelated.md", "expected_revision": "central.content-fnv1a64/v1:1:0"});
    assert_eq!(
        adopt_with_token_at(&ground.root, &stale, Some(HUMAN), 10)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(!floor(&ground).contains("Waits for the person"));
    let receipt = adopt_with_token_at(&ground.root, &input, Some(HUMAN), 10).unwrap();
    assert_eq!(receipt["schema"], LAW_ADOPTION_SCHEMA);
    assert_eq!(receipt["adopted"], true);
    assert_eq!(receipt["adopted_by"], "human:test");
    assert_eq!(receipt["floor"]["changed"], true, "{receipt}");
    // The floor every harness reads carries it now, under its topic.
    let text = floor(&ground);
    assert!(
        text.contains(
            "## Written, not adopted\n\nSource: `Control/agents/governance/craft/unrelated.md`"
        ),
        "{text}"
    );
    let reading = read(&ground.root, &files(&["CLAUDE.md"])).unwrap();
    let carried: Vec<&str> = reading["floors"][0]["statements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|statement| statement["file"].as_str().unwrap())
        .collect();
    assert!(carried.contains(&"craft/unrelated.md"), "{reading}");
    // Its relation is durable, the person's own.
    let index = crate::control::index_governance(&ground.root).unwrap();
    let entry = index
        .statements
        .iter()
        .find(|entry| entry.file == "craft/unrelated.md")
        .unwrap();
    assert_eq!(entry.standing, "durable");
    // A draft adopted keeps its relation's identity: its standing changes in place.
    let draft = "Control/agents/governance/craft/draft.md";
    let revision =
        crate::source_safety::content_revision_bytes(&fs::read(ground.root.join(draft)).unwrap());
    adopt_with_token_at(
        &ground.root,
        &json!({"statement": draft, "expected_revision": revision}),
        Some(HUMAN),
        11,
    )
    .unwrap();
    let relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    let held: Vec<&Value> = relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == draft)
        .collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["standing"], "durable-source");
    // A path outside the governance folder is no statement.
    for outside in [
        "../user/authority.json",
        "/etc/passwd",
        "craft/README.md",
        "craft/plain.txt",
    ] {
        let refusal = adopt_with_token_at(
            &ground.root,
            &json!({"statement": outside, "expected_revision": "x"}),
            Some(HUMAN),
            12,
        );
        assert_eq!(
            refusal.unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "{outside}"
        );
    }
}

#[test]
fn adopting_on_a_foreign_floor_keeps_the_adoption_and_says_why_the_floor_stands() {
    let ground = ground();
    fs::write(ground.root.join(FLOOR_FILE), FOREIGN).unwrap();
    let path = "Control/agents/governance/craft/unrelated.md";
    let revision =
        crate::source_safety::content_revision_bytes(&fs::read(ground.root.join(path)).unwrap());
    let receipt = adopt_with_token_at(
        &ground.root,
        &json!({"statement": path, "expected_revision": revision}),
        Some(HUMAN),
        10,
    )
    .unwrap();
    assert_eq!(receipt["adopted"], true);
    assert!(
        receipt["floor"]["refused"]
            .as_str()
            .unwrap()
            .contains("was not rendered by Central"),
        "{receipt}"
    );
    assert_eq!(floor(&ground), FOREIGN);
}

#[test]
fn where_there_is_no_central_ground_the_reading_says_so_in_words() {
    let ground = ground();
    let missing = ground.root.join("no-such-ground");
    let refusal = read(&missing, &files(&["AGENTS.md"])).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::NotFound);
    assert!(
        refusal.to_string().starts_with("no Central ground at "),
        "{refusal}"
    );
    assert!(!refusal.to_string().contains("os error"), "{refusal}");
    fs::create_dir_all(&missing).unwrap();
    let refusal = render(&missing, &json!({})).unwrap_err();
    assert!(
        refusal
            .to_string()
            .contains("is no Central ground (it keeps no Control and Work)"),
        "{refusal}"
    );
    assert!(
        fs::read_dir(&missing).unwrap().next().is_none(),
        "nothing was written"
    );
}

#[test]
fn the_floor_actions_stand_in_centrals_action_field() {
    let registry = crate::cli::create_runtime_action_registry();
    // Rendering writes AGENTS.md and its links at the root, never under `.central` alone: a write, though what it writes is derived.
    for (id, class) in [
        (RENDER, "locally-mutating"),
        (READ, "read-only"),
        (LAW_ADOPT, "locally-mutating"),
    ] {
        let descriptor = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not registered"));
        assert_eq!(descriptor.mutation_class.as_str(), class, "{id}");
    }
    assert!(authority::AUTHENTICATED_ACTIONS.contains(&LAW_ADOPT));
    assert!(!authority::PROJECT_AUTHENTICATED_ACTIONS.contains(&LAW_ADOPT));
}
