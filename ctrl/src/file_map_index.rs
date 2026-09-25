//! Incremental native bkmr maintenance and scoped query federation.
use super::file_map::*;
use crate::file_map_backend::{self as native, Backend};
use crate::source_safety::content_revision_bytes;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
pub(crate) fn refresh(scope: &Scope, embeddings: bool) -> io::Result<Value> {
    let backend = Backend::new(&scope.root);
    backend.prepare()?;
    let mut index = scope.index()?;
    let mut records = backend.records()?;
    let entries = entries(scope)?;
    let allowed: BTreeSet<_> = entries
        .iter()
        .map(|e| e.source.source_ref.as_str())
        .collect();
    let mut changes = Vec::new();
    let mut diagnostics = Vec::new();
    // A changed embedder poisons every stored vector. Clear before the entry
    // pass so rows added below embed under the configured model, then fill
    // everything the clear removed once the pass is done. Backfill can
    // re-embed thousands of rows, so it runs with its own long allowance.
    let model = native::embedding_model();
    let model_changed =
        embeddings && index.embedding_model.as_deref() != Some(model.as_str());
    if model_changed {
        backend.run(&["clear-embeddings".into()])?;
    }
    // Withdraw only our rows. Unknown/user bookmarks are never deleted.
    for (reference, old) in index.entries.clone() {
        if !allowed.contains(reference.as_str()) {
            for row_id in [Some(old.id), old.import_id].into_iter().flatten() {
                if let Some(row) = records.iter().find(|r| native::id(r).ok() == Some(row_id)) {
                    if native::record(row)["description"]
                        .as_str()
                        .is_some_and(|d| d.starts_with(&marker(&reference)))
                    {
                        backend.run(&["delete".into(), row_id.to_string()])?;
                    } else {
                        diagnostics
                            .push(format!("Unowned or changed row {row_id} was not removed"));
                    }
                }
            }
            index.entries.remove(&reference);
            changes.push(format!("withdrawn:{reference}"));
        }
    }
    let mut added_without_embed = 0usize;
    let mut checkpointed = 0usize;
    for entry in entries {
        let entry = with_revision(entry)?;
        let reference = &entry.source.source_ref;
        let url = uri(&entry.path);
        let mut generated = description(&entry);
        let existing = index.entries.get(reference).cloned();
        if let Some(text) = existing
            .as_ref()
            .and_then(|v| v.retained_description.as_ref())
        {
            generated.push_str(&format!("\n\nRetained bookmark description:\n{text}"));
        }
        let row = existing
            .as_ref()
            .and_then(|b| records.iter().find(|r| native::id(r).ok() == Some(b.id)))
            .cloned();
        let record_id;
        if let (Some(old), Some(row)) = (&existing, &row) {
            let row = native::record(row);
            if row["url"] != old.url
                || !row["description"]
                    .as_str()
                    .is_some_and(|d| d.starts_with(&marker(reference)))
            {
                return Err(conflict(
                    "Managed bkmr row changed identity; explicit reconciliation is required",
                ));
            }
            record_id = old.id;
            if old.revision != entry.revision || old.url != url || index.embeddings != embeddings {
                let current_desc = row["description"].as_str().unwrap();
                // Indexes from before the digest era stored the whole text;
                // migrate them by trusting the marker-verified row once.
                let stored_hash = if old.description_hash.is_empty() {
                    content_revision_bytes(current_desc.as_bytes())
                } else {
                    old.description_hash.clone()
                };
                if content_revision_bytes(current_desc.as_bytes()) != stored_hash {
                    return Err(conflict(
                        "Managed indexed description was edited in bkmr; preserve/reconcile it before refresh",
                    ));
                }
                let mut args = vec![
                    "update".into(),
                    record_id.to_string(),
                    "--url".into(),
                    url.clone(),
                    "--description".into(),
                    generated.clone(),
                ];
                if row["title"] == old.generated_title {
                    args.extend(["--title".into(), entry.title.clone()]);
                }
                args.push(if embeddings { "--embed" } else { "--no-embed" }.into());
                backend.run(&args)?;
                changes.push(format!("updated:{reference}"));
            }
        } else {
            // Attach a retained marker after interruption; never adopt an unrelated
            // bookmark just because its URL happens to match.
            let matches: Vec<_> = records
                .iter()
                .filter(|r| {
                    native::record(r)["url"] == url
                        && native::record(r)["description"]
                            .as_str()
                            .is_some_and(|d| d.starts_with(&marker(reference)))
                })
                .collect();
            if matches.len() > 1 {
                return Err(conflict("Duplicate source marker in bkmr"));
            }
            if let Some(row) = matches.first() {
                record_id = native::id(row)?;
            } else {
                if records.iter().any(|r| native::record(r)["url"] == url) {
                    return Err(conflict(
                        "Existing unowned bookmark for this URI must be explicitly adopted, not overwritten",
                    ));
                }
                let mut args = vec![
                    "add".into(),
                    url.clone(),
                    "--title".into(),
                    entry.title.clone(),
                    "--description".into(),
                    generated.clone(),
                    "--no-web".into(),
                    // Embedding happens once, in bulk, after the pass: the
                    // per-row path would spawn the embedder for every file.
                    "--no-embed".into(),
                ];
                if !entry.tags.is_empty() {
                    args.push(entry.tags.join(","));
                }
                backend.run(&args)?;
                added_without_embed += 1;
                // The new row's identity comes from a bounded title search,
                // not a full listing: re-listing every row after every add
                // made pooled refreshes quadratic. The title is the source's
                // path; the exact-URL match confirms identity, and the full
                // listing stays as the fallback when full-text misbehaves.
                let listing = backend
                    .search(entry.title.as_str(), &[], false, 8)
                    .or_else(|_| backend.records())?;
                record_id = native::id(
                    listing
                        .iter()
                        .find(|r| native::record(r)["url"] == url)
                        .ok_or_else(|| io::Error::other("bkmr add produced no URI record"))?,
                )?;
                changes.push(format!("added:{reference}"));
            }
        }
        let mut import_id = existing.as_ref().and_then(|v| v.import_id);
        if existing
            .as_ref()
            .is_some_and(|old| old.source_path != entry.source.path)
        {
            if let Some(id) = import_id {
                if !records.iter().any(|row| {
                    native::id(row).ok() == Some(id)
                        && native::record(row)["description"]
                            .as_str()
                            .is_some_and(|d| d.starts_with(&marker(reference)))
                }) {
                    return Err(conflict(
                        "Native imported bookmark changed owner binding; relocation does not delete it",
                    ));
                }
                backend.run(&["delete".into(), id.to_string()])?;
                import_id = None;
                records = backend.records()?;
            }
        }
        if entry.native_import
            && entry.kind == "file"
            && existing
                .as_ref()
                .is_none_or(|b| b.revision != entry.revision || import_id.is_none())
        {
            let before: BTreeSet<_> = records.iter().filter_map(|r| native::id(r).ok()).collect();
            let mut args = vec!["import-files".into(), "--update".into()];
            if !embeddings {
                args.push("--no-embed".into());
            }
            if let Ok(relative) = entry.path.strip_prefix(&scope.root) {
                args.extend([
                    "--base-path".into(),
                    "WORLD".into(),
                    "--".into(),
                    relative.to_string_lossy().into(),
                ]);
            } else {
                args.extend(["--".into(), entry.path.to_string_lossy().into()]);
            }
            backend.run(&args)?;
            records = backend.records()?;
            let created: Vec<_> = records
                .iter()
                .filter_map(|r| native::id(r).ok())
                .filter(|id| !before.contains(id))
                .collect();
            if import_id.is_none() && created.len() == 1 {
                import_id = Some(created[0]);
            }
            if let Some(id) = import_id {
                backend.run(&[
                    "update".into(),
                    id.to_string(),
                    "--description".into(),
                    marker(reference),
                ])?;
            } else {
                diagnostics.push(format!("{reference}: native importer produced no distinct tracked row; URI identity and content index retained"));
            }
        }
        index.entries.insert(
            reference.clone(),
            Indexed {
                source_path: entry.source.path.clone(),
                id: record_id,
                revision: entry.revision,
                url,
                generated_title: entry.title,
                description_hash: content_revision_bytes(generated.as_bytes()),
                retained_description: existing
                    .as_ref()
                    .and_then(|v| v.retained_description.clone()),
                import_id,
            },
        );
        // Checkpoint in batches: a resumed refresh replays at most this many
        // bkmr writes, while per-record saves would rewrite a pooled scope's
        // whole index for every file.
        checkpointed += 1;
        if checkpointed >= 256 {
            scope.save_index(&index)?;
            checkpointed = 0;
        }
    }
    // Rows the clear removed and rows added this pass all lack vectors;
    // backfill regenerates them in one bulk pass under the configured model.
    if embeddings && (model_changed || added_without_embed > 0) {
        backend.run_allowance(&["backfill".into()], 3600)?;
    }
    if embeddings {
        index.embedding_model = Some(model);
    }
    index.embeddings = embeddings;
    scope.save_index(&index)?;
    Ok(
        json!({"world_ref":scope.world,"database":backend.db(),"changes":changes,"diagnostics":diagnostics,"entries":index.entries.len(),"embeddings":embeddings}),
    )
}
pub(crate) fn search(all: &[Scope], input: &Value) -> io::Result<Value> {
    let scope = selected(all, input)?;
    let mut chosen: Vec<_> = if input["federated"] == true {
        all.iter().collect()
    } else {
        vec![scope]
    };
    let linked = if input["federated"] == true {
        Vec::new()
    } else {
        verified_links(all, scope)?
    };
    let linked_refs: BTreeSet<_> = linked
        .iter()
        .map(|(_, e)| e.source.source_ref.clone())
        .collect();
    for (_, entry) in &linked {
        if let Some(owner) = all.iter().find(|s| s.world == entry.world_ref) {
            if !chosen.iter().any(|s| s.world == owner.world) {
                chosen.push(owner);
            }
        }
    }
    let selected_world = scope.world.clone();
    let raw_query = input["query"].as_str().unwrap_or("");
    let mode = input["mode"].as_str().unwrap_or("fulltext");
    if !matches!(mode, "fulltext" | "hybrid") {
        return Err(invalid(
            "Supported structured modes: fulltext, hybrid; sem-search has no JSON contract",
        ));
    }
    let tags: Vec<String> = serde_json::from_value(input.get("tags").cloned().unwrap_or(json!([])))
        .map_err(io::Error::other)?;
    let limit = input["limit"].as_u64().unwrap_or(50).min(1000) as usize;
    let allow: Option<BTreeSet<String>> = input
        .get("allowed_sources")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(io::Error::other)?;
    let excluded = context_exclusions(all, selected(all, input)?)?;
    if limit == 0 {
        return Ok(json!({"hits":Vec::<Value>::new(),"absences":Vec::<Value>::new()}));
    }
    // Full-text matching requires every token to hit; a long phrase that
    // misses everywhere relaxes by dropping trailing tokens (to a floor of
    // one) instead of answering nothing. The relaxed query is disclosed.
    let tokens: Vec<&str> = raw_query.split_whitespace().collect();
    let attempts = tokens.len().clamp(1, 4);
    let mut hits = Vec::new();
    let mut absences = Vec::new();
    let mut seen = BTreeSet::new();
    let mut effective = raw_query.to_owned();
    let mut relaxed = false;
    for attempt in 0..attempts {
        effective = tokens[..tokens.len() - attempt].join(" ");
        relaxed = attempt > 0;
        hits.clear();
        seen.clear();
        absences.clear();
        let query = effective.as_str();
        for scope in chosen.iter().copied() {
        let index = scope.index()?;
        let backend = Backend::new(&scope.root);
        if !backend.present() {
            absences.push(format!("{}: map not initialized", scope.world));
            continue;
        }
        if mode == "hybrid" && !index.embeddings {
            absences.push(format!("{}: embeddings not ready", scope.world));
            continue;
        }
        // Vectors from another embedder poison hybrid results without any
        // revision change, so the mismatch itself is the absence the refresh
        // routine reacts to.
        if mode == "hybrid" && index.embedding_model.as_deref() != Some(native::embedding_model().as_str())
        {
            absences.push(format!(
                "{}: embeddings stored under another model; refresh required",
                scope.world
            ));
            continue;
        }
        let current: BTreeMap<_, _> = match entries(scope) {
            Ok(items) => items
                .into_iter()
                .map(|e| (e.source.source_ref.clone(), e))
                .collect(),
            // One scope's broken enumeration must not take the whole
            // federated query down: name it in the absences and move on.
            Err(error) => {
                absences.push(format!("{}: source enumeration failed: {}", scope.world, error));
                continue;
            }
        };
        // Apply live authority and revisions BEFORE returning any cached snippet.
        for (rank, value) in backend
            .search(query, &tags, mode == "hybrid", MAX_ENTRIES)?
            .into_iter()
            .enumerate()
        {
            let id = native::id(&value)?;
            let Some((reference, binding)) = index
                .entries
                .iter()
                .find(|(_, b)| b.id == id || b.import_id == Some(id))
            else {
                continue;
            };
            if input["federated"] != true
                && scope.world != selected_world
                && !linked_refs.contains(reference)
            {
                continue;
            }
            if !policy_allows(&excluded, reference) {
                continue;
            }
            if allow.as_ref().is_some_and(|a| !a.contains(reference)) {
                continue;
            }
            let Some(entry) = current.get(reference) else {
                continue;
            };
            let entry = with_revision(entry.clone())?;
            if entry.revision != binding.revision {
                absences.push(format!("{reference}: stale index; refresh required"));
                continue;
            }
            if !native::record(&value)["description"]
                .as_str()
                .is_some_and(|d| d.starts_with(&marker(reference)))
            {
                return Err(conflict("Search row's source binding drifted"));
            }
            if !seen.insert(reference.clone()) {
                continue;
            }
            hits.push(json!({"source":entry.source,"world_ref":scope.world,"project":scope.project,"path":entry.path,"kind":entry.kind,"revision":entry.revision,"title":native::record(&value)["title"],"tags":native::tags(&value)?,"snippet":content(&entry).unwrap_or_default().chars().take(1000).collect::<String>(),"provider_binding":id.to_string(),"score":value["rrf_score"].as_f64().unwrap_or(1.0/(rank+1) as f64),"mode":mode}));
        }
    }
        if !hits.is_empty() {
            break;
        }
    }
    let current_exclusions = context_exclusions(all, selected(all, input)?)?;
    hits.retain(|hit| {
        hit["source"]["ref"]
            .as_str()
            .is_some_and(|reference| policy_allows(&current_exclusions, reference))
    });
    hits.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&a["score"].as_f64().unwrap_or_default())
            .then_with(|| {
                a["source"]["ref"]
                    .as_str()
                    .cmp(&b["source"]["ref"].as_str())
            })
    });
    hits.truncate(limit);
    Ok(json!({"hits":hits,"absences":absences,"query":effective,"query_relaxed":relaxed}))
}
