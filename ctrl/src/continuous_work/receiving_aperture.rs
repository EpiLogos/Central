//! A root reading of the existing scoped receiving ledgers, never another inbox.
//! Acquire one scope at a time in the receiving -> root -> Project lock order.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const CURSOR_SCHEMA: &str = "central.receiving-aperture-cursor/v1";

pub(super) fn read(scope: &Scope, input: &Value, token: Option<&str>, now: u64) -> io::Result<Value> {
    if scope.project.is_some() {
        return Err(invalid("cross-Project receiving is a root reading; omit project"));
    }
    if input.get("after").is_some() {
        return Err(invalid("a root receiving aperture uses cursor, not one scope's after value"));
    }
    let projects = input.get("projects").and_then(Value::as_array)
        .ok_or_else(|| invalid("projects must be an explicit array of existing Work members"))?;
    if projects.len() > 32 { return Err(invalid("at most 32 Projects per receiving aperture")); }
    let limit = match input.get("limit") {
        None => 50,
        Some(value) => value.as_u64().ok_or_else(|| invalid("limit must be an integer"))?,
    };
    if !(1..=200).contains(&limit) { return Err(invalid("receiving page limit must be 1..200")); }
    let mut scopes = vec![scope.clone()];
    let mut names = BTreeSet::new();
    for project in projects {
        let project = project.as_str().ok_or_else(|| invalid("Project selection must contain strings"))?;
        if !names.insert(project) { return Err(invalid("duplicate Project selection")); }
        scopes.push(Scope::resolve(&scope.central_root, Some(project))?);
    }
    let refs: Vec<String> = scopes.iter().map(|s| s.world_ref.clone()).collect();
    if refs.iter().collect::<BTreeSet<_>>().len() != refs.len() {
        return Err(conflict("different Work members claim the same World; reconcile identity first"));
    }
    let mut after: BTreeMap<String, u64> = refs.iter().cloned().map(|r| (r, 0)).collect();
    let mut turn = 0usize;
    if let Some(cursor) = input.get("cursor").filter(|v| !v.is_null()) {
        if cursor["schema"] != CURSOR_SCHEMA || cursor["scope_refs"] != json!(refs) {
            return Err(conflict("receiving cursor belongs to a different explicit scope selection"));
        }
        let values: BTreeMap<String, u64> = serde_json::from_value(cursor["after"].clone())?;
        if values.keys().ne(after.keys()) { return Err(invalid("cursor must name exactly its selected scopes")); }
        after = values;
        turn = cursor["next_scope"].as_u64().filter(|v| *v < scopes.len() as u64)
            .ok_or_else(|| invalid("invalid receiving cursor turn"))? as usize;
    }

    let mut queues: Vec<VecDeque<Value>> = Vec::new();
    let mut more_in_scope = Vec::new();
    let mut authority_basis: Option<String> = None;
    let mut principal_ref: Option<String> = None;
    let mut withheld = 0u64;
    for selected in &scopes {
        let _receiving = crate::source_safety::lock(&selected.root, "source-return.lock")?;
        let _sources = source::lock(selected)?;
        // A root grant does not imply a Project grant. Every requested scope
        // must admit the same credential at the same current source revision.
        let expected = authority_basis.as_deref().or_else(|| input.get("expected_authority_revision").and_then(Value::as_str));
        let principal = authority::authenticate(selected, token, "central.receiving.list", expected, now)?;
        if principal_ref.as_ref().is_some_and(|r| r != &principal.principal_ref) {
            return Err(conflict("receiving principal changed during composition"));
        }
        principal_ref = Some(principal.principal_ref);
        authority_basis = Some(principal.authority_revision);
        let page = super::list(selected, &json!({"after":after[&selected.world_ref],"limit":limit}))?;
        withheld += page["withheld_unavailable_sources"].as_u64().unwrap_or(0);
        more_in_scope.push(page["more"] == true);
        let mut queue = VecDeque::new();
        for mut record in page["returns"].as_array().ok_or_else(|| invalid("native receiving page lacks returns"))?.clone() {
            record["scope_ref"] = json!(selected.world_ref);
            record["project"] = json!(selected.project);
            record["open"] = json!({"action":"central.receiving.read","input":{"project":selected.project,"return_ref":record["return_ref"]}});
            queue.push_back(record);
        }
        queues.push(queue);
    }
    // Round-robin k-way reading preserves each owner's sequence, even when
    // receipt wall clocks move backwards. The cursor prevents a busy root
    // from starving a Project; it never consumes an undisclosed future record.
    let mut records = Vec::new();
    let mut empty_turns = 0;
    while records.len() < limit as usize && empty_turns < scopes.len() {
        let index = turn;
        turn = (turn + 1) % scopes.len();
        if let Some(record) = queues[index].pop_front() {
            after.insert(scopes[index].world_ref.clone(), record["sequence"].as_u64().ok_or_else(|| invalid("native Return sequence missing"))?);
            records.push(record);
            empty_turns = 0;
        } else { empty_turns += 1; }
    }
    // Recheck authority and each disclosed source after collecting scopes.
    // Policy changes cannot quietly produce a partial/cached sharing result.
    for selected in &scopes {
        let _sources = source::lock(selected)?;
        authority::authenticate(selected, token, "central.receiving.list", authority_basis.as_deref(), now)?;
        for record in records.iter().filter(|r| r["scope_ref"] == selected.world_ref) {
            let (current, _) = super::read_record(selected, text(record, "return_ref")?)?;
            super::require_disclosure(selected, &current)?;
        }
    }
    let more = more_in_scope.into_iter().any(|v| v) || queues.iter().any(|q| !q.is_empty());
    Ok(json!({
        "schema":"central.receiving-aperture/v1","scope_ref":scope.world_ref,
        "scope_refs":refs,"returns":records,"more":more,
        "cursor":{"schema":CURSOR_SCHEMA,"scope_refs":refs,"after":after,"next_scope":turn},
        "authority_revision":authority_basis,"principal_ref":principal_ref,
        "withheld_unavailable_sources":withheld,"proposal_bodies_in_page":false,
        "composition":"references-to-owner-ledgers-not-copied-human-prose",
        "consistency":"per-scope-locked-read-with-live-disclosure-revalidation",
        "automatic_agent_or_model_invocation":false,"human_awareness_or_recognition_inferred":false
    }))
}
