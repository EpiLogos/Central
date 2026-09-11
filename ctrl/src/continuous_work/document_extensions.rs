//! Retained documents are material, never native identity or credential grants.
//! All mutations below run inside the existing exact-revision mutation journal.
use super::*;

const OPEN: &str = "<script type=\"application/json\" id=\"central-retained-document\">";

pub(super) fn validate(document: &Value) -> io::Result<()> {
    for collection in ["fields", "entries", "contributions", "notes"] {
        let Some(value) = document.get(collection) else {
            if collection == "notes" { continue; }
            return Err(invalid("document collection missing"));
        };
        let values = value.as_array().ok_or_else(|| invalid("document collection must be an array"))?;
        if values.len() > 4096 { return Err(invalid("document collection exceeds bounded size")); }
        let mut ids = BTreeSet::new();
        for value in values {
            let id = text(value, "id")?;
            if !ids.insert(id) { return Err(invalid("duplicate document-local identity")); }
        }
    }
    for field in document["fields"].as_array().ok_or_else(|| invalid("fields missing"))? {
        if let Some(pointer) = field.get("template_pointer") {
            let pointer = pointer.as_str().ok_or_else(|| invalid("template_pointer must be text"))?;
            if document["template_payload"].pointer(pointer).is_none() {
                return Err(invalid("retained field lost its original payload key"));
            }
        }
    }
    for contribution in document["contributions"].as_array().ok_or_else(|| invalid("contributions missing"))? {
        for (key, collection) in [("entry_id", "entries"), ("field_id", "fields")] {
            if let Some(id) = contribution.get(key).filter(|v| !v.is_null()) {
                if !document[collection].as_array().unwrap().iter().any(|entry| entry["id"] == *id) {
                    return Err(invalid("contribution has a dangling local target"));
                }
            }
        }
    }
    Ok(())
}
fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
fn target(document: &Value, kind: &str, id: &str) -> Option<Value> {
    match kind {
        "contribution" => document["contributions"].as_array()?.iter().find(|v| v["id"] == id).cloned(),
        "field" => {
            let field = document["fields"].as_array()?.iter().find(|v| v["id"] == id)?;
            let value = field["template_pointer"].as_str().and_then(|p| document["template_payload"].pointer(p));
            Some(json!({"field":field,"value":value}))
        }
        "entry" => {
            let entry = document["entries"].as_array()?.iter().find(|v| v["id"] == id)?;
            let content: Vec<_> = document["contributions"].as_array()?.iter().filter(|v| v["entry_id"] == id).collect();
            Some(json!({"entry":entry,"contributions":content}))
        }
        _ => None,
    }
}
pub(super) fn refresh_anchors(document: &mut Value) -> io::Result<()> {
    let Some(notes) = document.get("notes").and_then(Value::as_array).cloned() else { return Ok(()); };
    let mut refreshed = Vec::new();
    for mut note in notes {
        if let Some(anchor) = note.get("anchor").filter(|v| !v.is_null()) {
            let current = target(document, text(anchor, "kind")?, text(anchor, "target_id")?);
            let revision = current.as_ref().map(serde_json::to_string).transpose()?.map(|v| source::key(&v));
            // A stale anchor never silently becomes current again. Its original
            // quote, local target and basis remain available for human review.
            if anchor["status"] != "needs-review" && revision.as_deref() != anchor["basis"].as_str() {
                note["anchor"]["status"] = json!("needs-review");
            }
        }
        refreshed.push(note);
    }
    document["notes"] = json!(refreshed);
    Ok(())
}
fn note(document: &mut Value, input: &Value, author: &ContributionAuthor, now: u64) -> io::Result<()> {
    let id = text(input, "note_id")?;
    if document.get("notes").is_none() { document["notes"] = json!([]); }
    if document["notes"].as_array().unwrap().iter().any(|n| n["id"] == id) { return Err(conflict("NoteId already exists")); }
    let timing = text(input, "timing")?;
    if !matches!(timing, "During" | "After") { return Err(invalid("note timing must be During or After")); }
    let parent = input.get("parent_note_id").filter(|v| !v.is_null());
    if parent.is_some_and(|p| !document["notes"].as_array().unwrap().iter().any(|n| n["id"] == *p)) {
        return Err(invalid("reply note is not in this document"));
    }
    let anchor = if let Some(value) = input.get("anchor").filter(|v| !v.is_null()) {
        let kind = text(value, "kind")?;
        let id = text(value, "target_id")?;
        let basis = target(document, kind, id).ok_or_else(|| invalid("annotation target is not in this document"))?;
        let quote = value.get("original_text").and_then(Value::as_str).ok_or_else(|| invalid("anchor retains original_text, including an empty whole-entry selection"))?;
        if quote.len() > 65536 { return Err(invalid("selected passage exceeds bounded size")); }
        Some(json!({"kind":kind,"target_id":id,"original_text":quote,"basis":source::key(&serde_json::to_string(&basis)?),"status":"current"}))
    } else { None };
    let html = clean(input.get("html").and_then(Value::as_str).unwrap_or(""))?;
    document["notes"].as_array_mut().unwrap().push(json!({"id":id,"parent_note_id":parent,"anchor":anchor,"timing":timing,"html":html,
        "author_ref":author.principal_ref,"actor_kind":author.actor_kind,"recorded_at_unix_seconds":now}));
    Ok(())
}
fn decode(input: &Value) -> io::Result<Value> {
    if input.get("html").is_some() && input.get("value").is_some() {
        return Err(invalid("supply retained HTML or snapshot value, not both"));
    }
    let snapshot = if let Some(html) = input.get("html") {
        let html = html.as_str().ok_or_else(|| invalid("retained HTML must be text"))?;
        if html.len() > crate::source_safety::MAX_SOURCE * 4 { return Err(invalid("retained HTML exceeds bounded import size")); }
        let (_, rest) = html.split_once(OPEN).ok_or_else(|| invalid("not a native retained HTML document; original-template import needs its own adapter"))?;
        if rest.contains(OPEN) { return Err(invalid("ambiguous multiple retained payloads")); }
        let (json, _) = rest.split_once("</script>").ok_or_else(|| invalid("retained JSON payload is unterminated"))?;
        serde_json::from_str(json)?
    } else { input.get("value").cloned().ok_or_else(|| invalid("retained snapshot required"))? };
    if snapshot["schema"] != "central.document-retained-snapshot/v1" || snapshot["document"]["schema"] != SCHEMA {
        return Err(invalid("unsupported retained snapshot schema"));
    }
    validate(&snapshot["document"])?;
    Ok(snapshot)
}
fn protect(document: &mut Value, actor: &ContributionAuthor, now: u64, previous: Option<&Value>) -> io::Result<()> {
    for collection in ["contributions", "notes"] {
        let Some(values) = document.get_mut(collection).and_then(Value::as_array_mut) else { continue; };
        for value in values {
            let html = clean(value["html"].as_str().unwrap_or(""))?;
            value["html"] = json!(html);
            let prior = previous.and_then(|p| p[collection].as_array()).and_then(|v| v.iter().find(|p| p["id"] == value["id"]));
            if prior == Some(value) { continue; }
            // Portable/external attribution is a claim, never an authentication
            // grant. Existing native authors are retained despite edited labels.
            let claimed = json!({"author_ref":value["author_ref"],"actor_kind":value["actor_kind"]});
            if let Some(prior) = prior {
                value["author_ref"] = prior["author_ref"].clone();
                value["actor_kind"] = prior["actor_kind"].clone();
            }
            value["imported_attribution"] = claimed;
            value["attribution_standing"] = json!("retained-material-not-authenticated-by-import");
            value["human_touched"] = json!(true);
            value["locked"] = json!(true);
            value["reviewed_by"] = json!(actor.principal_ref);
            value["reviewed_at_unix_seconds"] = json!(now);
        }
    }
    Ok(())
}
pub(super) fn reconcile(document: &mut Value, input: &Value, author: &ContributionAuthor, now: u64) -> io::Result<()> {
    if author.actor_kind != "human" { return Err(denied("external source reconciliation requires authenticated human review")); }
    validate(document)?;
    protect(document, author, now, None)?;
    if document.get("external_reconciliations").is_none() { document["external_reconciliations"] = json!([]); }
    document["external_reconciliations"].as_array_mut().ok_or_else(|| invalid("invalid external reconciliation history"))?.push(json!({
        "external_revision":text(input,"expected_revision")?,"previous_native_revision":text(input,"expected_native_revision")?,
        "reviewed_by":author.principal_ref,"recorded_at_unix_seconds":now,"source_authorship_inferred":false
    }));
    Ok(())
}
pub(super) fn edit(document: &mut Value, input: &Value, author: &ContributionAuthor, reviewer: Option<&Principal>, now: u64) -> io::Result<()> {
    match text(input, "operation")? {
        "note.add" => note(document, input, author, now),
        "entry.insert" => {
            let id = text(input, "entry_id")?;
            let before = text(input, "before_entry_id")?;
            let entries = document["entries"].as_array().unwrap();
            if entries.iter().any(|v| v["id"] == id) { return Err(conflict("EntryId already exists")); }
            let index = entries.iter().position(|v| v["id"] == before).ok_or_else(|| invalid("insertion target is not an existing entry"))?;
            document["entries"].as_array_mut().unwrap().insert(index,json!({"id":id,"author_ref":author.principal_ref,"reply_to":null,"occurred_at_unix_seconds":now,"received_at_unix_seconds":now}));
            if input.get("html").is_some() { append(document,input,author,reviewer,now,Some(id.into()),None)?; }
            Ok(())
        }
        "portable.restore" => {
            if author.actor_kind != "human" { return Err(denied("portable restoration requires an authenticated human, not imported labels")); }
            let snapshot = decode(input)?;
            let mut restored = snapshot["document"].clone();
            for key in ["document_id","kind","day_ref","date","created_at_unix_seconds","created_by","fields"] {
                if restored[key] != document[key] { return Err(conflict(format!("portable restoration cannot change native {key}"))); }
            }
            if restored["lifecycle"] != document["lifecycle"] { return Err(conflict("restore does not implicitly close or reopen a document")); }
            protect(&mut restored,author,now,Some(document))?;
            // Recovery and idempotency history come from the owner, never the
            // imported JSON. Preserve origin as untrusted evidence by reference.
            restored["operations"] = document["operations"].clone();
            restored["creation_digest"] = document["creation_digest"].clone();
            restored["external_reconciliations"] = document.get("external_reconciliations").cloned().unwrap_or(json!([]));
            restored["portable_imports"] = document.get("portable_imports").cloned().unwrap_or(json!([]));
            restored["portable_imports"].as_array_mut().ok_or_else(||invalid("invalid retained import history"))?.push(json!({
                "source_ref":snapshot["source_ref"],"revision":snapshot["revision"],"standing":"retained-unverified-origin",
                "reviewed_by":author.principal_ref,"recorded_at_unix_seconds":now
            }));
            *document=restored;
            Ok(())
        }
        _ => Err(invalid("unsupported native contribution operation")),
    }
}
pub(super) fn export(scope: &Scope,input: &Value) -> io::Result<Value> {
    let (source,mut document,meta)=reading(scope,input)?;
    validate(&document)?;
    refresh_anchors(&mut document)?;
    let title=escape(document["title"].as_str().unwrap_or(""));
    let mut body=String::new();
    for field in document["fields"].as_array().unwrap() {
        let value=field["template_pointer"].as_str().and_then(|p|document["template_payload"].pointer(p));
        body.push_str(&format!("<section data-field-id=\"{}\"><h2>{}</h2><pre>{}</pre>",escape(text(field,"id")?),escape(field["label"].as_str().unwrap_or("")),escape(&value.map(|v|v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string())).unwrap_or_default())));
        render_contributions(&document,"field_id",&field["id"],&mut body)?;
        body.push_str("</section>");
    }
    for entry in document["entries"].as_array().unwrap() {
        body.push_str(&format!("<article data-entry-id=\"{}\"><header>{}</header>",escape(text(entry,"id")?),escape(entry["author_ref"].as_str().unwrap_or(""))));
        if let Some(reply)=entry["reply_to"].as_str() {body.push_str(&format!("<p>Reply to {}</p>",escape(reply)));}
        render_contributions(&document,"entry_id",&entry["id"],&mut body)?;
        body.push_str("</article>");
    }
    if let Some(notes)=document.get("notes").and_then(Value::as_array) {
        for note in notes {
            body.push_str(&format!("<aside data-note-id=\"{}\"><header>{} — {}</header><blockquote>{}</blockquote>{}",escape(text(note,"id")?),escape(note["author_ref"].as_str().unwrap_or("")),escape(note["timing"].as_str().unwrap_or("")),escape(note["anchor"]["original_text"].as_str().unwrap_or("")),clean(note["html"].as_str().unwrap_or(""))?));
            if note["anchor"]["status"]=="needs-review" {body.push_str("<p>Stale anchor — review original target.</p>");}
            if let Some(parent)=note["parent_note_id"].as_str() {body.push_str(&format!("<p>Reply to note {}</p>",escape(parent)));}
            body.push_str("</aside>");
        }
    }
    let snapshot=json!({"schema":"central.document-retained-snapshot/v1","source_ref":source.source.source_ref,"revision":source.revision,"document":document,"source_authority":"retained-snapshot-not-live-authority"});
    let escaped=serde_json::to_string(&snapshot)?.replace('<',"\\u003c").replace('>',"\\u003e").replace('&',"\\u0026");
    let html=format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; base-uri 'none'; form-action 'none'\"><title>{title}</title></head><body><h1>{title}</h1>{body}{OPEN}{escaped}</script></body></html>");
    Ok(json!({"schema":"central.document-export/v1","snapshot":snapshot,"html":html,"unreviewed_external_revision":meta["last_native_revision"]!=source.revision.revision,
        "executable_scripts":false,"automatic_network_or_model_calls":false,"original_HTML_fidelity":"not asserted; original fixture required for that independent test",
        "persistence":"explicit retained copy; export does not overwrite native source","native_reopen_operation":"portable.restore"}))
}
fn render_contributions(document:&Value,key:&str,id:&Value,body:&mut String)->io::Result<()> {
    for contribution in document["contributions"].as_array().unwrap().iter().filter(|c|c["removed"]!=true && c[key]==*id) {
        let label=if contribution["actor_kind"]=="human" {"F"} else {"H"};
        body.push_str(&format!("<section data-contribution-id=\"{}\"><header>{label} — {}</header>{}</section>",escape(text(contribution,"id")?),escape(contribution["author_ref"].as_str().unwrap_or("")),clean(contribution["html"].as_str().unwrap_or(""))?));
    }
    Ok(())
}
