//! The Documentation Field praxis: one METHODOLOGY-classified Skill, seven
//! METHOD-classified Skills, five unprefixed form Skills with their templates,
//! and the portable registry SkillSets that carry them.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

const FORMS: [&str; 5] = [
    "vision-authoring",
    "design-authoring",
    "ui-mockup-authoring",
    "architecture-authoring",
    "diagram-authoring",
];
const METHODS: [&str; 7] = [
    "product-development",
    "ui-development",
    "architecture-development",
    "evidence-led-repair",
    "documentation-reconciliation",
    "reverse-recovery",
    "experimental-development",
];
const ACCOUNT_AUTHORING: [&str; 3] = [
    "skill/aikit/product-understanding",
    "skill/aikit/structured-account-authoring",
    "skill/aikit/html-account",
];

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(relative: &str) -> String {
    fs::read_to_string(repository_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// Front matter `key: value` pairs of a SKILL.md, quotes removed.
fn front_matter(skill: &str) -> BTreeMap<String, String> {
    let text = read(&format!("skills/{skill}/SKILL.md"));
    let body = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("{skill}: SKILL.md must open with front matter"));
    let end = body
        .find("\n---")
        .unwrap_or_else(|| panic!("{skill}: unterminated front matter"));
    body[..end]
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| {
            (
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            )
        })
        .collect()
}

fn description(skill: &str) -> String {
    front_matter(skill)
        .remove("description")
        .unwrap_or_else(|| panic!("{skill}: description required"))
}

#[test]
fn praxis_forms_are_classified_by_description_prefix() {
    assert!(description("docs-methodology").starts_with("METHODOLOGY: "));
    for method in METHODS {
        let text = description(method);
        assert!(text.starts_with("METHOD: "), "{method}: {text}");
        assert!(!text.starts_with("METHODOLOGY:"), "{method}");
    }
    for form in FORMS {
        let text = description(form);
        assert!(
            !text.starts_with("METHOD:") && !text.starts_with("METHODOLOGY:"),
            "{form} must be an ordinary Skill: {text}"
        );
    }
}

#[test]
fn every_documentation_skill_name_matches_its_directory() {
    for skill in ["docs-methodology"].into_iter().chain(FORMS).chain(METHODS) {
        assert_eq!(
            front_matter(skill).get("name").map(String::as_str),
            Some(skill)
        );
    }
}

#[test]
fn methodology_names_every_method_and_form_and_the_local_repair_exit() {
    let body = read("skills/docs-methodology/SKILL.md");
    for skill in FORMS.into_iter().chain(METHODS) {
        assert!(
            body.contains(&format!("../{skill}/SKILL.md")),
            "docs-methodology must point at {skill}"
        );
    }
    assert!(body.contains("small local repair selects none of these and loads no Vision"));
    assert!(body.contains("Wayfinder"));
    assert!(body.contains("../documentation-standing/SKILL.md"));
}

fn assert_standalone_html(relative: &str) {
    let html = read(relative);
    assert!(
        html.trim_start()
            .to_lowercase()
            .starts_with("<!doctype html>"),
        "{relative}"
    );
    assert!(html.trim_end().ends_with("</html>"), "{relative}");
    let lower = html.to_lowercase();
    let mut rest = lower.as_str();
    while let Some(index) = rest.find("<script") {
        let tag_end = rest[index..].find('>').expect("closed script tag") + index;
        assert!(
            !rest[index..tag_end].contains("src="),
            "{relative} must not load an external script"
        );
        rest = &rest[tag_end..];
    }
    let mut rest = lower.as_str();
    while let Some(index) = rest.find("<link") {
        let tag_end = rest[index..].find('>').expect("closed link tag") + index;
        let tag = &rest[index..tag_end];
        assert!(
            !tag.contains("href=\"http") || tag.contains("fonts.googleapis.com"),
            "{relative} may only link Google Fonts externally"
        );
        rest = &rest[tag_end..];
    }
}

#[test]
fn form_templates_exist_and_html_templates_are_standalone() {
    assert_standalone_html("skills/vision-authoring/assets/vision-template.html");
    assert_standalone_html("skills/ui-mockup-authoring/assets/ui-mockup-template.html");

    let vision = read("skills/vision-authoring/assets/vision-template.html");
    assert!(vision.contains("full-account-template.html"));
    assert!(vision.contains("data-whole-anchor=\"0/1\""));
    let mockup = read("skills/ui-mockup-authoring/assets/ui-mockup-template.html");
    for attribute in [
        "data-state=\"failure\"",
        "data-state=\"degraded\"",
        "data-design-ref=",
        "data-vision-ref=",
        "data-capability-ref=",
        "id=\"switcher\"",
    ] {
        assert!(
            mockup.contains(attribute),
            "mockup template lacks {attribute}"
        );
    }

    assert!(read("skills/design-authoring/assets/design-template.md").contains("role: design"));
    assert!(
        read("skills/architecture-authoring/assets/architecture-template.md")
            .contains("role: architecture")
    );
}

#[test]
fn diagram_templates_open_with_the_header_block() {
    for kind in [
        "context",
        "topology",
        "sequence",
        "state",
        "dataflow",
        "dependency",
    ] {
        let text = read(&format!("skills/diagram-authoring/assets/{kind}.mmd"));
        let header = text
            .lines()
            .take_while(|line| line.starts_with("%%"))
            .collect::<Vec<_>>()
            .join("\n");
        for field in [
            "%% source_id:",
            "%% visual_question:",
            "%% companion_doc:",
            "%% revision:",
            "%% provenance:",
        ] {
            assert!(header.contains(field), "{kind}.mmd header lacks {field}");
        }
    }
}

#[derive(Default, Debug)]
struct Entry {
    semantic_ref: String,
    directory: String,
    description: String,
    child_refs: Vec<String>,
    package: BTreeMap<String, String>,
}

fn string_value(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

/// Reads the fixed registry index shape (`schema`, `[[skillset]]`,
/// `[skillset.package]`). ctrl carries no TOML dependency; AIKit owns the
/// authoritative parser.
fn registry_index() -> (u32, Vec<Entry>) {
    let text = read("skillsets/index.toml");
    let mut schema = 0;
    let mut entries: Vec<Entry> = Vec::new();
    let mut in_package = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[skillset]]" {
            entries.push(Entry::default());
            in_package = false;
            continue;
        }
        if line == "[skillset.package]" {
            in_package = true;
            continue;
        }
        assert!(!line.starts_with('['), "unexpected table {line}");
        let (key, value) = line.split_once('=').expect("key = value");
        let key = key.trim();
        match (entries.last_mut(), in_package) {
            (None, _) => {
                assert_eq!(key, "schema");
                schema = value.trim().parse().unwrap();
            }
            (Some(entry), true) => {
                entry.package.insert(key.to_string(), string_value(value));
            }
            (Some(entry), false) => match key {
                "semantic_ref" => entry.semantic_ref = string_value(value),
                "directory" => entry.directory = string_value(value),
                "description" => entry.description = string_value(value),
                "child_refs" => {
                    entry.child_refs = value
                        .trim()
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(string_value)
                        .filter(|item| !item.is_empty())
                        .collect()
                }
                other => panic!("unknown registry SkillSet key {other}"),
            },
        }
    }
    (schema, entries)
}

fn members(directory: &str) -> Vec<String> {
    read(&format!("skillsets/{directory}/members"))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn registry_skillsets_resolve_without_duplication() {
    let (schema, entries) = registry_index();
    assert_eq!(schema, 1);
    let refs = entries
        .iter()
        .map(|entry| entry.semantic_ref.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        refs,
        BTreeSet::from(["central:core-development", "central:documentation"])
    );

    for entry in &entries {
        assert!(!entry.description.is_empty(), "{}", entry.semantic_ref);
        let listed = members(&entry.directory);
        let unique = listed.iter().collect::<BTreeSet<_>>();
        assert_eq!(
            unique.len(),
            listed.len(),
            "{} duplicates a member",
            entry.semantic_ref
        );
        for member in &listed {
            if let Some(name) = member.strip_prefix("skill/central/") {
                assert!(
                    repository_root()
                        .join(format!("skills/{name}/SKILL.md"))
                        .is_file(),
                    "{member} does not resolve to skills/{name}/SKILL.md"
                );
            }
        }
    }

    let documentation = entries
        .iter()
        .find(|entry| entry.semantic_ref == "central:documentation")
        .unwrap();
    assert_eq!(documentation.directory, "documentation");
    assert!(documentation
        .child_refs
        .contains(&"aikit:account-authoring".to_string()));
    let listed = members(&documentation.directory);
    for carried_by_child in ACCOUNT_AUTHORING {
        assert!(
            !listed.iter().any(|member| member == carried_by_child),
            "{carried_by_child} is carried by aikit:account-authoring, not flat"
        );
    }
    for skill in [
        "docs-methodology",
        "documentation-standing",
        "capability-matrices",
    ]
    .into_iter()
    .chain(FORMS)
    .chain(METHODS)
    {
        assert!(
            listed.contains(&format!("skill/central/{skill}")),
            "central:documentation lacks {skill}"
        );
    }
    assert_eq!(
        documentation.package.get("name").map(String::as_str),
        Some("epi-logos-documentation")
    );
    assert_eq!(
        documentation.package.get("version").map(String::as_str),
        Some("0.1.0")
    );
}
