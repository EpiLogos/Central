//! The floor's conformance (O-I #1005): every world here is a disposable Central root under a disposable home, with its own
//! governance, relations and credentials; none of it touches a real ground, a real floor or a real Codex.
use super::*;
use sha2::{Digest, Sha256};

const HUMAN: &str = "floor-conformance-human-token-not-a-live-credential";
const AGENT: &str = "floor-conformance-agent-token-not-a-live-credential";

/// A home holding a Central root at `~/Central`.
struct Ground {
    _temp: crate::TempDir,
    home: PathBuf,
    root: PathBuf,
}

/// A statement of the fixture: its file under the governance folder, its words, and its relation (standing and place) if it has
/// one.
type Written<'a> = (&'a str, &'a str, Option<(&'a str, Option<&'a str>)>);

/// A root with the given statements, and the person's grant for `control.law.adopt`, a human's and an agent's.
fn ground_with(statements: &[Written<'_>]) -> Ground {
    let temp = crate::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let root = home.join("Central");
    for dir in ["Control/user", "Control/relations", "Work"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let scope = source::Scope::resolve(&root, None).unwrap();
    let mut relations = Vec::new();
    for (file, text, relation) in statements {
        let path = format!("{GOVERNANCE_DIR}/{file}");
        fs::create_dir_all(root.join(&path).parent().unwrap()).unwrap();
        fs::write(root.join(&path), text).unwrap();
        if let Some((standing, floor)) = relation {
            let mut relation = json!({"ref": scope.source_ref(&path), "path": path, "roles": [GOVERNANCE_ROLE],
                "provenance": if *standing == "durable-source" { "human-authored" } else { "agent-proposed" },
                "standing": standing, "treatment": "retain-native-in-place",
                "recognition": "controlled-test-only", "recorded_at_unix_seconds": 1});
            if let Some(floor) = floor {
                relation["floor"] = json!(floor);
            }
            relations.push(relation);
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
    write_relations(&root, &relations);
    Ground {
        _temp: temp,
        home,
        root,
    }
}

fn write_relations(root: &Path, relations: &[Value]) {
    fs::write(
        root.join("Control/relations/source-relations.json"),
        serde_json::to_vec(
            &json!({"schema": "central.control.ground-relations/v1", "project_id": "control:root",
            "relations": relations}),
        )
        .unwrap(),
    )
    .unwrap();
}

/// A fresh World: one adopted statement and one written but never related, nothing the owner's own World keeps.
fn fresh() -> Ground {
    ground_with(&[
        (
            "craft/plain.md",
            "# Plain words\n\nSay it plainly.\n",
            Some(("durable-source", None)),
        ),
        (
            "craft/unrelated.md",
            "# Written, not adopted\n\nWaits for the person.\n",
            None,
        ),
    ])
}

/// A World shaped as the owner's: the statements the floor quotes (the world's shape, wrapped across lines; the authorship law,
/// never declared; responsibility), one statement on the floor in full, one draft on it, one door line, the engineering law with
/// its compile, and the session strap's own words.
fn owners() -> Ground {
    let ground = ground_with(&[
        (
            "world/shape-of-the-world.md",
            "# The shape of the world\n\nOne tree. Since then nothing creates another\nworktree (owner ruling, 29 September 2026, after repeated per-lane\nworktrees bloated the disk).\n\nThe ground's primary checkout for a project — `Work/<Name>` itself — stands on its `main` branch.\nWhen a lane lands, no checkout is left standing on the landed branch.\n",
            Some(("durable-source", None)),
        ),
        (
            "authorship-and-return/propose-not-write.md",
            "# Propose, not write\n\nAuthorship names whose intent originates a change, never whose hands\nperform it: on my stated intent you perform every operation yourself\n— including writes into this ground — and handing file mechanics back\nto me is unfinished work, not deference.\n",
            None,
        ),
        (
            "authorship-and-return/responsibility.md",
            "# Responsibility\n\nOwn the fault you make or discover; own means resolve honestly.\n",
            Some(("durable-source", None)),
        ),
        (
            "communicate-and-engage/smallest-sufficient.md",
            "---\nstanding: durable\n---\n# Smallest sufficient\n\nThe source can be large. The act should not be.\n\n",
            Some(("durable-source", Some("kernel"))),
        ),
        (
            "craft/draft.md",
            "# A draft\n\nNot law yet.\n",
            Some(("draft-source", Some("kernel"))),
        ),
        (
            "world/dlw-plan.md",
            "# DLW work\n\nDLW work: its plan is the Wayfinder map O-I #706.\nClaim a ticket there.\n",
            Some(("durable-source", Some("load"))),
        ),
        (
            "craft/plain.md",
            "# Plain words\n\nSay it plainly.\n",
            Some(("durable-source", None)),
        ),
        (
            "engineering/coding-approach.md",
            "# Coding approach\n\nThe smallest change that can work.\n",
            Some(("durable-source", None)),
        ),
        (
            "engineering/foundational-prompt.md",
            "# Foundational prompt\n\nFrom Control/agents/governance/engineering/coding-approach.md.\n",
            None,
        ),
        (
            "craft/unrelated.md",
            "# Written, not adopted\n\nWaits for the person.\n",
            None,
        ),
    ]);
    let strap = ground.root.join(STRAP);
    fs::create_dir_all(strap.parent().unwrap()).unwrap();
    fs::write(strap, "# Central session strap\n").unwrap();
    ground
}

fn floor(ground: &Ground) -> String {
    fs::read_to_string(ground.root.join(FLOOR_FILE)).unwrap()
}

fn files(names: &[&str]) -> Value {
    json!({"files": names})
}

fn render_in(ground: &Ground, input: &Value) -> io::Result<Value> {
    render(&ground.root, input, Some(&ground.home))
}

fn read_in(ground: &Ground, names: &[&str]) -> Value {
    read(&ground.root, &files(names), Some(&ground.home)).unwrap()
}

fn adopt(ground: &Ground, input: &Value, token: Option<&str>, at: u64) -> io::Result<Value> {
    adopt_with_token_at(&ground.root, input, token, at, Some(&ground.home))
}

/// The row a reading or a receipt gives one statement.
fn row<'a>(statements: &'a Value, file: &str) -> &'a Value {
    statements
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["file"] == file)
        .unwrap_or_else(|| panic!("no {file} in {statements}"))
}

fn revision_of(ground: &Ground, file: &str) -> String {
    crate::source_safety::content_revision_bytes(
        &fs::read(ground.root.join(GOVERNANCE_DIR).join(file)).unwrap(),
    )
}

#[test]
fn a_fresh_world_gets_a_floor_of_doors_and_links_for_its_harnesses() {
    let ground = fresh();
    let receipt = render_in(&ground, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap();
    assert_eq!(receipt["schema"], FLOOR_RENDER_SCHEMA);
    assert_eq!(receipt["changed"], true);
    let text = floor(&ground);
    // Its standing, its renderer and the revision of the law it answers to, in its header.
    assert!(
        text.starts_with("# Central — session ground (derived; do not edit)\n\n<!-- derived, generated-derived: "),
        "{text}"
    );
    let law_revision = receipt["law_revision"].as_str().unwrap();
    assert!(
        text.contains(&format!(
            "renderer: {FLOOR_RENDERER}; 1 statements, revision {law_revision}\n-->\n\n## The field you stand in\n"
        )),
        "{text}"
    );
    // The doors: the field under the root's name in the person's home, the front door, load by act, the card, the session's end.
    for line in [
        "You are in the Central personal world, `~/Central` (no repo at the root).",
        "## Front door: oi",
        "- Law: `ctrl control index` lists the statements (topic, standing); read the one your act touches.",
        "- Procedure: the `central-*` skills. `central-session-strap` orients a session and routes it to the practice it needs.",
        "- Before a field's work, load its methodology: `engineering-methodology` (code), `collaboration-methodology`, `authorship-methodology`, `world-keeping-methodology`, `docs-methodology`.",
        "## Never-do card",
        "- Carried is not loaded, and loaded is not used: the disclosure answers what",
        "Session end, every session: leave no loose folders at the Work root. Write no",
    ] {
        assert!(text.lines().any(|held| held == line), "no {line:?} in {text}");
    }
    // Nothing the owner's World alone holds: no seats, no quoted authorship law, no pointer to a file this World does not keep.
    for absent in [
        "env-1/2/3",
        "authorship law",
        "SKILL.md",
        "foundational-prompt.md",
    ] {
        assert!(!text.contains(absent), "{absent} in {text}");
    }
    // An adopted statement is carried by the door, never pasted in.
    assert!(!text.contains("Say it plainly"), "{text}");
    assert!(text.len() <= FLOOR_BUDGET_BYTES);
    // The harnesses' files are links to the floor; Codex is not kept at this home.
    for name in ["CLAUDE.md", "GEMINI.md"] {
        assert_eq!(
            fs::read_link(ground.root.join(name)).unwrap(),
            Path::new(FLOOR_FILE)
        );
    }
    assert_eq!(receipt["codex"]["file"], Value::Null);
    // The same law renders the same bytes, and nothing is written again.
    let again = render_in(&ground, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(again["revision"], receipt["revision"]);
    assert_eq!(floor(&ground), text);
    assert_eq!(again["links"][0]["state"], "kept");
    // Read: rendered by Central, current, each statement carried by the door with its standing.
    let reading = read_in(&ground, &["CLAUDE.md", "GEMINI.md", "AGENTS.md"]);
    assert_eq!(reading["schema"], FLOOR_READING_SCHEMA);
    for floor in reading["floors"].as_array().unwrap() {
        assert_eq!(floor["state"], "rendered", "{floor}");
        assert_eq!(floor["renderer"], FLOOR_RENDERER);
        assert_eq!(floor["current"], true, "{floor}");
        assert_eq!(floor["law_revision"], law_revision);
        let plain = row(&floor["statements"], "craft/plain.md");
        assert_eq!(plain["standing"], "durable");
        assert_eq!(plain["carried"], "door");
        assert_eq!(plain["topic"], "Plain words");
        assert_eq!(
            row(&floor["statements"], "craft/unrelated.md")["standing"],
            "undeclared"
        );
    }
}

#[test]
fn the_owners_world_lines_come_from_its_own_statements() {
    let ground = owners();
    let receipt = render_in(&ground, &files(&["CLAUDE.md"])).unwrap();
    let text = floor(&ground);
    let load = "## Load by act, not by default\n\n\
- Law: `ctrl control index` lists the statements (topic, standing); read the one your act touches.\n\
- Procedure: the `central-*` skills. `central-session-strap` orients a session and routes it to the practice it needs (no skills: `Control/user/skills/central-session-strap/SKILL.md`).\n\
- Before a field's work, load its methodology: `engineering-methodology` (code; compiled in `Control/agents/governance/engineering/foundational-prompt.md`), `collaboration-methodology`, `authorship-methodology`, `world-keeping-methodology`, `docs-methodology`.\n\
- DLW work: its plan is the Wayfinder map O-I #706. Claim a ticket there.\n\n## Never-do card\n";
    assert!(text.contains(load), "{text}");
    let card = "  re-render; a generated suggestion stays generated until adopted.\n\
- Development happens in the persistent env-1/2/3 seats (`seat join`), and\n  \
“nothing creates another worktree (owner ruling, 29 September 2026, after repeated per-lane worktrees bloated the disk)”.\n\
- “The ground's primary checkout for a project — `Work/<Name>` itself — stands on its `main` branch.” A landed lane releases its seat: “no checkout is left standing on the landed branch”.\n  \
(Both from `Control/agents/governance/world/shape-of-the-world.md`.)\n\
- Carried is not loaded, and loaded is not used: the disclosure answers what\n";
    assert!(text.contains(card), "{text}");
    let tail = "  is actually operative.\n\n\
Durable Control change — the authorship law, quoted from `Control/agents/governance/authorship-and-return/propose-not-write.md`:\n\n\
> Authorship names whose intent originates a change, never whose hands perform it: on my stated intent you perform every operation yourself — including writes into this ground — and handing file mechanics back to me is unfinished work, not deference.\n\n\
Session end, every session: leave no loose folders at the Work root. Write no\n\
handoff or session return by hand: a session's NOW opens and closes with its\n\
hooks (O-I PROGRAMME R36).\n\n\
## Smallest sufficient\n\nThe source can be large. The act should not be.\n\n\
## A draft\n\n*Standing: draft-source (agent-proposed).*\n\nNot law yet.\n";
    assert!(text.ends_with(tail), "{text}");
    // The law behind the door stays behind it.
    assert!(!text.contains("Say it plainly") && !text.contains("Waits for the person"));
    // Where each statement is carried.
    let statements = &receipt["statements"];
    for (file, carried, place) in [
        (
            "communicate-and-engage/smallest-sufficient.md",
            "floor",
            "kernel",
        ),
        ("craft/draft.md", "floor", "kernel"),
        ("world/dlw-plan.md", "floor", "load"),
        ("world/shape-of-the-world.md", "door", "index"),
        (
            "authorship-and-return/propose-not-write.md",
            "door",
            "index",
        ),
        ("craft/plain.md", "door", "index"),
        ("engineering/coding-approach.md", "door", "index"),
        ("craft/unrelated.md", "door", "index"),
    ] {
        let held = row(statements, file);
        assert_eq!(held["carried"], carried, "{held}");
        assert_eq!(held["floor"], place, "{held}");
    }
    assert_eq!(
        row(statements, "world/shape-of-the-world.md")["quoted"],
        true
    );
    assert_eq!(
        row(statements, "authorship-and-return/propose-not-write.md")["standing"],
        "undeclared"
    );
    assert_eq!(
        row(statements, "engineering/coding-approach.md")["compiled_in"],
        COMPILED
    );
    assert_eq!(receipt["unrendered"], json!([]));
    // Eight statements answered: the seven declared and the authorship law the floor quotes.
    assert!(text.contains("; 8 statements, revision "), "{text}");
    let reading = read_in(&ground, &["CLAUDE.md"]);
    assert_eq!(reading["floors"][0]["current"], true);
    assert_eq!(reading["floors"][0]["statements"], receipt["statements"]);
}

#[test]
fn a_quote_its_statement_no_longer_holds_refuses_the_floor() {
    let ground = owners();
    render_in(&ground, &json!({})).unwrap();
    let before = floor(&ground);
    fs::write(
        ground.root.join(SHAPE),
        "# The shape of the world\n\nnothing creates another worktree (owner ruling, 29 September 2026, after repeated per-lane worktrees bloated the disk). The ground's primary checkout for a project — `Work/<Name>` itself — stands on its `main` branch.\n",
    )
    .unwrap();
    let refusal = render_in(&ground, &json!({})).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    let words = refusal.to_string();
    assert!(
        words.contains(&format!(
            "The floor quotes {SHAPE}, and it no longer holds the quoted sentence (“{LANDED}”)"
        )),
        "{words}"
    );
    assert!(words.ends_with("Nothing was written."), "{words}");
    assert_eq!(floor(&ground), before);
    let reading = read_in(&ground, &["AGENTS.md"]);
    assert_eq!(reading["floors"][0]["current"], false);
    assert!(reading["floors"][0]["why"]
        .as_str()
        .unwrap()
        .starts_with("Central would not render it now: The floor quotes"));
}

#[test]
fn adopting_a_statement_moves_the_law_revision_and_the_door_carries_it() {
    let ground = fresh();
    let first = render_in(&ground, &files(&["CLAUDE.md"])).unwrap();
    let path = "Control/agents/governance/craft/new.md";
    fs::write(ground.root.join(path), "# New law\n\nSay what changed.\n").unwrap();
    // Written, not adopted: the door lists it, the floor's law has not moved.
    let reading = read_in(&ground, &["CLAUDE.md"]);
    assert_eq!(reading["floors"][0]["current"], true);
    assert_eq!(
        row(&reading["floors"][0]["statements"], "craft/new.md")["standing"],
        "undeclared"
    );
    let revision = revision_of(&ground, "craft/new.md");
    let input =
        json!({"statement": "craft/new.md", "expected_revision": revision, "files": ["CLAUDE.md"]});
    // Not on an agent's credential, nor on no credential, nor on a revision the person did not read.
    for token in [Some(AGENT), None] {
        assert_eq!(
            adopt(&ground, &input, token, 10).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    let stale =
        json!({"statement": "craft/new.md", "expected_revision": "central.content-fnv1a64/v1:1:0"});
    assert_eq!(
        adopt(&ground, &stale, Some(HUMAN), 10).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    let receipt = adopt(&ground, &input, Some(HUMAN), 10).unwrap();
    assert_eq!(receipt["schema"], LAW_ADOPTION_SCHEMA);
    assert_eq!(receipt["adopted"], true);
    assert_eq!(receipt["adopted_by"], "human:test");
    assert_eq!(receipt["statement"]["floor"], "index");
    // The floor is rendered again: its header's revision moved, the statement carried by the door, its words not pasted in.
    assert_eq!(receipt["floor"]["changed"], true, "{receipt}");
    assert_ne!(receipt["floor"]["law_revision"], first["law_revision"]);
    let text = floor(&ground);
    assert!(!text.contains("Say what changed"), "{text}");
    let reading = read_in(&ground, &["CLAUDE.md"]);
    let floor_read = &reading["floors"][0];
    assert_eq!(floor_read["current"], true);
    assert_eq!(floor_read["law_revision"], receipt["floor"]["law_revision"]);
    let adopted = row(&floor_read["statements"], "craft/new.md");
    assert_eq!(adopted["standing"], "durable");
    assert_eq!(adopted["carried"], "door");
    // Adopted again as it stands: nothing to adopt, nothing written.
    let again = adopt(&ground, &input, Some(HUMAN), 11).unwrap();
    assert_eq!(again["adopted"], false);
    assert_eq!(again["floor"]["changed"], false);
    // Placed on the floor in full: adopted again, the floor carries it under its title.
    let kernel = json!({"statement": path, "expected_revision": revision, "floor": "kernel"});
    let placed = adopt(&ground, &kernel, Some(HUMAN), 12).unwrap();
    assert_eq!(placed["adopted"], true);
    assert_eq!(placed["statement"]["floor"], "kernel");
    assert!(
        floor(&ground).ends_with("## New law\n\nSay what changed.\n"),
        "{}",
        floor(&ground)
    );
    let reading = read_in(&ground, &["CLAUDE.md"]);
    assert_eq!(
        row(&reading["floors"][0]["statements"], "craft/new.md")["carried"],
        "floor"
    );
    // Its relation is durable, the person's own, placed on the floor; one relation, changed in place.
    let relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    let held: Vec<&Value> = relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == path)
        .collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["standing"], "durable-source");
    assert_eq!(held[0]["floor"], "kernel");
    // Back behind the door: the relation keeps no place of its own.
    let index = json!({"statement": path, "expected_revision": revision, "floor": "index"});
    adopt(&ground, &index, Some(HUMAN), 13).unwrap();
    let relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    let held = relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == path)
        .unwrap();
    assert!(held.get("floor").is_none(), "{held}");
    assert!(!floor(&ground).contains("## New law"));
    // A place the floor does not have, and a path outside the governance folder, are no adoption.
    let nowhere = json!({"statement": path, "expected_revision": revision, "floor": "everywhere"});
    assert_eq!(
        adopt(&ground, &nowhere, Some(HUMAN), 14)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    for outside in [
        "../user/authority.json",
        "/etc/passwd",
        "craft/README.md",
        "craft/plain.txt",
    ] {
        let refusal = adopt(
            &ground,
            &json!({"statement": outside, "expected_revision": "x"}),
            Some(HUMAN),
            15,
        );
        assert_eq!(
            refusal.unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "{outside}"
        );
    }
}

#[test]
fn a_draft_adopted_keeps_its_relation_and_moves_the_floors_revision() {
    let ground = owners();
    let first = render_in(&ground, &json!({})).unwrap();
    let revision = revision_of(&ground, "craft/draft.md");
    let receipt = adopt(
        &ground,
        &json!({"statement": "craft/draft.md", "expected_revision": revision}),
        Some(HUMAN),
        10,
    )
    .unwrap();
    // It keeps its place on the floor, now the person's law: its standing mark goes, and the floor's revision moves.
    assert_eq!(receipt["statement"]["floor"], "kernel");
    assert_ne!(receipt["floor"]["law_revision"], first["law_revision"]);
    assert!(floor(&ground).ends_with("## A draft\n\nNot law yet.\n"));
    let relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    let held: Vec<&Value> = relations["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == "Control/agents/governance/craft/draft.md")
        .collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["standing"], "durable-source");
}

#[test]
fn a_floor_another_hand_wrote_is_read_and_never_overwritten() {
    let ground = fresh();
    for (written, renderer) in [
        (
            "# Our floor\n\n<!--\nrenderer: someone-elses-renderer@1; its own words\n-->\n\n## Plain words\n\nSay it plainly.\n",
            Some("someone-elses-renderer@1"),
        ),
        ("# My own notes\n\nFrom `Control/agents/governance/craft/plain.md`.\n", None),
    ] {
        fs::write(ground.root.join(FLOOR_FILE), written).unwrap();
        let refusal = render_in(&ground, &json!({})).unwrap_err();
        assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
        let words = refusal.to_string();
        assert!(words.contains("was not rendered by Central"), "{words}");
        assert!(words.contains("Nothing was written"), "{words}");
        assert_eq!(floor(&ground), written);
        let reading = read_in(&ground, &["AGENTS.md"]);
        let foreign = &reading["floors"][0];
        assert_eq!(foreign["state"], "foreign", "{foreign}");
        assert_eq!(foreign["renderer"].as_str(), renderer);
        // Named by its path, or by its topic as a heading.
        assert_eq!(foreign["statements"][0]["file"], "craft/plain.md", "{foreign}");
        assert!(foreign["why"].as_str().unwrap().contains("Central"));
    }
}

#[test]
fn the_strap_floor_is_this_floor_and_rendered_again_naming_what_it_drops() {
    let ground = fresh();
    let strap = "# Central — session ground (derived; do not edit)\n\n<!-- derived, generated-derived: edit the sources under Control/agents/governance/, then re-render\nrenderer: central-session-strap/render-context.py@2; 39 sources, revision central.content-fnv1a64/v1:4125:c5f9537ec91b5786\n-->\n\n## The field you stand in\n\nOld words.\n\n## Learned from a session\n\nAppended by another writer.\n";
    fs::write(ground.root.join(FLOOR_FILE), strap).unwrap();
    let reading = read_in(&ground, &["AGENTS.md"]);
    let held = &reading["floors"][0];
    assert_eq!(held["state"], "rendered", "{held}");
    assert_eq!(
        held["renderer"],
        "central-session-strap/render-context.py@2"
    );
    assert_eq!(
        held["law_revision"],
        "central.content-fnv1a64/v1:4125:c5f9537ec91b5786"
    );
    assert_eq!(held["current"], false);
    let receipt = render_in(&ground, &json!({})).unwrap();
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["dropped"], json!(["Learned from a session"]));
    assert!(floor(&ground).contains(&format!("renderer: {FLOOR_RENDERER};")));
    assert_eq!(
        read_in(&ground, &["AGENTS.md"])["floors"][0]["current"],
        true
    );
}

#[test]
fn an_instruction_file_of_its_own_is_left_as_it_is() {
    let ground = fresh();
    fs::write(ground.root.join("GEMINI.md"), "the person's own words\n").unwrap();
    let refusal = render_in(&ground, &files(&["CLAUDE.md", "GEMINI.md"])).unwrap_err();
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
fn past_its_budget_the_floor_is_refused_naming_what_it_carries_in_full() {
    let ground = ground_with(&[
        (
            "craft/plain.md",
            "# Plain words\n\nSay it plainly.\n",
            Some(("durable-source", Some("kernel"))),
        ),
        (
            "world/long.md",
            &format!(
                "# The long one\n\n{}\n",
                "Every word of this statement counts. ".repeat(80)
            ),
            Some(("durable-source", None)),
        ),
    ]);
    // Behind the door, a long statement costs the floor nothing.
    render_in(&ground, &json!({})).unwrap();
    let before = floor(&ground);
    let mut relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    for relation in relations["relations"].as_array_mut().unwrap() {
        if relation["path"] == "Control/agents/governance/world/long.md" {
            relation["floor"] = json!("kernel");
        }
    }
    write_relations(&ground.root, relations["relations"].as_array().unwrap());
    let refusal = render_in(&ground, &json!({})).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    let words = refusal.to_string();
    assert!(
        words.contains(&format!("budget of {FLOOR_BUDGET_BYTES}")),
        "{words}"
    );
    assert!(
        words.contains("Plain words (Control/agents/governance/craft/plain.md, 31 bytes); The long one (Control/agents/governance/world/long.md, 2977 bytes)"),
        "{words}"
    );
    assert_eq!(floor(&ground), before, "nothing was written");
}

#[test]
fn a_declared_statement_that_is_lost_is_named() {
    let ground = fresh();
    let scope = source::Scope::resolve(&ground.root, None).unwrap();
    let mut relations: Value = serde_json::from_slice(
        &fs::read(ground.root.join("Control/relations/source-relations.json")).unwrap(),
    )
    .unwrap();
    let lost = "Control/agents/governance/craft/lost-draft.md";
    relations["relations"].as_array_mut().unwrap().push(json!({"ref": scope.source_ref(lost), "path": lost,
        "roles": [GOVERNANCE_ROLE], "provenance": "agent-proposed", "standing": "draft-source",
        "treatment": "retain-native-in-place", "recognition": "controlled-test-only", "recorded_at_unix_seconds": 1}));
    write_relations(&ground.root, relations["relations"].as_array().unwrap());
    // A lost draft is named in the header, and in the reading.
    let receipt = render_in(&ground, &json!({})).unwrap();
    assert!(floor(&ground).contains(
        "unrendered — declared but not carried anywhere (a fault, not a choice):\n- declared source is absent from disk: Control/agents/governance/craft/lost-draft.md\n-->"
    ));
    assert_eq!(receipt["unrendered"].as_array().unwrap().len(), 1);
    // A lost durable statement leaves the floor broken: refused, nothing written.
    let before = floor(&ground);
    let gone = "Control/agents/governance/craft/gone.md";
    relations["relations"].as_array_mut().unwrap().push(json!({"ref": scope.source_ref(gone), "path": gone,
        "roles": [GOVERNANCE_ROLE], "provenance": "human-authored", "standing": "durable-source",
        "treatment": "retain-native-in-place", "recognition": "controlled-test-only", "recorded_at_unix_seconds": 1}));
    write_relations(&ground.root, relations["relations"].as_array().unwrap());
    let refusal = render_in(&ground, &json!({})).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    assert!(
        refusal
            .to_string()
            .starts_with(&format!("The law declares {gone} durable")),
        "{refusal}"
    );
    assert_eq!(floor(&ground), before);
}

#[test]
fn codex_keeps_the_persons_words_and_one_block_naming_the_floor() {
    let ground = owners();
    let codex = ground.home.join(".codex");
    fs::create_dir_all(&codex).unwrap();
    let mine = "# My Codex\n\nmy own words\n\n<!-- central:generated-begin (central-session-strap/render-context.py; do not edit inside) -->\nan old block\n<!-- central:generated-end -->\n\nafter it\n";
    fs::write(codex.join("AGENTS.md"), mine).unwrap();
    let receipt = render_in(&ground, &json!({})).unwrap();
    assert_eq!(receipt["codex"]["changed"], true);
    let text = fs::read_to_string(codex.join("AGENTS.md")).unwrap();
    let root = ground.home.join("Central");
    let responsibility = revision_of(&ground, "authorship-and-return/responsibility.md");
    let expected = format!(
        "# My Codex\n\nmy own words\n\n{CODEX_BEGIN}\n# Central personal world — carried for every Codex session\n\n\
When your working directory is under {root}, you stand in the Central personal world.\n\
Its derived ground is {root}/AGENTS.md — a short floor of doors (`oi`, `ctrl control index`).\n\
Read it before consequential work, even when this repository has its own AGENTS.md:\n\
Codex does not load the root ground from inside a repository.\n\n\
<!-- central:source:control:root:{RESPONSIBILITY} @ {responsibility} -->\n\
Responsibility (`{RESPONSIBILITY}`): “{OWN_THE_FAULT}”\n{CODEX_END}\n\nafter it\n",
        root = root.display()
    );
    assert_eq!(text, expected);
    let again = render_in(&ground, &json!({})).unwrap();
    assert_eq!(again["codex"]["changed"], false);
    // Where Codex keeps no global instructions yet, they are made holding the block alone.
    fs::remove_file(codex.join("AGENTS.md")).unwrap();
    render_in(&ground, &json!({})).unwrap();
    let made = fs::read_to_string(codex.join("AGENTS.md")).unwrap();
    assert!(made.starts_with(CODEX_BEGIN) && made.ends_with(&format!("{CODEX_END}\n")));
}

#[test]
fn the_reading_accounts_for_codexs_block_in_the_home_as_the_render_writes_it() {
    let ground = owners();
    let codex = ground.home.join(".codex");
    let homes = |ground: &Ground| read_in(ground, &["AGENTS.md"])["homes"].clone();
    // No Codex folder at this home: absent, in the render's own words.
    assert_eq!(
        homes(&ground),
        json!([{"file": "~/.codex/AGENTS.md", "state": "absent", "current": false, "why": NO_CODEX}])
    );
    // Codex's own instructions with no block of Central's: absent, and the render writes one.
    fs::create_dir_all(&codex).unwrap();
    fs::write(codex.join("AGENTS.md"), "# My Codex\n\nmy own words\n").unwrap();
    let row = homes(&ground)[0].clone();
    assert_eq!(
        (row["state"].as_str(), row["current"].as_bool()),
        (Some("absent"), Some(false))
    );
    assert!(
        row["why"]
            .as_str()
            .unwrap()
            .contains("holds no block of Central's"),
        "{row}"
    );
    // Rendered: the block stands as the render writes it now.
    render_in(&ground, &json!({})).unwrap();
    assert_eq!(
        homes(&ground),
        json!([{"file": "~/.codex/AGENTS.md", "state": "rendered", "current": true,
            "renderer": "central control.floor.render"}])
    );
    // A hand inside the block: still Central's block, not the one it writes now, and why.
    let text = fs::read_to_string(codex.join("AGENTS.md")).unwrap();
    fs::write(
        codex.join("AGENTS.md"),
        text.replace("for every Codex session", "by hand"),
    )
    .unwrap();
    let row = homes(&ground)[0].clone();
    assert_eq!(
        (row["state"].as_str(), row["current"].as_bool()),
        (Some("rendered"), Some(false))
    );
    assert!(
        row["why"]
            .as_str()
            .unwrap()
            .contains("control.floor.render writes it again"),
        "{row}"
    );
    // The strap's block, before Central renders: its renderer named, not current.
    fs::write(codex.join("AGENTS.md"), text.replace(CODEX_BEGIN, "<!-- central:generated-begin (central-session-strap/render-context.py; do not edit inside) -->")).unwrap();
    let row = homes(&ground)[0].clone();
    assert_eq!(row["renderer"], "central-session-strap/render-context.py");
    assert_eq!(row["current"], false);
    // The render writes it again, keeping the person's words, and it reads back current.
    render_in(&ground, &json!({})).unwrap();
    assert_eq!(homes(&ground)[0]["current"], true);
    assert!(fs::read_to_string(codex.join("AGENTS.md"))
        .unwrap()
        .starts_with("# My Codex\n\nmy own words\n"));
}

#[test]
fn codex_carries_the_floor_only_for_the_homes_own_central_world() {
    let ground = owners();
    let codex = ground.home.join(".codex");
    fs::create_dir_all(&codex).unwrap();
    let mine = "# My Codex\n\nmy own words\n";
    fs::write(codex.join("AGENTS.md"), mine).unwrap();
    // Another world rendered with this home: Codex's block at this home is not its to write.
    let other = owners();
    let receipt = render(&other.root, &json!({}), Some(&ground.home)).unwrap();
    assert_eq!(receipt["codex"]["changed"], false, "{receipt}");
    assert_eq!(receipt["codex"]["file"], Value::Null);
    assert!(
        receipt["codex"]["why"]
            .as_str()
            .unwrap()
            .contains("not this home's own Central world"),
        "{receipt}"
    );
    assert_eq!(fs::read_to_string(codex.join("AGENTS.md")).unwrap(), mine);
    // A world under this home that is not its ~/Central: the same.
    let elsewhere = ground.home.join("Elsewhere");
    fs::rename(&ground.root, &elsewhere).unwrap();
    let receipt = render(&elsewhere, &json!({}), Some(&ground.home)).unwrap();
    assert_eq!(receipt["codex"]["changed"], false, "{receipt}");
    assert_eq!(fs::read_to_string(codex.join("AGENTS.md")).unwrap(), mine);
    // The home's own ~/Central carries it.
    fs::rename(&elsewhere, &ground.root).unwrap();
    let receipt = render_in(&ground, &json!({})).unwrap();
    assert_eq!(receipt["codex"]["changed"], true, "{receipt}");
    assert!(fs::read_to_string(codex.join("AGENTS.md"))
        .unwrap()
        .contains(CODEX_END));
}

#[test]
fn a_codex_file_central_cannot_write_refuses_the_render_before_anything_is_written() {
    let ground = owners();
    let codex = ground.home.join(".codex");
    fs::create_dir_all(&codex).unwrap();
    let kept = ground.home.join("my-agents.md");
    fs::write(&kept, "# kept elsewhere\n").unwrap();
    // Codex's global instructions as a link to a file the person keeps elsewhere, then as a file with a second link.
    std::os::unix::fs::symlink(&kept, codex.join("AGENTS.md")).unwrap();
    for _ in 0..2 {
        let refused = render_in(&ground, &files(&["CLAUDE.md"])).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
        assert!(
            refused.to_string().contains("Nothing was written"),
            "{refused}"
        );
        assert!(
            !ground.root.join(FLOOR_FILE).exists(),
            "the floor was written"
        );
        assert!(fs::symlink_metadata(ground.root.join("CLAUDE.md")).is_err());
        assert_eq!(fs::read_to_string(&kept).unwrap(), "# kept elsewhere\n");
        fs::remove_file(codex.join("AGENTS.md")).unwrap();
        fs::hard_link(&kept, codex.join("AGENTS.md")).unwrap();
    }
}

#[test]
fn adopting_says_the_floor_changed_whenever_the_render_wrote_anything() {
    let ground = fresh();
    let path = "Control/agents/governance/craft/unrelated.md";
    let revision = revision_of(&ground, "craft/unrelated.md");
    let input = json!({"statement": path, "expected_revision": revision});
    let first = adopt(&ground, &input, Some(HUMAN), 10).unwrap();
    assert_eq!(first["floor"]["changed"], true);
    // Codex is now kept at this home: adopting the same statement again changes no law and no floor, but writes Codex's block.
    fs::create_dir_all(ground.home.join(".codex")).unwrap();
    let again = adopt(&ground, &input, Some(HUMAN), 20).unwrap();
    assert_eq!(again["adopted"], false);
    assert_eq!(again["floor"]["revision"], first["floor"]["revision"]);
    assert_eq!(again["floor"]["codex"]["changed"], true, "{again}");
    assert_eq!(again["floor"]["changed"], true, "{again}");
    // A link made, and nothing else: changed.
    let mut linked = input.clone();
    linked["files"] = json!(["GEMINI.md"]);
    let made = adopt(&ground, &linked, Some(HUMAN), 30).unwrap();
    assert_eq!(made["floor"]["links"][0]["state"], "made");
    assert_eq!(made["floor"]["changed"], true, "{made}");
    // Nothing written: not changed.
    let still = adopt(&ground, &linked, Some(HUMAN), 40).unwrap();
    assert_eq!(still["floor"]["links"][0]["state"], "kept");
    assert_eq!(still["floor"]["changed"], false, "{still}");
}

#[test]
fn a_statements_words_are_its_text_without_front_matter_or_title() {
    assert_eq!(
        body_of("---\nstanding: durable\n---\n# Title\n\n\nWords.\n\nMore.\n\n"),
        "Words.\n\nMore."
    );
    assert_eq!(
        body_of("Words without a title.\n"),
        "Words without a title."
    );
    assert_eq!(
        title_of("no title here", "x/smallest-sufficient.md"),
        "Smallest sufficient"
    );
    assert_eq!(title_of("x\n# The title \n", "x/a.md"), "The title");
}

#[test]
fn adopting_on_a_foreign_floor_keeps_the_adoption_and_says_why_the_floor_stands() {
    let ground = fresh();
    let foreign = "# Our floor\n\nrenderer: someone-elses-renderer@1\n";
    fs::write(ground.root.join(FLOOR_FILE), foreign).unwrap();
    let path = "Control/agents/governance/craft/unrelated.md";
    let revision = revision_of(&ground, "craft/unrelated.md");
    let receipt = adopt(
        &ground,
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
    assert_eq!(floor(&ground), foreign);
}

#[test]
fn where_there_is_no_central_ground_the_reading_says_so_in_words() {
    let ground = fresh();
    let missing = ground.root.join("no-such-ground");
    let refusal = read(&missing, &files(&["AGENTS.md"]), None).unwrap_err();
    assert_eq!(refusal.kind(), io::ErrorKind::NotFound);
    assert!(
        refusal.to_string().starts_with("no Central ground at "),
        "{refusal}"
    );
    assert!(!refusal.to_string().contains("os error"), "{refusal}");
    fs::create_dir_all(&missing).unwrap();
    let refusal = render(&missing, &json!({}), None).unwrap_err();
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
    // A reading names the files as they stand.
    assert_eq!(
        read(&ground.root, &json!({}), None).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        read(&ground.root, &files(&["../AGENTS.md"]), None)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        read(&ground.root, &files(&["GEMINI.md"]), None).unwrap()["floors"][0]["state"],
        "absent"
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
