//! Explicit, bounded source relocation using the current relation/Flow owners.
//! Journals are recovery intent, never replacement authored source or a Git reset.
use super::{
    authority::Principal,
    placement,
    source::{self, conflict, denied, encoded, invalid, text, Scope},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    ffi::CString,
    fs::{self, File},
    io,
    os::unix::{fs::MetadataExt, io::AsRawFd},
    path::Path,
};

const AREA: &str = ".central/temporal-migrations";
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Move {
    source_ref: String,
    from: String,
    to: String,
    revision: String,
    device: u64,
    inode: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetadataChange {
    path: String,
    before: Option<String>,
    after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Plan {
    plan_ref: String,
    request_key: String,
    scope_ref: String,
    policy_revision: String,
    actor_ref: String,
    moves: Vec<Move>,
    metadata: Vec<MetadataChange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    schema: String,
    plan: Plan,
    plan_revision: String,
    phase: String,
    created_at_unix_seconds: u64,
    updated_at_unix_seconds: u64,
}
fn relative(raw: &str) -> io::Result<std::path::PathBuf> {
    let path = crate::projectcentral_flow::relative_member(raw)?;
    if path
        .components()
        .any(|c| matches!(c.as_os_str().to_str(), Some(".git" | ".central")))
    {
        return Err(denied(
            "source migration does not relocate Git or Central private persistence",
        ));
    }
    Ok(path)
}
fn optional(scope: &Scope, path: &str) -> io::Result<Option<String>> {
    match crate::source_safety::read(&scope.root, path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn journal_path(reference: &str) -> String {
    format!("{AREA}/{}.json", source::key(reference))
}
fn save(scope: &Scope, journal: &Journal) -> io::Result<()> {
    source::directories(&scope.root, Path::new(AREA))?;
    let bytes = serde_json::to_vec_pretty(journal)?;
    if bytes.len() > crate::source_safety::MAX_SOURCE {
        return Err(invalid(
            "migration journal exceeds native bounded recovery size",
        ));
    }
    crate::file_mutation::atomic_record(
        &scope.root.join(journal_path(&journal.plan.plan_ref)),
        &bytes,
    )
}
fn load(scope: &Scope, reference: &str) -> io::Result<Journal> {
    let raw = crate::source_safety::read(&scope.root, &journal_path(reference))?;
    let journal: Journal = serde_json::from_str(&raw)?;
    if journal.schema != "central.temporal-migration/v1"
        || journal.plan.plan_ref != reference
        || journal.plan.scope_ref != scope.world_ref
        || journal.plan_revision != source::revision(&encoded(&journal.plan)?)
    {
        return Err(invalid(
            "migration journal identity or immutable plan basis is invalid",
        ));
    }
    Ok(journal)
}
fn report(journal: &Journal) -> io::Result<Value> {
    Ok(
        json!({"schema":journal.schema,"plan_ref":journal.plan.plan_ref,"plan_revision":journal.plan_revision,
        "journal_revision":source::revision(&encoded(journal)?),"phase":journal.phase,"plan":journal.plan,
        "created_at_unix_seconds":journal.created_at_unix_seconds,"updated_at_unix_seconds":journal.updated_at_unix_seconds,
        "source_bodies_in_journal":false,"git_index_or_worktree_reset":false,"personal_governance_adoption":false,
        "scope_of_operation":"explicit selected SourceRefs only; no whole-directory or installed-world acceptance",
        "empty_destination_directories_retained_after_rollback":true}),
    )
}
pub fn read(scope: &Scope, input: &Value) -> io::Result<Value> {
    report(&load(scope, text(input, "plan_ref")?)?)
}

pub fn plan(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    principal.require_human()?;
    let policy = placement::checked_policy(scope, input, now)?;
    let selections = input
        .get("moves")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            invalid("moves must be an explicit array of source_ref/expected_revision/to selections")
        })?;
    if selections.is_empty() || selections.len() > 32 {
        return Err(invalid("select 1..32 sources per inspectable migration"));
    }
    let request_key = source::key(&format!(
        "{}\n{}\n{}",
        principal.principal_ref,
        text(input, "request_id")?,
        serde_json::to_string(selections)?
    ));
    let plan_ref = format!(
        "central:migration:{}:{}",
        scope.world_ref,
        source::key(&format!(
            "{}\n{}",
            principal.principal_ref,
            text(input, "request_id")?
        ))
    );
    match load(scope, &plan_ref) {
        Ok(existing) => {
            if existing.plan.request_key != request_key {
                return Err(conflict(
                    "migration request identity has different selections",
                ));
            }
            return report(&existing);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let (mut relations, _) = scope.relations()?;
    let relations_before = optional(scope, &scope.relations_path)?;
    let mut moves = Vec::new();
    let mut refs = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for selection in selections {
        let reading = scope.read(text(selection, "source_ref")?)?;
        if reading.revision.revision != text(selection, "expected_revision")? {
            return Err(conflict("selected source revision is stale"));
        }
        if reading.source.roles.iter().any(|r| {
            matches!(
                r.as_str(),
                "now-clearing"
                    | "work-placement-policy"
                    | "civil-time-policy"
                    | "native-action-authority"
                    | "project-agent-governance"
            )
        }) {
            return Err(denied("selected source owns active NOW, placement or authority; use its dedicated owner transition rather than relocating a single file"));
        }
        let from = relative(&reading.source.path)?;
        let to_raw = text(selection, "to")?;
        let to = relative(to_raw)?;
        let human = from.starts_with(format!("{}/user", scope.prefix))
            || from.starts_with(format!("{}/now/user", scope.prefix))
            || reading
                .source
                .roles
                .iter()
                .any(|r| r.contains("human") || r == "human-day")
            || matches!(
                reading.source.provenance.as_str(),
                "human-authored" | "human-adopted"
            );
        let destination_prefix = if human {
            format!("{}/user", scope.prefix)
        } else {
            format!("{}/agents/now", scope.prefix)
        };
        if !to.starts_with(&destination_prefix) || to == Path::new(&destination_prefix) {
            return Err(denied(
                "destination does not preserve the selected source's human/agent aperture",
            ));
        }
        if from == to
            || !refs.insert(reading.source.source_ref.clone())
            || !paths.insert(from.clone())
            || !paths.insert(to.clone())
        {
            return Err(invalid("migration aliases, cycles and duplicate selections are not implicit merge operations"));
        }
        if optional(scope, to_raw)?.is_some() {
            return Err(conflict(
                "migration destination already exists; no overwrite or automatic merge",
            ));
        }
        placement::anchor(&scope.central_root, &scope.root.join(&to))?;
        let file = crate::file_mutation::open_native_file(&scope.root, &reading.source.path)?;
        let metadata = file.metadata()?;
        if metadata.nlink() != 1 {
            return Err(denied(
                "selected source has existing hard-link aliases requiring separate review",
            ));
        }
        let entries = relations["relations"]
            .as_array_mut()
            .ok_or_else(|| invalid("invalid source relations"))?;
        if entries
            .iter()
            .any(|entry| entry["path"] == to_raw && entry["ref"] != reading.source.source_ref)
        {
            return Err(conflict(
                "destination is already another SourceRef's identity",
            ));
        }
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry["ref"] == reading.source.source_ref)
        {
            entry["path"] = json!(to_raw);
        } else {
            entries.push(json!({"ref":reading.source.source_ref,"path":to_raw,"roles":reading.source.roles,"provenance":reading.source.provenance,"standing":reading.source.standing,"treatment":reading.source.treatment,"recognition":"retained-source-identity-not-new-governance-adoption","recorded_at_unix_seconds":now}));
        }
        moves.push(Move {
            source_ref: reading.source.source_ref,
            from: reading.source.path,
            to: to_raw.into(),
            revision: reading.revision.revision,
            device: metadata.dev(),
            inode: metadata.ino(),
        });
    }
    let mut changes = vec![MetadataChange {
        path: scope.relations_path.clone(),
        before: relations_before,
        after: encoded(&relations)?,
    }];
    // Flow transcript identity/history stays in the existing registry. Unknown
    // metadata is preserved, and the entire original registry is CAS-pinned.
    if let Some(raw) = optional(scope, ".central/flows.json")? {
        let mut registry: Value = serde_json::from_str(&raw)?;
        let flows = registry["flows"]
            .as_array_mut()
            .ok_or_else(|| invalid("existing Flow registry has no flows array"))?;
        let mut changed = false;
        for flow in flows {
            if let Some(step) = moves
                .iter()
                .find(|step| flow["source_ref"] == step.source_ref)
            {
                if flow["path"] != step.from {
                    return Err(conflict(
                        "Flow registry and source relation disagree before migration",
                    ));
                }
                flow["path"] = json!(step.to);
                changed = true;
            }
        }
        if changed {
            changes.push(MetadataChange {
                path: ".central/flows.json".into(),
                before: Some(raw),
                after: encoded(&registry)?,
            });
        }
    }
    let plan = Plan {
        plan_ref,
        request_key,
        scope_ref: scope.world_ref.clone(),
        policy_revision: policy.revision,
        actor_ref: principal.principal_ref.clone(),
        moves,
        metadata: changes,
    };
    let journal = Journal {
        schema: "central.temporal-migration/v1".into(),
        plan_revision: source::revision(&encoded(&plan)?),
        plan,
        phase: "planned".into(),
        created_at_unix_seconds: now,
        updated_at_unix_seconds: now,
    };
    save(scope, &journal)?;
    report(&journal)
}
fn matches_file(scope: &Scope, path: &str, step: &Move) -> io::Result<bool> {
    let Some(content) = optional(scope, path)? else {
        return Ok(false);
    };
    let file = crate::file_mutation::open_native_file(&scope.root, path)?;
    let metadata = file.metadata()?;
    if source::revision(&content) != step.revision
        || metadata.dev() != step.device
        || metadata.ino() != step.inode
    {
        return Err(conflict(format!("migration source bytes or inode changed at {path}; retain both sides for explicit reconciliation")));
    }
    Ok(true)
}
fn remove_at(parent: &File, name: &str) -> io::Result<()> {
    let name = CString::new(name).map_err(io::Error::other)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    parent.sync_all()
}
fn transfer(scope: &Scope, step: &Move, reverse: bool) -> io::Result<()> {
    let (from, to) = if reverse {
        (&step.to, &step.from)
    } else {
        (&step.from, &step.to)
    };
    let source_exists = matches_file(scope, from, step)?;
    let target_exists = matches_file(scope, to, step)?;
    if !source_exists && target_exists {
        return Ok(());
    }
    if !source_exists {
        return Err(conflict(
            "migration source and destination are both absent; no success inferred",
        ));
    }
    let from_path = relative(from)?;
    let to_path = relative(to)?;
    let source_parent = crate::file_mutation::directory(
        &scope.root,
        from_path
            .parent()
            .ok_or_else(|| invalid("source parent missing"))?,
    )?;
    let target_parent = source::directories(
        &scope.root,
        to_path
            .parent()
            .ok_or_else(|| invalid("destination parent missing"))?,
    )?;
    let source_name = from_path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("UTF-8 source filename required"))?;
    let target_name = to_path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("UTF-8 destination filename required"))?;
    if !target_exists {
        let source_name = CString::new(source_name).map_err(io::Error::other)?;
        let target_name = CString::new(target_name).map_err(io::Error::other)?;
        if unsafe {
            libc::linkat(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                target_parent.as_raw_fd(),
                target_name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        target_parent.sync_all()?;
    }
    // The two-name state is intentional recoverable intent. Never unlink a
    // substitute inode or newer human revision following a partially done move.
    matches_file(scope, from, step)?;
    matches_file(scope, to, step)?;
    let held_source = source_parent.metadata()?;
    let held_target = target_parent.metadata()?;
    let live_source =
        crate::file_mutation::directory(&scope.root, from_path.parent().unwrap())?.metadata()?;
    let live_target =
        crate::file_mutation::directory(&scope.root, to_path.parent().unwrap())?.metadata()?;
    if held_source.ino() != live_source.ino()
        || held_source.dev() != live_source.dev()
        || held_target.ino() != live_target.ino()
        || held_target.dev() != live_target.dev()
    {
        return Err(conflict(
            "migration parent changed before unlink; both names retained",
        ));
    }
    remove_at(&source_parent, source_name)
}
fn metadata_change(scope: &Scope, change: &MetadataChange, reverse: bool) -> io::Result<()> {
    let current = optional(scope, &change.path)?;
    let (from, to) = if reverse {
        (Some(change.after.clone()), change.before.clone())
    } else {
        (change.before.clone(), Some(change.after.clone()))
    };
    if current == to {
        return Ok(());
    }
    if current != from {
        return Err(conflict(format!(
            "{} changed outside this migration; metadata was not overwritten",
            change.path
        )));
    }
    match (current, to) {
        (None, Some(next)) => {
            source::put_new(&scope.root, &change.path, &next)?;
        }
        (Some(previous), Some(next)) => crate::source_safety::replace(
            &scope.root,
            &change.path,
            &source::revision(&previous),
            &next,
        )?,
        (Some(previous), None) => {
            let path = crate::projectcentral_flow::relative_member(&change.path)?;
            let parent = crate::file_mutation::directory(
                &scope.root,
                path.parent()
                    .ok_or_else(|| invalid("metadata parent missing"))?,
            )?;
            if crate::source_safety::read(&scope.root, &change.path)? != previous {
                return Err(conflict("metadata changed before rollback removal"));
            }
            remove_at(
                &parent,
                path.file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| invalid("metadata name invalid"))?,
            )?;
        }
        (None, None) => {}
    }
    Ok(())
}
pub fn transition(
    scope: &Scope,
    operation: &str,
    input: &Value,
    principal: &Principal,
    now: u64,
) -> io::Result<Value> {
    principal.require_human()?;
    let policy = placement::checked_policy(scope, input, now)?;
    let mut journal = load(scope, text(input, "plan_ref")?)?;
    if journal.plan_revision != text(input, "expected_plan_revision")? {
        return Err(conflict("immutable migration plan revision changed"));
    }
    if journal.plan.policy_revision != policy.revision {
        return Err(conflict(
            "migration placement law changed; no automatic authority widening",
        ));
    }
    let reverse = match operation {
        "migration_apply"
            if matches!(journal.phase.as_str(), "planned" | "applying" | "applied") =>
        {
            false
        }
        "migration_rollback" => true,
        "migration_recover" => match journal.phase.as_str() {
            "rolling-back" | "rolled-back" => true,
            "planned" | "applying" | "applied" => false,
            _ => return Err(invalid("unknown recovery intent")),
        },
        _ => return Err(conflict("migration phase does not permit this transition")),
    };
    // Before any new physical effect, refuse unrelated metadata drift. A
    // partially committed before/after pair remains independently recoverable.
    for change in &journal.plan.metadata {
        let current = optional(scope, &change.path)?;
        if current != change.before && current.as_deref() != Some(change.after.as_str()) {
            return Err(conflict(
                "migration metadata has concurrent edits; inspect/reconcile instead of overwriting",
            ));
        }
    }
    journal.phase = if reverse { "rolling-back" } else { "applying" }.into();
    journal.updated_at_unix_seconds = now;
    save(scope, &journal)?;
    let result = (|| {
        if reverse {
            for step in journal.plan.moves.iter().rev() {
                transfer(scope, step, true)?;
            }
            for change in journal.plan.metadata.iter().rev() {
                metadata_change(scope, change, true)?;
            }
        } else {
            for step in &journal.plan.moves {
                transfer(scope, step, false)?;
            }
            for change in &journal.plan.metadata {
                metadata_change(scope, change, false)?;
            }
        }
        Ok::<(), io::Error>(())
    })();
    if let Err(error) = result {
        return Err(io::Error::new(
            error.kind(),
            format!(
                "{error}; recovery intent retained at {} (phase {})",
                journal.plan.plan_ref, journal.phase
            ),
        ));
    }
    scope.reconcile(
        Some((&principal.principal_ref, &principal.actor_kind, None)),
        &journal
            .plan
            .moves
            .iter()
            .map(|step| step.source_ref.clone())
            .collect::<Vec<_>>(),
    )?;
    journal.phase = if reverse { "rolled-back" } else { "applied" }.into();
    save(scope, &journal)?;
    report(&journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn principal(scope: &Scope) -> Principal {
        let path = "Control/user/test-authority.json";
        let token = "migration-human-test-credential-only-0001";
        let value = json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{"principal_ref":"human:test","actor_kind":"human","token_sha256":source::key(token),"scope_refs":[scope.world_ref],"actions":["central.migration.plan"],"expires_at_unix_seconds":9999}]});
        fs::write(scope.root.join(path), encoded(&value).unwrap()).unwrap();
        let (mut relations, basis) = scope.relations().unwrap();
        relations["relations"].as_array_mut().unwrap().push(json!({"ref":scope.source_ref(path),"path":path,"roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user","recognition":"controlled-fixture","recorded_at_unix_seconds":1}));
        super::super::temporal::save_relations(scope, &relations, &basis).unwrap();
        super::super::authority::authenticate(
            scope,
            Some(token),
            "central.migration.plan",
            None,
            100,
        )
        .unwrap()
    }
    fn setup() -> (crate::TempDir, Scope, Principal, Value, String) {
        let world = super::super::tests::world();
        let scope = Scope::resolve(world.path(), None).unwrap();
        let actor = principal(&scope);
        let path = "Control/user/old-day.md";
        fs::write(
            scope.root.join(path),
            "Human bytes\r\n- [ ] Still open\r\nλ  \r\n",
        )
        .unwrap();
        let binding = crate::source_horizon::SourceBinding {
            source_ref: scope.source_ref(path),
            path: path.into(),
            roles: vec!["human-day".into()],
            provenance: "human-authored".into(),
            standing: "authored-human-position".into(),
            treatment: "projectcentral-user".into(),
            agent_retrieval_allowed: true,
        };
        scope.bind(&binding, 100).unwrap();
        let revision = scope.read(&binding.source_ref).unwrap().revision.revision;
        let policy = placement::effective_policy(&scope, 100).unwrap();
        let request = json!({"request_id":"move:one","expected_policy_revision":policy.revision,"moves":[{"source_ref":binding.source_ref,"expected_revision":revision,"to":"Control/user/day/retained/day.md"}]});
        (world, scope, actor, request, binding.source_ref)
    }
    fn request(plan: &Value, policy: &Value) -> Value {
        json!({"plan_ref":plan["plan_ref"],"expected_plan_revision":plan["plan_revision"],"expected_policy_revision":policy["expected_policy_revision"]})
    }
    #[test]
    fn apply_replay_and_rollback_preserve_bytes_source_identity_and_dirty_repository() {
        let (_world, scope, actor, input, reference) = setup();
        fs::create_dir_all(scope.root.join("Work/one/.git")).unwrap();
        fs::write(scope.root.join("Work/one/.git/index"), [0, 1, 2, 3]).unwrap();
        fs::write(
            scope.root.join("Work/one/untracked.patch"),
            "dirty working bytes",
        )
        .unwrap();
        let original = scope.read(&reference).unwrap();
        let planned = plan(&scope, &input, &actor, 100).unwrap();
        assert!(scope.root.join(&original.source.path).exists());
        let apply = request(&planned, &input);
        assert_eq!(
            transition(&scope, "migration_apply", &apply, &actor, 101).unwrap()["phase"],
            "applied"
        );
        assert_eq!(scope.read(&reference).unwrap().content, original.content);
        assert_eq!(
            scope.read(&reference).unwrap().source.path,
            "Control/user/day/retained/day.md"
        );
        transition(&scope, "migration_apply", &apply, &actor, 102).unwrap();
        transition(&scope, "migration_rollback", &apply, &actor, 103).unwrap();
        assert_eq!(
            scope.read(&reference).unwrap().source.path,
            original.source.path
        );
        assert_eq!(scope.read(&reference).unwrap().content, original.content);
        assert_eq!(
            fs::read(scope.root.join("Work/one/.git/index")).unwrap(),
            [0, 1, 2, 3]
        );
        assert_eq!(
            fs::read_to_string(scope.root.join("Work/one/untracked.patch")).unwrap(),
            "dirty working bytes"
        );
    }
    #[test]
    fn interrupted_link_before_unlink_recovers_and_replays() {
        let (_world, scope, actor, input, reference) = setup();
        let planned = plan(&scope, &input, &actor, 100).unwrap();
        let mut journal = load(&scope, planned["plan_ref"].as_str().unwrap()).unwrap();
        let step = &journal.plan.moves[0];
        fs::create_dir_all(scope.root.join(&step.to).parent().unwrap()).unwrap();
        fs::hard_link(scope.root.join(&step.from), scope.root.join(&step.to)).unwrap();
        journal.phase = "applying".into();
        save(&scope, &journal).unwrap();
        transition(
            &scope,
            "migration_recover",
            &request(&planned, &input),
            &actor,
            101,
        )
        .unwrap();
        assert!(!scope.root.join("Control/user/old-day.md").exists());
        assert!(scope
            .read(&reference)
            .unwrap()
            .content
            .contains("Still open"));
        transition(
            &scope,
            "migration_recover",
            &request(&planned, &input),
            &actor,
            102,
        )
        .unwrap();
    }
    #[test]
    fn stale_selected_source_or_new_human_bytes_refuse_without_overwrite() {
        let (_world, scope, actor, input, reference) = setup();
        let planned = plan(&scope, &input, &actor, 100).unwrap();
        let apply = request(&planned, &input);
        fs::write(
            scope.root.join("Control/user/old-day.md"),
            "new human source",
        )
        .unwrap();
        assert_eq!(
            transition(&scope, "migration_apply", &apply, &actor, 101)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(scope.read(&reference).unwrap().content, "new human source");
        assert!(!scope.root.join("Control/user/day/retained/day.md").exists());
    }
    #[test]
    fn rollback_does_not_overwrite_post_migration_human_edit() {
        let (_world, scope, actor, input, reference) = setup();
        let planned = plan(&scope, &input, &actor, 100).unwrap();
        let apply = request(&planned, &input);
        transition(&scope, "migration_apply", &apply, &actor, 101).unwrap();
        fs::write(
            scope.root.join("Control/user/day/retained/day.md"),
            "human continued after move",
        )
        .unwrap();
        assert_eq!(
            transition(&scope, "migration_rollback", &apply, &actor, 102)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            scope.read(&reference).unwrap().content,
            "human continued after move"
        );
        assert!(!scope.root.join("Control/user/old-day.md").exists());
    }
}
