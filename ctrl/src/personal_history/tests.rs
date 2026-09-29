//! Personal-history intake tests: adapter classification, placement plan
//! laws, human-accepted apply, repeat/changed imports, and rollback.

use super::*;
use crate::pasu::{
    PasuIdentityManifest, PasuIdentitySource, PasuSourcedFile, PasuSubject,
    PASU_IDENTITY_SOURCE_DIR,
};
use crate::tempdir;

fn anchor_manifest(root: &Path) {
    fs::create_dir_all(root.join(PASU_IDENTITY_SOURCE_DIR)).unwrap();
    fs::write(
        root.join("Control/user/identity/present.md"),
        "who I am now",
    )
    .unwrap();
    PasuIdentityManifest::new(
        PasuSubject {
            ref_: PasuRef::local_nara(),
            title: Some("test subject".to_owned()),
        },
        PasuIdentitySource {
            path: PASU_IDENTITY_SOURCE_DIR.to_owned(),
            provenance_law: "test".to_owned(),
            sources: vec![PasuSourcedFile {
                path: "Control/user/identity/present.md".to_owned(),
                standing: "authored-ground".to_owned(),
                promoted: None,
                revision: None,
            }],
        },
    )
    .persist(root)
    .unwrap();
}

fn entry(origin: &Path, relative: &str, body: &str) {
    let path = origin.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn sample_plan(root: &Path, origin: &Path, mode: Option<&str>) -> CollectionPlan {
    anchor_manifest(root);
    build_plan(
        root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            collection_id: Some("field-notes".to_owned()),
            mode: mode.map(str::to_owned),
            ..Default::default()
        },
    )
    .unwrap()
}

fn date_parses_to(raw: &str, expected: Option<&str>, approximate: bool) {
    let (date, was_approximate) = parse_date(raw);
    assert_eq!(date.as_deref(), expected, "parsing {raw}");
    assert_eq!(was_approximate, approximate, "approximation of {raw}");
}

#[test]
fn dates_parse_with_honest_approximation() {
    date_parses_to("2026-03-14", Some("2026-03-14"), false);
    date_parses_to("2026-03-14T09:30", Some("2026-03-14"), false);
    date_parses_to("~2026", Some("2026"), true);
    date_parses_to("2026-03", Some("2026-03"), true);
    date_parses_to("midwinter", None, true);
    date_parses_to("", None, false);
}

#[test]
fn members_classify_whole_files_without_interpreting_authority() {
    let temp = tempdir().unwrap();
    let origin = temp.path().join("collection");
    entry(
        &origin,
        "journal/2026-03-14.md",
        "---\ndate: 2026-03-14\ntype: journal\nweather: sleet\n---\n# First thaw\n\nस्नोड्रॉप्स 🌱\n",
    );
    entry(
        &origin,
        "journal/dream.md",
        "---\ndate: 2026-04-02\ntype: dream\n---\nTwo doors.\n",
    );
    {
        let scan = origin.join("attachments/scan.bin");
        fs::create_dir_all(scan.parent().unwrap()).unwrap();
        fs::write(&scan, [0x00u8, 0x01, 0xff, 0xfe]).unwrap();
    }
    entry(&origin, "boards/moods.canvas", "{\"lines\":[]}");
    entry(&origin, "private/secret.md", "excluded from agents");
    fs::write(origin.join("private/.no-agent-retrieval"), "").unwrap();

    let members = read_members(&origin, MARKDOWN_ADAPTER).unwrap();
    let by_id = |id: &str| members.iter().find(|m| m.entry_id == id).unwrap();

    let thaw = by_id("journal/2026-03-14.md");
    assert_eq!(thaw.role, "entry");
    assert_eq!(thaw.entry_type, "journal");
    assert_eq!(thaw.event_date.as_deref(), Some("2026-03-14"));
    assert_eq!(thaw.date_basis.as_deref(), Some("frontmatter date"));
    assert_eq!(thaw.disposition, "retained");
    assert_eq!(
        thaw.entry_meta.get("weather").map(String::as_str),
        Some("sleet")
    );

    let dream = by_id("journal/dream.md");
    assert_eq!(dream.entry_type, "dream");
    assert_eq!(dream.role, "entry");

    let scan = by_id("attachments/scan.bin");
    assert_eq!(scan.disposition, "unreadable");
    assert_eq!(scan.readable, Some(false));
    assert!(scan.reason.is_some());

    let canvas = by_id("boards/moods.canvas");
    assert_eq!(canvas.role, "record");

    let secret = by_id("private/secret.md");
    assert_eq!(secret.disposition, "excluded-by-selection");
}

#[test]
fn plan_copies_external_collections_and_refuses_the_wrong_person() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "2026-03-14.md", "# First thaw\n");

    // No person anchor yet: the plan must say so, not invent one.
    let missing = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            ..Default::default()
        },
    );
    assert!(missing.err().unwrap().contains("no person anchor"));

    anchor_manifest(&root);
    let wrong = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            person_ref: Some("central:pasu:nara:someone-else".to_owned()),
            ..Default::default()
        },
    );
    assert!(wrong.err().unwrap().contains("anchored subject"));

    let plan = sample_plan(&root, &origin, None);
    assert_eq!(plan.placement.mode, "copy");
    assert_eq!(plan.person_ref, "central:pasu:nara:local");
    assert_eq!(plan.entries.len(), 1);
    let only = &plan.entries[0];
    assert_eq!(only.action, "copy-register");
    assert_eq!(
        only.destination.as_deref(),
        Some("Control/user/collections/field-notes/2026-03-14.md")
    );
    assert!(only.source_ref.starts_with("central:source:control:root:"));
    assert!(plan
        .plan_revision
        .starts_with("central.content-fnv1a64/v1:"));
}

#[test]
fn plan_retains_in_place_inside_the_world() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = root.join("Work/Notes/vault");
    entry(&origin, "a.md", "# A\n");
    anchor_manifest(&root);

    let plan = build_plan(
        &root,
        &PlanRequest {
            path: Some("Work/Notes/vault".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plan.placement.mode, "retain-in-place");
    let only = &plan.entries[0];
    assert_eq!(only.action, "register");
    assert!(only.destination.is_none());
    assert_eq!(
        only.source_ref,
        "central:source:control:root:Work/Notes/vault/a.md".to_owned()
    );
}

#[test]
fn apply_copies_exact_bytes_and_records_adopted_relations() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    let body = "# First thaw\n\nक्षेत्र 🌱\n";
    entry(&origin, "2026-03-14.md", body);

    let plan = sample_plan(&root, &origin, None);
    let outcome = apply_plan(&root, &plan, "human-accepted").unwrap();
    assert_eq!(outcome.receipt.entries_added, 1);
    assert!(outcome.steps_refused.is_empty());

    // Exact bytes survived.
    let copied = root.join("Control/user/collections/field-notes/2026-03-14.md");
    assert_eq!(fs::read(&copied).unwrap(), body.as_bytes());

    // The source is registered and the relation is human-adopted.
    let scope = scope_for(&root, None).unwrap();
    let ground = scope.ground().unwrap();
    assert!(ground.resources.contains_key(&plan.entries[0].source_ref));
    let doc = scope.document().unwrap();
    let relation = doc["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ref"] == json!(plan.entries[0].source_ref))
        .unwrap();
    assert_eq!(relation["provenance"], json!("human-adopted"));
    assert_eq!(relation["roles"][0], json!(ROLE_ENTRY));

    // The record exists, carries the entry and the receipt, and verifies.
    let record = PersonalCollectionRecord::load(&root, "field-notes")
        .unwrap()
        .unwrap();
    assert_eq!(record.entries.len(), 1);
    assert_eq!(record.imports.len(), 1);
    assert_eq!(record.person_ref, "central:pasu:nara:local");
    let reading = verify_collection(&root, None, "field-notes").unwrap();
    assert_eq!(reading.verified, 1);
    assert!(reading.problems.is_empty());

    // Apply again without acceptance: refused; nothing changed.
    let second = apply_plan(&root, &plan, "agent-suggested");
    assert!(second.err().unwrap().contains("human-accepted"));
}

#[test]
fn apply_refuses_an_origin_that_moved_mid_flight() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    let plan = sample_plan(&root, &origin, None);
    entry(&origin, "a.md", "one\ntwo\n");
    let error = apply_plan(&root, &plan, "human-accepted").unwrap_err();
    assert!(error.contains("origin changed since the plan"));
    assert!(!root
        .join("Control/user/collections/field-notes/a.md")
        .exists());
}

#[test]
fn repeat_imports_are_idempotent_and_updates_keep_history() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    entry(&origin, "b.md", "two\n");

    let plan = sample_plan(&root, &origin, None);
    apply_plan(&root, &plan, "human-accepted").unwrap();

    // Identical re-export: every member unchanged, nothing to apply.
    let same = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            collection_id: Some("field-notes".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        same.entries.iter().filter(|e| e.action == "none").count(),
        2
    );
    let error = apply_plan(&root, &same, "human-accepted").unwrap_err();
    assert!(error.contains("nothing to apply"));

    // Overlapping export: one entry changed at origin, one new file appears.
    entry(&origin, "a.md", "one\ntwo\n");
    entry(&origin, "c.md", "three\n");
    let overlap = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            collection_id: Some("field-notes".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    let update = overlap
        .entries
        .iter()
        .find(|e| e.entry_id == "a.md")
        .unwrap();
    assert_eq!(update.action, "update");
    assert_eq!(
        update.prior_revision.as_deref(),
        Some(fnv(b"one\n").revision.as_str())
    );
    assert!(overlap
        .entries
        .iter()
        .any(|e| e.entry_id == "c.md" && e.action == "copy-register"));

    let outcome = apply_plan(&root, &overlap, "human-accepted").unwrap();
    assert_eq!(outcome.receipt.entries_changed, 1);
    assert_eq!(outcome.receipt.entries_added, 1);

    let record = PersonalCollectionRecord::load(&root, "field-notes")
        .unwrap()
        .unwrap();
    let updated = record
        .entries
        .iter()
        .find(|e| e.entry_id == "a.md")
        .unwrap();
    assert_eq!(updated.alternatives.len(), 1);
    assert_eq!(updated.alternatives[0].replaced_by_import, 2);
    assert_eq!(record.imports.len(), 2);

    // The restore point exists for the updated member.
    let restore = alternatives_area(&root, "field-notes", 2).join("a.md");
    assert_eq!(fs::read(&restore).unwrap(), b"one\n");
}

#[test]
fn divergent_retained_copies_are_reported_never_overwritten() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    let plan = sample_plan(&root, &origin, None);
    apply_plan(&root, &plan, "human-accepted").unwrap();

    // The person edits the retained copy; the origin also moves on.
    fs::write(
        root.join("Control/user/collections/field-notes/a.md"),
        "one\nedited by hand\n",
    )
    .unwrap();
    entry(&origin, "a.md", "one\ntwo\nthree\n");

    let replan = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            collection_id: Some("field-notes".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(replan
        .conflicts
        .iter()
        .any(|line| line.contains("edited after import")));
    let update = replan
        .entries
        .iter()
        .find(|e| e.entry_id == "a.md")
        .unwrap();
    assert_eq!(update.action, "none");

    // The human edit survived untouched.
    assert_eq!(
        fs::read(root.join("Control/user/collections/field-notes/a.md")).unwrap(),
        b"one\nedited by hand\n"
    );
}

#[test]
fn rollback_removes_owned_effects_and_preserves_later_edits() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    entry(&origin, "b.md", "two\n");
    let plan = sample_plan(&root, &origin, None);
    apply_plan(&root, &plan, "human-accepted").unwrap();

    let record = PersonalCollectionRecord::load(&root, "field-notes")
        .unwrap()
        .unwrap();
    let revision = record.record_revision(&root).unwrap();

    // One member is edited by hand after import; the other is untouched.
    fs::write(
        root.join("Control/user/collections/field-notes/b.md"),
        "two\nwith later thoughts\n",
    )
    .unwrap();

    let report = rollback_import(&root, None, "field-notes", 1, &revision).unwrap();
    assert_eq!(report.removed_entries, vec!["a.md"]);
    assert_eq!(report.preserved_entries, vec!["b.md"]);
    assert!(report.record_removed);
    assert!(!root
        .join("Control/user/collections/field-notes/a.md")
        .exists());
    // The later edit is not reverted by the rollback.
    assert_eq!(
        fs::read(root.join("Control/user/collections/field-notes/b.md")).unwrap(),
        b"two\nwith later thoughts\n"
    );
    assert!(PersonalCollectionRecord::load(&root, "field-notes")
        .unwrap()
        .is_none());
}

#[test]
fn rollback_of_an_update_restores_the_prior_revision() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    let plan = sample_plan(&root, &origin, None);
    apply_plan(&root, &plan, "human-accepted").unwrap();

    entry(&origin, "a.md", "one\ntwo\n");
    let overlap = build_plan(
        &root,
        &PlanRequest {
            path: Some(origin.to_string_lossy().to_string()),
            collection_id: Some("field-notes".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    apply_plan(&root, &overlap, "human-accepted").unwrap();
    assert_eq!(
        fs::read(root.join("Control/user/collections/field-notes/a.md")).unwrap(),
        b"one\ntwo\n"
    );

    let record = PersonalCollectionRecord::load(&root, "field-notes")
        .unwrap()
        .unwrap();
    let revision = record.record_revision(&root).unwrap();
    let report = rollback_import(&root, None, "field-notes", 2, &revision).unwrap();
    assert_eq!(report.restored_entries, vec!["a.md"]);
    assert_eq!(
        fs::read(root.join("Control/user/collections/field-notes/a.md")).unwrap(),
        b"one\n"
    );
}

#[test]
fn rollback_refuses_a_stale_record_revision() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    let plan = sample_plan(&root, &origin, None);
    apply_plan(&root, &plan, "human-accepted").unwrap();
    let error = rollback_import(
        &root,
        None,
        "field-notes",
        1,
        "central.content-fnv1a64/v1:2:0000000000000000",
    )
    .unwrap_err();
    assert!(error.contains("record moved"));
}

#[test]
fn plan_identity_survives_a_json_round_trip() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    fs::create_dir_all(root.join("Control/user")).unwrap();
    let origin = temp.path().join("archive");
    entry(&origin, "a.md", "one\n");
    let plan = sample_plan(&root, &origin, None);
    let value = serde_json::to_value(&plan).unwrap();
    let back: CollectionPlan = serde_json::from_value(value).unwrap();
    assert_eq!(plan_identity(&back).unwrap(), plan.plan_revision);
    assert_eq!(back, plan);
}
