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
    let backend = Backend::for_scope(scope);
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
    // Sources whose text the description holds only the start of. Named, not
    // counted away: this is the bound `INDEXED_TEXT_BYTES` puts on search.
    let mut truncated = Vec::new();
    // A changed embedder poisons every stored vector. Clear before the entry
    // pass so rows added below embed under the configured model, then fill
    // everything the clear removed once the pass is done. Backfill can
    // re-embed thousands of rows, so it runs with its own long allowance.
    let model = native::embedding_model();
    let model_changed = embeddings && index.embedding_model.as_deref() != Some(model.as_str());
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
        let (mut generated, extent) = describe(&entry);
        if extent.total.is_some_and(|total| total > extent.indexed) {
            truncated.push(json!({
                "source_ref": reference,
                "indexed_bytes": extent.indexed,
                "total_bytes": extent.total,
            }));
        }
        if extent.gap.is_some() && !extent.is_not_text() {
            diagnostics.push(format!(
                "{reference}: no text indexed: {}",
                extent.gap.as_deref().unwrap_or_default()
            ));
        }
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
            // The URI row may have been added above. Observe the actual native
            // provider immediately before import so it is not misclassified as
            // a second newly imported row.
            records = backend.records()?;
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
        json!({"world_ref":scope.world,"database":backend.db(),"changes":changes,"diagnostics":diagnostics,"truncated":truncated,"entries":index.entries.len(),"embeddings":embeddings}),
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
    let limit = input["limit"].as_u64().unwrap_or(50) as usize;
    let snippet_chars = input["snippet_chars"].as_u64().unwrap_or(1000) as usize;
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
            let backend = Backend::for_scope(scope);
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
            if mode == "hybrid"
                && index.embedding_model.as_deref() != Some(native::embedding_model().as_str())
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
                    absences.push(format!(
                        "{}: source enumeration failed: {}",
                        scope.world, error
                    ));
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
                // The snippet is the passage that matched, from the live text
                // the index was built from, with where it sits in the source.
                let (snippet, span, matched, indexed, total) =
                    match text_prefix(&entry, INDEXED_TEXT_BYTES) {
                        Ok((text, total)) => {
                            let found = passage(&text, query, snippet_chars);
                            (
                                found.text,
                                json!({"unit":"text_span","start":found.start,"end":found.end}),
                                found.matched,
                                text.len() as u64,
                                Some(total),
                            )
                        }
                        Err(error) => {
                            absences.push(format!("{reference}: no snippet: {error}"));
                            (String::new(), Value::Null, false, 0, None)
                        }
                    };
                hits.push(json!({"source":entry.source,"world_ref":scope.world,"project":scope.project,"path":entry.path,"kind":entry.kind,"revision":entry.revision,"title":native::record(&value)["title"],"tags":native::tags(&value)?,"snippet":snippet,"span":span,"matched":matched,"indexed_bytes":indexed,"total_bytes":total,"provider_binding":id.to_string(),"score":value["rrf_score"].as_f64().unwrap_or(1.0/(rank+1) as f64),"mode":mode}));
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

/// The passage of a source's text that best answers a query, and where it sits
/// (character offsets, the unit of a `text_span` selector).
pub(crate) struct Passage {
    pub text: String,
    pub start: usize,
    pub end: usize,
    /// Whether any query term was found. A hit the meaning arm returned for a
    /// query whose words are not in the text gets the start of the source.
    pub matched: bool,
}

/// The window of about `width` characters holding the most distinct query terms
/// (then the most occurrences), earliest on a tie, widened to word edges. The
/// text is the indexed start of a source, so scanning it whole is cheap.
pub(crate) fn passage(text: &str, query: &str, width: usize) -> Passage {
    let width = width.max(1);
    let mut terms: Vec<String> = Vec::new();
    for raw in query.split_whitespace() {
        let term = raw
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    // Byte offset of every character, and the character index of every byte.
    let starts: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
        .collect();
    let total = starts.len() - 1;
    let mut char_of = vec![0usize; text.len() + 1];
    for (index, window) in starts.windows(2).enumerate() {
        char_of[window[0]..window[1]].fill(index);
    }
    char_of[text.len()] = total;
    // Lowercasing can change a character's byte length; keep the way back.
    let mut lower = String::with_capacity(text.len());
    let mut origin: Vec<usize> = Vec::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        for folded in c.to_lowercase() {
            let before = lower.len();
            lower.push(folded);
            origin.extend(std::iter::repeat_n(at, lower.len() - before));
        }
    }
    let mut found: Vec<(usize, usize)> = Vec::new();
    for (which, term) in terms.iter().enumerate() {
        for (at, _) in lower.match_indices(term.as_str()) {
            found.push((char_of[origin[at]], which));
        }
    }
    found.sort_unstable();
    let slice = |from: usize, to: usize| Passage {
        text: text[starts[from]..starts[to]].to_owned(),
        start: from,
        end: to,
        matched: false,
    };
    if found.is_empty() {
        return slice(0, total.min(width));
    }
    // Sweep a window of `width` characters over the occurrences.
    let mut counts = vec![0usize; terms.len()];
    let mut distinct = 0usize;
    let mut tail = 0usize;
    let mut best = (0usize, 0usize, 0usize); // (distinct, occurrences, first position)
    for head in 0..found.len() {
        let (at, which) = found[head];
        counts[which] += 1;
        if counts[which] == 1 {
            distinct += 1;
        }
        while found[tail].0 + width <= at {
            let (_, gone) = found[tail];
            counts[gone] -= 1;
            if counts[gone] == 0 {
                distinct -= 1;
            }
            tail += 1;
        }
        let occurrences = head + 1 - tail;
        if (distinct, occurrences) > (best.0, best.1) {
            best = (distinct, occurrences, found[tail].0);
        }
    }
    let first = best.2;
    let mut from = first.saturating_sub(width / 8);
    let mut to = (from + width).min(total);
    from = to.saturating_sub(width).min(from);
    // Begin and end on word edges where one is near.
    let edge = |c: char| c.is_whitespace();
    for _ in 0..60 {
        if from == 0 || text[starts[from - 1]..starts[from]].chars().all(edge) {
            break;
        }
        from -= 1;
    }
    for _ in 0..60 {
        if to >= total || text[starts[to]..starts[to + 1]].chars().all(edge) {
            break;
        }
        to += 1;
    }
    let mut found = slice(from, to);
    found.matched = true;
    found
}

#[cfg(test)]
mod passage_tests {
    use super::passage;

    fn filler(count: usize) -> String {
        "the quick brown fox jumps over a lazy dog. ".repeat(count)
    }

    #[test]
    fn the_passage_is_where_the_terms_are_not_where_the_file_begins() {
        let text = format!(
            "{}needle in the haystack sits here. {}",
            filler(120),
            filler(40)
        );
        let found = passage(&text, "needle haystack", 200);
        assert!(found.matched);
        assert!(
            found.text.contains("needle in the haystack"),
            "{}",
            found.text
        );
        assert!(found.start > 4000, "offset {}", found.start);
        let chars: Vec<char> = text.chars().collect();
        let sliced: String = chars[found.start..found.end].iter().collect();
        assert_eq!(sliced, found.text, "the span is the passage");
    }

    #[test]
    fn a_cluster_of_distinct_terms_beats_one_term_repeated() {
        let text = format!(
            "{}alpha alpha alpha alpha. {}alpha beta gamma together. {}",
            filler(20),
            filler(20),
            filler(5)
        );
        let found = passage(&text, "alpha beta gamma", 160);
        assert!(
            found.text.contains("alpha beta gamma together"),
            "{}",
            found.text
        );
    }

    #[test]
    fn offsets_count_characters_not_bytes() {
        let text = format!(
            "{}ünïcode señal here and more. {}",
            "é".repeat(500),
            filler(10)
        );
        let found = passage(&text, "SEÑAL", 80);
        assert!(found.matched, "case folds across non-ASCII");
        let chars: Vec<char> = text.chars().collect();
        let sliced: String = chars[found.start..found.end].iter().collect();
        assert_eq!(sliced, found.text);
        assert!(found.text.contains("señal"));
    }

    #[test]
    fn a_query_with_no_term_in_the_text_answers_with_its_start_unmatched() {
        let text = filler(60);
        let found = passage(&text, "zebra", 100);
        assert!(!found.matched);
        assert_eq!(found.start, 0);
        assert!(found.end <= 100);
        let empty = passage(&text, "   ", 100);
        assert!(!empty.matched && empty.start == 0);
    }

    #[test]
    fn a_passage_never_runs_past_the_text() {
        let found = passage("short text with needle", "needle", 1000);
        assert_eq!(found.start, 0);
        assert_eq!(found.end, "short text with needle".chars().count());
    }
}
