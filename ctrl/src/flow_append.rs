// Native Plural Flow contribution append (O:I #558, PF1/PF2). Included by
// `file_mutation.rs`, so it shares the ordinary-file owner's root policy,
// lock, journal and conflict semantics instead of adding a second write path.
//
// A Flow is an ordinary HTML file under Control/user/flows/ whose state is
// the embedded `ql-doc` JSON island. This action rereads the current bytes,
// applies the same rules as O:I's `desktop/cradle/src/flow/plural.ts`
// (`appendContribution`), and writes through `central.files.write`'s CAS. A
// concurrent writer that wins the CAS makes this append reread and try again
// on the newer bytes, so both contributions survive. The document is the
// idempotency record: `entry.request = {ref, digest}`. The append writes only
// what it owns into the island as written (`flow_splice_append`, O:I #766):
// every other byte, key order and number stays exactly as it was. The shared
// conformance cases live in `ctrl/tests/fixtures/plural-flow-cases.json`, a
// copy of O:I's `desktop/cradle/documents/fixtures/plural-flow-cases.json`.
use sha2::{Digest, Sha256};

const FLOW_ISLAND_OPEN: &str = "<script type=\"application/json\" id=\"ql-doc\">";
const FLOW_FORMAT: u64 = 4;
const FLOW_APPEND_RETRIES: usize = 8;

struct FlowRefusal {
    code: String,
    message: String,
}
fn refuse<T>(code: &str, message: impl Into<String>) -> Result<T, FlowRefusal> {
    Err(FlowRefusal {
        code: code.to_string(),
        message: message.into(),
    })
}

/// The caller as this owner identified it. `authenticated` is true only when a
/// host-held credential matched a native-action grant; body fields never
/// establish it.
struct FlowCaller {
    kind: String,
    reference: Option<String>,
    session: Option<String>,
    agent: Option<String>,
    generation: Option<Value>,
    workcell: Option<String>,
    authenticated: bool,
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn flow_island_span(html: &str) -> io::Result<(usize, usize)> {
    let start = html
        .find(FLOW_ISLAND_OPEN)
        .ok_or_else(|| invalid("Not a 0/1 flow document: its embedded document state is missing"))?
        + FLOW_ISLAND_OPEN.len();
    let end = html[start..]
        .find("</script>")
        .map(|i| start + i)
        .ok_or_else(|| invalid("Flow document state is not terminated"))?;
    Ok((start, end))
}
/// The island's text as JSON: the legacy `<\/script` and `<\!--` escapes read
/// back as written content.
fn flow_unescape(raw: &str) -> String {
    raw.replace("<\\/script", "</script").replace("<\\!--", "<!--")
}
fn flow_parse(html: &str) -> io::Result<Value> {
    let (start, end) = flow_island_span(html)?;
    serde_json::from_str(&flow_unescape(&html[start..end]))
        .map_err(|e| invalid(&format!("Flow document state is not valid JSON: {e}")))
}
/// JSON this owner writes into the island. `</script`, in any case, and `<!--`
/// must not survive inside it; both escapes are valid JSON string escapes, so
/// the island stays parseable anywhere. The letters after `<\/` stay as
/// written, so `</SCRIPT` reads back as itself (O:I #797, ported from O:I
/// `desktop/cradle/kernel/src/page/island.rs` `escape`, #793).
fn flow_island_escape(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let lower = json.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("</script") {
        out.push_str(&json[at..at + i]);
        out.push_str("<\\/");
        at += i + "</".len();
    }
    out.push_str(&json[at..]);
    out.replace("<!--", "<\\u0021--")
}

// The island carries state this owner does not own: unknown metadata, the
// order its writer gave the keys, integers no u64/i64/f64 holds exactly, the
// writer's spacing and escapes. Re-serialising a parsed value rewrites all of
// it (keys alphabetical, integers past 2^64 rounded: D10 of O:I #558). So an
// append locates what it owns in the island's own text and splices only that.

/// Where one JSON value sits in the island text, in bytes.
#[derive(Clone, Copy)]
struct IslandSpan {
    start: usize,
    end: usize,
}
/// Replace `start..end` of the island with the text.
type IslandEdit = (usize, usize, String);
/// One child of an object (with its key's span) or of an array (without).
type IslandChild = (Option<IslandSpan>, IslandSpan);

fn island_unlocated() -> io::Error {
    invalid("Flow document state could not be located for the append")
}
fn island_space(text: &[u8], mut at: usize) -> usize {
    while text
        .get(at)
        .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    {
        at += 1;
    }
    at
}
/// The children of the object or array opening at `open` (an object's carry
/// their key spans) and the end of the container.
fn island_children(text: &[u8], open: usize) -> io::Result<(Vec<IslandChild>, usize)> {
    let close = match text.get(open) {
        Some(b'{') => b'}',
        Some(b'[') => b']',
        _ => return Err(island_unlocated()),
    };
    let mut children = Vec::new();
    let mut at = island_space(text, open + 1);
    if text.get(at) == Some(&close) {
        return Ok((children, at + 1));
    }
    loop {
        let key = if close == b'}' {
            let key = island_value(text, at)?;
            at = island_space(text, key.end);
            if text.get(at) != Some(&b':') {
                return Err(island_unlocated());
            }
            at = island_space(text, at + 1);
            Some(key)
        } else {
            None
        };
        let value = island_value(text, at)?;
        children.push((key, value));
        at = island_space(text, value.end);
        match text.get(at) {
            Some(b',') => at = island_space(text, at + 1),
            Some(&b) if b == close => return Ok((children, at + 1)),
            _ => return Err(island_unlocated()),
        }
    }
}
/// The value starting at `at`. Only called on text that already parsed, so
/// its nesting is within the JSON reader's own depth limit.
fn island_value(text: &[u8], at: usize) -> io::Result<IslandSpan> {
    let end = match text.get(at) {
        Some(b'"') => {
            let mut i = at + 1;
            loop {
                match text.get(i) {
                    Some(b'\\') => i += 2,
                    Some(b'"') => break i + 1,
                    Some(_) => i += 1,
                    None => return Err(island_unlocated()),
                }
            }
        }
        Some(b'{' | b'[') => island_children(text, at)?.1,
        Some(_) => {
            let mut i = at;
            while text
                .get(i)
                .is_some_and(|b| !matches!(b, b',' | b']' | b'}' | b' ' | b'\t' | b'\n' | b'\r'))
            {
                i += 1;
            }
            if i == at {
                return Err(island_unlocated());
            }
            i
        }
        None => return Err(island_unlocated()),
    };
    Ok(IslandSpan { start: at, end })
}
/// An object's members in written order: each key as a reader decodes it, and
/// its value's span.
fn island_object(island: &str, object: IslandSpan) -> io::Result<Vec<(String, IslandSpan)>> {
    if island.as_bytes().get(object.start) != Some(&b'{') {
        return Err(island_unlocated());
    }
    island_children(island.as_bytes(), object.start)?
        .0
        .into_iter()
        .map(|(key, value)| {
            let key = key.ok_or_else(island_unlocated)?;
            let name = serde_json::from_str::<String>(&flow_unescape(&island[key.start..key.end]))
                .map_err(|_| island_unlocated())?;
            Ok((name, value))
        })
        .collect()
}
fn island_array(island: &str, array: IslandSpan) -> io::Result<Vec<IslandSpan>> {
    if island.as_bytes().get(array.start) != Some(&b'[') {
        return Err(island_unlocated());
    }
    Ok(island_children(island.as_bytes(), array.start)?
        .0
        .into_iter()
        .map(|(_, value)| value)
        .collect())
}
/// A member's value: the last occurrence of the key, the one a reader keeps.
fn island_find(members: &[(String, IslandSpan)], key: &str) -> Option<IslandSpan> {
    members
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| *value)
}
/// Set `key` of `object` to `json`: an existing value is replaced where it
/// stands; a new member goes after the object's last member.
fn island_set(
    members: &[(String, IslandSpan)],
    object: IslandSpan,
    key: &str,
    json: String,
    edits: &mut Vec<IslandEdit>,
) -> io::Result<()> {
    if let Some(value) = island_find(members, key) {
        edits.push((value.start, value.end, json));
        return Ok(());
    }
    let member = format!("{}:{json}", serde_json::to_string(key).map_err(io::Error::other)?);
    match members.last() {
        Some((_, last)) => edits.push((last.end, last.end, format!(",{member}"))),
        None => edits.push((object.start + 1, object.start + 1, member)),
    }
    Ok(())
}

/// The Flow file with this append spliced into its island: the new entry
/// after the last one, `meta.revision`, and a participant binding the append
/// set. Every other byte of the file is copied as written. The island must
/// then read as exactly `next`, or nothing is written.
fn flow_splice_append(html: &str, doc: &Value, next: &Value) -> io::Result<String> {
    let (start, end) = flow_island_span(html)?;
    let island = &html[start..end];
    let root = island_value(island.as_bytes(), island_space(island.as_bytes(), 0))?;
    let top = island_object(island, root)?;
    let json = |value: &Value| {
        serde_json::to_string(value)
            .map(|text| flow_island_escape(&text))
            .map_err(io::Error::other)
    };
    let mut edits: Vec<IslandEdit> = Vec::new();

    let no_entries = || invalid("Flow document state has no entries list");
    let entries = island_find(&top, "entries").ok_or_else(no_entries)?;
    let entry = next
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|list| list.last())
        .ok_or_else(no_entries)?;
    match island_array(island, entries)?.last() {
        Some(last) => edits.push((last.end, last.end, format!(",{}", json(entry)?))),
        None => edits.push((entries.start + 1, entries.start + 1, json(entry)?)),
    }

    let meta_at = island_find(&top, "meta").ok_or_else(island_unlocated)?;
    let meta = island_object(island, meta_at)?;
    island_set(&meta, meta_at, "revision", json(&next["meta"]["revision"])?, &mut edits)?;

    let seats_before = doc.pointer("/meta/participants").and_then(Value::as_array);
    let seats_after = next.pointer("/meta/participants").and_then(Value::as_array);
    if let (Some(seats_before), Some(seats_after)) = (seats_before, seats_after) {
        for (index, (was, now)) in seats_before.iter().zip(seats_after).enumerate() {
            let Some(binding) = now.get("binding").filter(|b| Some(*b) != was.get("binding")) else {
                continue;
            };
            let list = island_find(&meta, "participants").ok_or_else(island_unlocated)?;
            let seat = *island_array(island, list)?
                .get(index)
                .ok_or_else(island_unlocated)?;
            let members = island_object(island, seat)?;
            island_set(&members, seat, "binding", json(binding)?, &mut edits)?;
        }
    }

    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut spliced = island.to_string();
    for (from, to, text) in edits {
        spliced.replace_range(from..to, &text);
    }
    let reread: Value =
        serde_json::from_str(&flow_unescape(&spliced)).map_err(|_| island_unlocated())?;
    if &reread != next {
        return Err(io::Error::other(
            "the append could not be placed in the Flow as written; nothing was written",
        ));
    }
    Ok(format!("{}{spliced}{}", &html[..start], &html[end..]))
}

fn relations_of(entry: &Value) -> Vec<Value> {
    if let Some(list) = entry.get("relations").and_then(Value::as_array) {
        if !list.is_empty() {
            return list.clone();
        }
    }
    match entry.get("replyTo") {
        Some(reply) if reply.is_object() => vec![json!({
            "type": "reply",
            "entryId": reply.get("entryId").cloned().unwrap_or(Value::Null),
            "anchor": reply.get("anchor").cloned().unwrap_or(Value::Null),
        })],
        _ => vec![],
    }
}
/// Digest of what the operation asked for, excluding id and time. Matches
/// `requestDigest` in plural.ts byte for byte: sorted keys, compact JSON.
fn flow_request_digest(entry: &Value) -> String {
    let payload = json!({
        "authorKey": entry.get("authorKey").cloned().unwrap_or(Value::Null),
        "html": entry.get("html").cloned().unwrap_or(Value::Null),
        "addressees": entry.get("addressees").cloned().unwrap_or_else(|| json!([])),
        "audience": entry.get("audience").cloned().unwrap_or_else(|| json!("group")),
        "intent": entry.get("intent").cloned().unwrap_or_else(|| json!("contribution")),
        "relations": relations_of(entry),
        "basisRevision": entry.get("basisRevision").cloned().unwrap_or(Value::Null),
        "ref": entry.pointer("/request/ref").cloned().unwrap_or(Value::Null),
    });
    sha256_hex(&serde_json::to_string(&payload).unwrap_or_default())
}
/// The document's `meta.revision` as every Flow writer reads it: a whole
/// number from 0 to 2^53 in any spelling JSON allows, so a hand-written `3.0`
/// is 3, as O:I's page and host (`plural.ts` `appendContribution`) and its
/// kernel read it (O:I #798, ported from O:I `desktop/cradle/kernel/src/page.rs`
/// `apply_field`, #797). Anything else is no count yet: 0, so the first
/// append writes 1.
fn flow_revision(doc: &Value) -> u64 {
    let count = |n: &serde_json::Number| {
        n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.fract() == 0.0 && (0.0..=9_007_199_254_740_992.0).contains(f))
                .map(|f| f as u64)
        })
    };
    match doc.pointer("/meta/revision") {
        Some(Value::Number(n)) => count(n).unwrap_or(0),
        _ => 0,
    }
}
fn flow_format_version(doc: &Value) -> u64 {
    doc.pointer("/meta/format/version")
        .and_then(Value::as_u64)
        .unwrap_or(3)
}
fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn label(participant: &Value) -> String {
    str_of(participant, "name")
        .or_else(|| str_of(participant, "initial"))
        .unwrap_or("that participant")
        .to_string()
}

fn flow_check_caller(
    author: &Value,
    caller: &FlowCaller,
    claimed_basis: Option<&str>,
    participants: &[Value],
) -> Result<Value, FlowRefusal> {
    if author.get("left").is_some_and(|v| !v.is_null()) {
        return refuse("participant-left", format!("{} has left this flow", label(author)));
    }
    if str_of(author, "role").unwrap_or("contributor") == "observer" {
        return refuse(
            "observer-cannot-contribute",
            format!("{} is an observer", label(author)),
        );
    }
    let bound = author
        .get("binding")
        .filter(|b| str_of(b, "basis") == Some("verified"));
    let bound_ref = bound.and_then(|b| str_of(b, "ref"));
    let kind = str_of(author, "kind").unwrap_or("person");
    fn need_auth(who: &Value) -> Result<Value, FlowRefusal> {
        refuse(
            "authentication-required",
            format!(
                "{} is bound; only an authenticated caller may write as them",
                label(who)
            ),
        )
    }
    match caller.kind.as_str() {
        "agent" => {
            if kind != "agent" {
                return refuse("impersonation", "an agent cannot author as a person");
            }
            let identity = caller.session.as_deref().or(caller.reference.as_deref());
            let Some(identity) = identity else {
                return refuse("author-not-caller", "the native caller carries no agent identity");
            };
            if bound.is_some() && !caller.authenticated {
                return need_auth(author);
            }
            if let Some(bound_ref) = bound_ref {
                if bound_ref != identity && Some(bound_ref) != caller.agent.as_deref() {
                    return refuse(
                        "author-not-caller",
                        format!("{} is bound to another agent", label(author)),
                    );
                }
            }
            if caller.authenticated {
                // The credential says which agent this is. It writes as the seat
                // declared for that agent, never as another agent's seat (bound or
                // not), and never as another seat while it holds its own.
                let mine: Vec<&str> = [
                    caller.agent.as_deref(),
                    caller.session.as_deref(),
                    caller.reference.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect();
                let declared = author.pointer("/binding/ref").and_then(Value::as_str);
                if declared.is_some_and(|d| !mine.contains(&d)) {
                    return refuse(
                        "impersonation",
                        format!("{} is declared for another agent", label(author)),
                    );
                }
                let own_seat = participants.iter().find(|p| {
                    str_of(p, "key") != str_of(author, "key")
                        && str_of(p, "kind") == Some("agent")
                        && p.pointer("/binding/ref")
                            .and_then(Value::as_str)
                            .is_some_and(|r| mine.contains(&r))
                });
                if let Some(seat) = own_seat {
                    return refuse(
                        "impersonation",
                        format!(
                            "this credential is {}'s; it cannot write as {}",
                            label(seat),
                            label(author)
                        ),
                    );
                }
            }
            let mut attribution = serde_json::Map::new();
            attribution.insert(
                "basis".into(),
                json!(if caller.authenticated { "verified" } else { "declared" }),
            );
            if let Some(v) = &caller.agent {
                attribution.insert("agency".into(), json!(v));
            }
            if let Some(v) = &caller.session {
                attribution.insert("session".into(), json!(v));
            }
            if let Some(v) = &caller.generation {
                attribution.insert("generation".into(), v.clone());
            }
            if let Some(v) = &caller.workcell {
                attribution.insert("workcell".into(), json!(v));
            }
            Ok(Value::Object(attribution))
        }
        "human" => {
            if kind == "agent" {
                if claimed_basis == Some("verified") {
                    return refuse(
                        "attribution-overclaim",
                        "a human caller cannot produce verified agent attribution",
                    );
                }
                return Ok(json!({"basis": "declared"}));
            }
            if bound.is_some() && !caller.authenticated {
                return need_auth(author);
            }
            if let (Some(bound_ref), Some(reference)) = (bound_ref, caller.reference.as_deref()) {
                if bound_ref != reference {
                    return refuse(
                        "impersonation",
                        format!("{} is bound to another person", label(author)),
                    );
                }
            }
            if caller.authenticated {
                if let Some(reference) = caller.reference.as_deref() {
                    // An authenticated person writes as the seat declared for their
                    // identity. Holding a seat of their own, they cannot write as
                    // another: the credential says who they are, and sharing a flow,
                    // a machine or a label does not make them the next seat's person.
                    let declared = author.pointer("/binding/ref").and_then(Value::as_str);
                    if declared.is_some_and(|d| d != reference) {
                        return refuse(
                            "impersonation",
                            format!("{} is declared for another identity", label(author)),
                        );
                    }
                    let own_seat = participants.iter().find(|p| {
                        str_of(p, "key") != str_of(author, "key")
                            && str_of(p, "kind") == Some("person")
                            && p.pointer("/binding/ref").and_then(Value::as_str) == Some(reference)
                    });
                    if let Some(seat) = own_seat {
                        return refuse(
                            "impersonation",
                            format!(
                                "this credential is {}'s; it cannot write as {}",
                                label(seat),
                                label(author)
                            ),
                        );
                    }
                    if declared == Some(reference) {
                        return Ok(json!({"basis": "verified"}));
                    }
                }
            }
            Ok(json!({
                "basis": if bound.is_some() && caller.reference.is_some() { "verified" } else { "declared" }
            }))
        }
        _ => {
            if bound.is_some() && !caller.authenticated {
                return need_auth(author);
            }
            if claimed_basis == Some("verified") {
                return refuse(
                    "attribution-overclaim",
                    "a system caller cannot verify an author",
                );
            }
            Ok(json!({"basis": "declared"}))
        }
    }
}

enum FlowAppend {
    Appended,
    Recovered,
}

/// Mirror of `appendContribution` (plural.ts). Returns the next document and
/// the entry; never mutates `doc` on refusal.
fn flow_append(
    doc: &Value,
    request: &Value,
    caller: &FlowCaller,
) -> Result<(Value, Value, FlowAppend), FlowRefusal> {
    let expected_document = request.get("expectedDocumentId").filter(|value| !value.is_null());
    if let Some(expected) = expected_document {
        // Validate with the existing native UUID parser, retaining the exact
        // authored spelling as the document basis rather than normalizing it.
        let valid = expected
            .as_str()
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok());
        if !valid {
            return refuse("invalid-expected-document-id", "expected_document_id must be an exact UUID string");
        }
        if doc.pointer("/meta/documentId") != Some(expected) {
            return refuse("document-mismatch", "the Flow document at this location differs from the pinned document");
        }
    }
    let operation_ref = str_of(request, "operationRef").unwrap_or("");
    if operation_ref.is_empty() {
        return refuse("empty-operation", "a native operation reference is required");
    }
    let version = flow_format_version(doc);
    if version > FLOW_FORMAT {
        return refuse(
            "unsupported-format",
            format!("flow format v{version} needs a newer writer"),
        );
    }
    if version < FLOW_FORMAT {
        return refuse(
            "legacy-format",
            "this document is a legacy form; upgrade it explicitly before contributing",
        );
    }
    let participants: Vec<Value> = doc
        .pointer("/meta/participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let author_key = str_of(request, "authorKey").unwrap_or("");
    let Some(author) = participants.iter().find(|p| str_of(p, "key") == Some(author_key)) else {
        return refuse("unknown-author", format!("no participant {author_key}"));
    };
    let relations: Vec<Value> = request
        .get("relations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let entry_id = str_of(request, "entryId")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("e-{}", &sha256_hex(operation_ref)[..24]));
    let mut probe = serde_json::Map::new();
    probe.insert("id".into(), json!(entry_id));
    probe.insert("author".into(), author.get("initial").cloned().unwrap_or(json!("")));
    probe.insert("authorKey".into(), json!(author_key));
    probe.insert("at".into(), request.get("at").cloned().unwrap_or(Value::Null));
    probe.insert("html".into(), request.get("html").cloned().unwrap_or(Value::Null));
    probe.insert("replyTo".into(), Value::Null);
    probe.insert("touched".into(), json!(false));
    for key in ["addressees", "audience", "intent", "basisRevision"] {
        if let Some(v) = request.get(key).filter(|v| !v.is_null()) {
            probe.insert(key.into(), v.clone());
        }
    }
    probe.insert("relations".into(), Value::Array(relations.clone()));
    probe.insert("request".into(), json!({"ref": operation_ref, "digest": ""}));
    if let Some(expected) = expected_document {
        probe.get_mut("request").expect("request inserted")["documentId"] = expected.clone();
    }
    let digest = flow_request_digest(&Value::Object(probe.clone()));
    let entries: Vec<Value> = doc
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(existing) = entries
        .iter()
        .find(|e| e.pointer("/request/ref").and_then(Value::as_str) == Some(operation_ref))
    {
        if existing.pointer("/request/documentId").filter(|value| !value.is_null()) != expected_document {
            return refuse("request-conflict", format!("operation {operation_ref} already recorded with a different document basis"));
        }
        if existing.pointer("/request/digest").and_then(Value::as_str) != Some(digest.as_str()) {
            return refuse(
                "request-conflict",
                format!("operation {operation_ref} already recorded with a different payload"),
            );
        }
        return Ok((doc.clone(), existing.clone(), FlowAppend::Recovered));
    }
    let html = str_of(request, "html").unwrap_or("");
    let has_media = ["<img", "<video", "<audio", "<iframe", "<object", "<embed"]
        .iter()
        .any(|tag| html.to_ascii_lowercase().contains(tag));
    if flow_plain_text(&html.replace("&nbsp;", " ")).trim().is_empty() && !has_media {
        return refuse("empty-contribution", "a contribution needs content");
    }
    let revision = flow_revision(doc);
    if let Some(basis) = request.get("basisRevision").filter(|v| !v.is_null()) {
        match basis.as_u64() {
            Some(n) if n <= revision => {}
            _ => {
                return refuse(
                    "invalid-basis-revision",
                    "the basis revision must be a revision this document has reached",
                )
            }
        }
    }
    if let Some(audience) = request.get("audience").filter(|v| !v.is_null()) {
        let listed = audience.get("keys").is_some_and(Value::is_array);
        if audience.as_str() != Some("group") && !listed {
            return refuse(
                "invalid-audience",
                "an audience is \"group\" or a list of participant keys",
            );
        }
    }
    let behalf = match request.pointer("/attribution/onBehalfOf").filter(|v| !v.is_null()) {
        None => None,
        Some(claim) => {
            let key = str_of(claim, "key").unwrap_or("");
            let authority = str_of(claim, "authority").unwrap_or("");
            if key == author_key
                || authority.trim().is_empty()
                || !participants.iter().any(|p| {
                    str_of(p, "key") == Some(key) && p.get("left").is_none_or(Value::is_null)
                })
            {
                return refuse(
                    "unknown-behalf-of",
                    "on behalf of names another participant and the authority claimed",
                );
            }
            // The authority is the caller's own statement, never something the owner checked.
            Some(json!({"key": key, "authority": authority, "basis": "declared"}))
        }
    };
    let claimed = request.pointer("/attribution/basis").and_then(Value::as_str);
    let attribution = flow_check_caller(author, caller, claimed, &participants)?;
    if entries.iter().any(|e| str_of(e, "id") == Some(entry_id.as_str())) {
        return refuse("duplicate-entry-id", format!("entry {entry_id} already exists"));
    }
    let keys: Vec<&str> = participants.iter().filter_map(|p| str_of(p, "key")).collect();
    for key in request
        .get("addressees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !keys.contains(&key) {
            return refuse("unknown-addressee", format!("no participant {key}"));
        }
        if participants
            .iter()
            .any(|p| str_of(p, "key") == Some(key) && p.get("left").is_some_and(|v| !v.is_null()))
        {
            return refuse(
                "addressee-left",
                format!("{key} has left this flow and cannot be asked"),
            );
        }
    }
    if let Some(audience) = request.get("audience").filter(|a| a.is_object()) {
        for key in audience
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !keys.contains(&key) {
                return refuse("unknown-audience-key", format!("no participant {key}"));
            }
        }
    }
    for relation in &relations {
        let kind = str_of(relation, "type").unwrap_or("");
        if !["reply", "branch", "converge", "correct", "source", "artifact"].contains(&kind) {
            return refuse(
                "invalid-relation-type",
                format!("{kind} is not a relation this form knows"),
            );
        }
        if kind == "source" || kind == "artifact" {
            if str_of(relation, "ref").unwrap_or("").is_empty() {
                return refuse("relation-shape", format!("{kind} relation needs a ref"));
            }
            continue;
        }
        let Some(target) = str_of(relation, "entryId").filter(|t| !t.is_empty()) else {
            return refuse("relation-shape", format!("{kind} relation needs an entry"));
        };
        if !entries.iter().any(|e| str_of(e, "id") == Some(target)) {
            return refuse(
                "relation-target-missing",
                format!("entry {target} is not in this document"),
            );
        }
        if relation
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|r| r > revision)
        {
            return refuse(
                "relation-revision-ahead",
                "relation names a revision the document has not reached",
            );
        }
    }
    let count = |kind: &str| relations.iter().filter(|r| str_of(r, "type") == Some(kind)).count();
    if count("reply") > 1 {
        return refuse(
            "reply-multiple",
            "an entry replies to one entry; use converge to relate several",
        );
    }
    if count("converge") == 1 {
        return refuse("converge-needs-two", "a convergence relates at least two entries");
    }
    let mut entry = probe;
    let mut attribution = attribution;
    if let (Some(behalf), Some(map)) = (behalf, attribution.as_object_mut()) {
        map.insert("onBehalfOf".into(), behalf);
    }
    entry.insert("attribution".into(), attribution.clone());
    entry.insert("request".into(), json!({"ref": operation_ref, "digest": digest}));
    if let Some(expected) = expected_document {
        entry.get_mut("request").expect("request inserted")["documentId"] = expected.clone();
    }
    if let Some(reply) = relations.iter().find(|r| str_of(r, "type") == Some("reply")) {
        entry.insert(
            "replyTo".into(),
            json!({
                "entryId": reply.get("entryId").cloned().unwrap_or(Value::Null),
                "anchor": reply.get("anchor").cloned().unwrap_or(Value::Null),
            }),
        );
    }
    if relations.is_empty() {
        entry.remove("relations");
    }
    let entry = Value::Object(entry);
    let mut next = doc.clone();
    next["entries"]
        .as_array_mut()
        .expect("entries checked by flow_append_execute")
        .push(entry.clone());
    next["meta"]["revision"] = json!(revision + 1);
    // A caller this owner authenticated binds its participant on first write.
    if str_of(&attribution, "basis") == Some("verified") {
        // An agent is bound as the enduring agent, not as the session it first
        // used: a fresh body for the same agent must still be that participant.
        let identity = if caller.kind == "agent" {
            caller
                .agent
                .as_deref()
                .or(caller.session.as_deref())
                .or(caller.reference.as_deref())
        } else {
            caller.reference.as_deref()
        };
        if let (Some(identity), Some(list)) = (
            identity,
            next.pointer_mut("/meta/participants").and_then(Value::as_array_mut),
        ) {
            if let Some(held) = list.iter_mut().find(|p| str_of(p, "key") == Some(author_key)) {
                let verified = held
                    .pointer("/binding/basis")
                    .and_then(Value::as_str)
                    == Some("verified");
                if !verified {
                    held["binding"] = json!({
                        "owner": if caller.kind == "agent" { "actuation" } else { "central" },
                        "ref": identity,
                        "basis": "verified",
                    });
                }
            }
        }
    }
    Ok((next, entry, FlowAppend::Appended))
}

fn flow_caller(input: &Value, principal: Option<&crate::continuous_work::authority::Principal>) -> io::Result<FlowCaller> {
    let (actor, kind, session) = attribution(input)?;
    let agent = input
        .get("agent_ref")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let generation = input.get("generation").filter(|v| !v.is_null()).cloned();
    let workcell = input
        .get("workcell")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(match principal {
        // The host-held credential decides who this is. A native service
        // (AIKit's encounter owner) speaks for the agent session it names; an
        // agent or human principal speaks for itself and cannot name another.
        Some(p) if p.actor_kind == "native-service" && kind == "agent" => FlowCaller {
            kind: "agent".into(),
            reference: session.clone(),
            session,
            agent,
            generation,
            workcell,
            authenticated: true,
        },
        Some(p) => {
            if let Some(session) = &session {
                if p.actor_kind == "agent" && session != &p.principal_ref {
                    return Err(denied(
                        "an authenticated agent cannot write as another agent session",
                    ));
                }
            }
            FlowCaller {
                kind: p.actor_kind.clone(),
                reference: Some(p.principal_ref.clone()),
                session: if p.actor_kind == "agent" { Some(p.principal_ref.clone()) } else { None },
                agent,
                generation,
                workcell,
                authenticated: true,
            }
        }
        None => FlowCaller {
            kind: kind.clone(),
            reference: if kind == "agent" { session.clone() } else { Some(actor) },
            session,
            agent,
            generation,
            workcell,
            authenticated: false,
        },
    })
}

fn flow_refusal_result(id: &str, refusal: FlowRefusal) -> ActionResult {
    // The refusal code is the error code, so a caller acts on it directly.
    ActionResult::failure_coded(
        Some(id),
        ResultStatus::InvalidInput,
        &refusal.code,
        refusal.message,
        Some(json!({"outcome": "refused", "refusal": refusal.code, "written": false})),
    )
}

/// Reread → apply → CAS write, retried on a lost race so a concurrent writer's
/// content is never overwritten. The outcome is `appended` or `recovered`
/// (an identical earlier request already in the document).
fn flow_append_execute(
    root: &Path,
    input: &Value,
    principal: Option<&crate::continuous_work::authority::Principal>,
) -> io::Result<Result<Value, FlowRefusal>> {
    let loc_value = input.get("location").cloned().unwrap_or(Value::Null);
    let loc: CentralPathRef = serde_json::from_value(loc_value.clone())?;
    if !loc.path.starts_with("Control/user/flows/") {
        return Err(denied("Flow contributions are appended only to flow instances under Control/user/flows/"));
    }
    let caller = flow_caller(input, principal)?;
    let request = json!({
        "operationRef": input.get("operation_ref"),
        "expectedDocumentId": input.get("expected_document_id"),
        "authorKey": input.get("author_key"),
        "html": input.get("html"),
        "at": input.get("at"),
        "addressees": input.get("addressees"),
        "audience": input.get("audience"),
        "intent": input.get("intent"),
        "relations": input.get("relations"),
        "basisRevision": input.get("basis_revision"),
        "attribution": input.get("attribution"),
        "entryId": input.get("entry_id"),
    });
    let root = root.canonicalize()?;
    let (actor, actor_kind, session) = attribution(input)?;
    for _ in 0..FLOW_APPEND_RETRIES {
        let current = read_file(&root, &loc, crate::files::FileEncoding::Utf8)?;
        let html = decode_content(&current.content, &current.content_encoding)?;
        let html = String::from_utf8(html).map_err(|_| invalid("Flow document is not UTF-8"))?;
        let doc = flow_parse(&html)?;
        if !doc.get("entries").is_some_and(Value::is_array) {
            return Err(invalid("Flow document state has no entries list"));
        }
        let (next, entry, outcome) = match flow_append(&doc, &request, &caller) {
            Ok(done) => done,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let revision = json!({"revision": current.revision});
        if matches!(outcome, FlowAppend::Recovered) {
            return Ok(Ok(json!({
                "schema": "central.flow-append/v1", "outcome": "recovered", "location": loc_value,
                "revision": revision["revision"], "document_revision": doc.pointer("/meta/revision"), "entry": entry, "changed": false,
                "attribution": entry.get("attribution"), "automatic_agent_or_model_invocation": false,
            })));
        }
        let content = flow_splice_append(&html, &doc, &next)?;
        let written = execute(
            &root,
            "write",
            &json!({
                "location": loc_value, "expected_revision": current.revision, "content": content,
                "actor": actor, "actor_kind": actor_kind, "agent_session_ref": session,
            }),
        )?;
        match written.get("outcome").and_then(Value::as_str) {
            Some("written") => {
                return Ok(Ok(json!({
                    "schema": "central.flow-append/v1", "outcome": "appended", "location": loc_value,
                    "previous_revision": current.revision, "revision": written["revision"], "document_revision": next.pointer("/meta/revision"),
                    "entry": entry, "changed": true, "attribution": entry.get("attribution"),
                    "change": written.get("change"), "automatic_agent_or_model_invocation": false,
                })));
            }
            // Another writer won the CAS: reread and apply again on its bytes.
            Some("conflict") => continue,
            other => return Err(io::Error::other(format!("Flow append write ended as {other:?}"))),
        }
    }
    Err(conflict("Flow changed under every append attempt; nothing was written. Try again."))
}

/// How a Flow append ends other than success: a typed refusal the caller can
/// act on (nothing was written), or an owner I/O condition.
pub enum FlowAppendError {
    Refused { code: String, message: String },
    Io(io::Error),
}
impl From<io::Error> for FlowAppendError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The append with the credential supplied by the host. `token` is the
/// host-held `CENTRAL_NATIVE_TOKEN` (never a request field); a missing or
/// unauthorised token leaves the caller unauthenticated, not refused.
pub fn flow_append_with_token(
    root: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> Result<Value, FlowAppendError> {
    let principal = token.and_then(|token| {
        let scope = crate::continuous_work::source::Scope::resolve(root, None).ok()?;
        crate::continuous_work::authority::authenticate(&scope, Some(token), "central.flow.append", None, now).ok()
    });
    match flow_append_execute(root, input, principal.as_ref())? {
        Ok(done) => Ok(done),
        Err(refusal) => Err(FlowAppendError::Refused {
            code: refusal.code,
            message: refusal.message,
        }),
    }
}

fn flow_append_action(input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let id = "central.flow.append";
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(e) => return ActionResult::failure(Some(id), ResultStatus::UnavailableCapability, e.to_string(), None),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
    match flow_append_with_token(&root.path, input, token.as_deref(), now) {
        Ok(data) => ActionResult::success(id, data),
        Err(FlowAppendError::Refused { code, message }) => flow_refusal_result(id, FlowRefusal { code, message }),
        Err(FlowAppendError::Io(e)) => ActionResult::failure(
            Some(id),
            match e.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                io::ErrorKind::AlreadyExists | io::ErrorKind::InvalidData => {
                    ResultStatus::VerificationFailure
                }
                _ => ResultStatus::InternalFailure,
            },
            e.to_string(),
            Some(
                json!({"outcome": if e.kind() == io::ErrorKind::PermissionDenied {"refused"} else if e.kind() == io::ErrorKind::AlreadyExists {"conflict"} else {"error"}}),
            ),
        ),
    }
}

/// Plain text of an entry body: tags dropped, block breaks kept as newlines,
/// the common entities decoded. Bodies are authored HTML; a reader that feeds a
/// model wants words, not markup, and never scripts.
/// Seconds since the epoch for an RFC 3339 UTC time, or None when it cannot be read.
fn flow_time(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let clock = rest.trim_end_matches('Z').split(['+', '.']).next()?;
    let mut c = clock.split(':');
    let (h, mi, sec): (i64, i64, i64) = (c.next()?.parse().ok()?, c.next()?.parse().ok()?, c.next().unwrap_or("0").parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + h * 3600 + mi * 60 + sec)
}

fn flow_plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.char_indices().peekable();
    let lower = html.to_ascii_lowercase();
    while let Some((i, c)) = chars.next() {
        if c == '<' {
            let rest = &lower[i..];
            let end = rest.find('>').map(|e| i + e + 1).unwrap_or(html.len());
            let tag = &lower[i..end];
            if tag.starts_with("<script") || tag.starts_with("<style") {
                let close = if tag.starts_with("<script") { "</script" } else { "</style" };
                let skip_to = lower[end..].find(close).map(|e| end + e).unwrap_or(html.len());
                let after = lower[skip_to..].find('>').map(|e| skip_to + e + 1).unwrap_or(html.len());
                while chars.peek().is_some_and(|(j, _)| *j < after) {
                    chars.next();
                }
                continue;
            }
            if tag.starts_with("<br") || tag.starts_with("</p") || tag.starts_with("</div") || tag.starts_with("</li") {
                out.push('\n');
            }
            while chars.peek().is_some_and(|(j, _)| *j < end) {
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

/// A participant-scoped reading of a Flow: only what this participant may read
/// (from their history horizon, excluding entries whose audience leaves them
/// out), as plain attributed text, bounded. Private collections — notes,
/// journal, packet, media, embedded metadata — are never part of a reading.
fn flow_reading(doc: &Value, reader_key: Option<&str>, limit: usize) -> Value {
    let participants: Vec<Value> = doc
        .pointer("/meta/participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let entries: Vec<Value> = doc.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
    let by_key = |key: &str| participants.iter().find(|p| str_of(p, "key") == Some(key));
    // A reader this flow does not know, or none at all, reads only what is open to
    // the whole group. Restricted, unrecognised or out-of-belonging is never public.
    let reader = reader_key.and_then(by_key);
    let key = reader.and_then(|p| str_of(p, "key"));
    let horizon = match reader.and_then(|p| str_of(p, "historyFrom")) {
        None => Some(0),
        Some(id) => entries.iter().position(|e| str_of(e, "id") == Some(id)),
    };
    // A departed reader's belonging ends where they left; times that cannot be
    // compared leave nothing after that point theirs.
    let last = match reader.and_then(|p| p.get("left")).filter(|v| !v.is_null()) {
        None => entries.len(),
        Some(left) => match left.get("at").and_then(Value::as_str).and_then(flow_time) {
            None => 0,
            Some(gone) => entries
                .iter()
                .position(|e| str_of(e, "at").and_then(flow_time).is_none_or(|at| at > gone))
                .unwrap_or(entries.len()),
        },
    };
    let (mut before_horizon, mut private_to_others) = (0usize, 0usize);
    let mut visible = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if horizon.is_none() || index < horizon.unwrap_or(0) {
            before_horizon += 1;
            continue;
        }
        if index >= last {
            private_to_others += 1;
            continue;
        }
        let audience_ok = match entry.get("audience") {
            None | Some(Value::Null) => true,
            Some(a) if a.as_str() == Some("group") => true,
            Some(a) => match (key, a.get("keys").and_then(Value::as_array)) {
                (Some(key), Some(keys)) => {
                    str_of(entry, "authorKey") == Some(key)
                        || keys.iter().any(|k| k.as_str() == Some(key))
                }
                // An audience this form does not understand belongs to its author alone.
                (Some(key), None) => str_of(entry, "authorKey") == Some(key),
                (None, _) => false,
            },
        };
        if !audience_ok {
            private_to_others += 1;
            continue;
        }
        let author = str_of(entry, "authorKey").and_then(by_key);
        visible.push((index, entry, author));
    }
    let over_limit = visible.len().saturating_sub(limit);
    let tail = visible.split_off(over_limit);
    let rendered: Vec<Value> = tail
        .into_iter()
        .map(|(index, entry, author)| {
            json!({
                "id": entry.get("id"), "index": index + 1,
                "author_key": entry.get("authorKey"),
                "author_name": author.and_then(|a| str_of(a, "name")).or_else(|| str_of(entry, "author")),
                "author_kind": author.and_then(|a| str_of(a, "kind")),
                "attribution_basis": entry.pointer("/attribution/basis"),
                "at": entry.get("at"),
                "text": flow_plain_text(str_of(entry, "html").unwrap_or("")),
                "relations": relations_of(entry),
                "addressees": entry.get("addressees").cloned().unwrap_or_else(|| json!([])),
                "intent": entry.get("intent"),
                "request_ref": entry.pointer("/request/ref"),
                "basis_revision": entry.get("basisRevision"),
            })
        })
        .collect();
    json!({
        "schema": "central.flow-reading/v1",
        "document_id": doc.pointer("/meta/documentId"),
        "document_revision": doc.pointer("/meta/revision"),
        "format_version": flow_format_version(doc),
        "reader_key": reader_key,
        "participants": participants.iter().map(|p| json!({
            "key": p.get("key"), "name": p.get("name"), "initial": p.get("initial"), "kind": p.get("kind"),
            "role": p.get("role").cloned().unwrap_or_else(|| json!("contributor")),
            "left": p.get("left").is_some_and(|v| !v.is_null()),
            "binding_basis": p.pointer("/binding/basis"),
        })).collect::<Vec<_>>(),
        "entries": rendered,
        "omitted": {"before_horizon": before_horizon, "private_to_others": private_to_others, "over_limit": over_limit},
        "private_collections_included": false,
    })
}

fn flow_read_action(input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let id = "central.flow.read";
    let result = resolve_central_root(context.root_options)
        .map_err(io::Error::other)
        .and_then(|root| {
            let root = root.path.canonicalize()?;
            let loc: CentralPathRef =
                serde_json::from_value(input.get("location").cloned().unwrap_or(Value::Null))?;
            if !loc.path.starts_with("Control/user/flows/") {
                return Err(denied("Flow readings are served only for flow instances under Control/user/flows/"));
            }
            let current = read_file(&root, &loc, crate::files::FileEncoding::Utf8)?;
            let html = decode_content(&current.content, &current.content_encoding)?;
            let html = String::from_utf8(html).map_err(|_| invalid("Flow document is not UTF-8"))?;
            let doc = flow_parse(&html)?;
            let limit = input.get("max_entries").and_then(Value::as_u64).unwrap_or(40).clamp(1, 200) as usize;
            let mut reading = flow_reading(&doc, input.get("participant_key").and_then(Value::as_str), limit);
            reading["location"] = serde_json::to_value(&loc).map_err(io::Error::other)?;
            reading["revision"] = json!(current.revision);
            Ok(reading)
        });
    match result {
        Ok(data) => ActionResult::success(id, data),
        Err(e) => ActionResult::failure(
            Some(id),
            match e.kind() {
                io::ErrorKind::PermissionDenied => ResultStatus::UnavailableCapability,
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
                _ => ResultStatus::InternalFailure,
            },
            e.to_string(),
            None,
        ),
    }
}

pub fn register_flow_read(registry: &mut ActionRegistry) {
    let input = |name: &str, kind: &str, required: bool| ActionInputDefinition {
        name: name.into(),
        input_type: kind.into(),
        required,
        choices: None,
        selection: None,
    };
    registry
        .register(
            ActionDescriptor {
                id: "central.flow.read".into(),
                title: "Read a Flow for one participant".into(),
                description: "Read a v0.4 Flow instance as one participant may read it: from their history horizon, without entries whose audience excludes them, as bounded plain attributed text with typed relations. Notes, journal pages, the packet, media and embedded metadata are never included; omissions are counted, not shown.".into(),
                inputs: vec![input("location", "object", true), input("participant_key", "string", false), input("max_entries", "integer", false)],
                output: ActionOutputDefinition { output_type: "central-flow-reading".into() },
                mutation_class: MutationClass::ReadOnly,
                preview_supported: false,
                required_ports: vec![],
                availability: ActionAvailability { available: true, reason: None },
            },
            (|_, i, c| flow_read_action(i, c)) as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        )
        .expect("unique flow read action");
}

pub fn register_flow_append(registry: &mut ActionRegistry) {
    let input = |name: &str, kind: &str, required: bool| ActionInputDefinition {
        name: name.into(),
        input_type: kind.into(),
        required,
        choices: None,
        selection: None,
    };
    let inputs = vec![
        input("location", "object", true),
        input("operation_ref", "string", true),
        input("expected_document_id", "string", false),
        input("author_key", "string", true),
        input("html", "string", true),
        input("at", "string", true),
        input("addressees", "array", false),
        input("audience", "object", false),
        input("intent", "string", false),
        input("relations", "array", false),
        input("basis_revision", "integer", false),
        input("attribution", "object", false),
        input("entry_id", "string", false),
        input("actor", "string", true),
        input("actor_kind", "string", true),
        input("agent_session_ref", "string", false),
        input("agent_ref", "string", false),
        input("generation", "string", false),
        input("workcell", "string", false),
    ];
    registry
        .register(
            ActionDescriptor {
                id: "central.flow.append".into(),
                title: "Append a Flow contribution".into(),
                description: "Append one distinct, validated contribution to a v0.4 Flow instance under Control/user/flows/ through the ordinary-file CAS. When supplied, expected_document_id pins the exact document UUID on every CAS read and replay. Idempotent by operation_ref: a replay recovers, a different payload under the same ref is refused. Verified attribution requires a host-held native credential; otherwise attribution is declared.".into(),
                inputs,
                output: ActionOutputDefinition { output_type: "central-flow-append".into() },
                mutation_class: MutationClass::LocallyMutating,
                preview_supported: false,
                required_ports: vec![],
                availability: ActionAvailability { available: true, reason: None },
            },
            (|_, i, c| flow_append_action(i, c)) as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        )
        .expect("unique flow append action");
}
