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
// idempotency record: `entry.request = {ref, digest}`. The shared conformance
// cases live in `ctrl/tests/fixtures/plural-flow-cases.json`.
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
fn flow_parse(html: &str) -> io::Result<Value> {
    let (start, end) = flow_island_span(html)?;
    let raw = html[start..end]
        .replace("<\\/script", "</script")
        .replace("<\\!--", "<!--");
    serde_json::from_str(&raw).map_err(|e| invalid(&format!("Flow document state is not valid JSON: {e}")))
}
fn flow_embed(html: &str, doc: &Value) -> io::Result<String> {
    let (start, end) = flow_island_span(html)?;
    let mut json = serde_json::to_string(doc).map_err(io::Error::other)?;
    // `</script` and `<!--` must not survive inside the island; both escapes
    // are valid JSON string escapes, so the island stays parseable anywhere.
    let mut out = String::with_capacity(json.len());
    let lower = json.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("</script") {
        out.push_str(&json[at..at + i]);
        out.push_str("<\\/script");
        at += i + "</script".len();
    }
    out.push_str(&json[at..]);
    json = out.replace("<!--", "<\\u0021--");
    Ok(format!("{}{}{}", &html[..start], json, &html[end..]))
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
    on_behalf_of: Option<&Value>,
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
            if let Some(v) = on_behalf_of {
                attribution.insert("onBehalfOf".into(), v.clone());
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
        if existing.pointer("/request/digest").and_then(Value::as_str) != Some(digest.as_str()) {
            return refuse(
                "request-conflict",
                format!("operation {operation_ref} already recorded with a different payload"),
            );
        }
        return Ok((doc.clone(), existing.clone(), FlowAppend::Recovered));
    }
    let claimed = request.pointer("/attribution/basis").and_then(Value::as_str);
    let on_behalf = request.pointer("/attribution/onBehalfOf");
    let attribution = flow_check_caller(author, caller, claimed, on_behalf)?;
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
    let revision = doc.pointer("/meta/revision").and_then(Value::as_i64).unwrap_or(0);
    for relation in &relations {
        let kind = str_of(relation, "type").unwrap_or("");
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
            .and_then(Value::as_i64)
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
    entry.insert("attribution".into(), attribution.clone());
    entry.insert("request".into(), json!({"ref": operation_ref, "digest": digest}));
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
        .expect("entries checked above")
        .push(entry.clone());
    next["meta"]["revision"] = json!(revision + 1);
    // A caller this owner authenticated binds its participant on first write.
    if str_of(&attribution, "basis") == Some("verified") {
        let identity = if caller.kind == "agent" {
            caller.session.as_deref().or(caller.reference.as_deref())
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
        let (next, entry, outcome) = match flow_append(&doc, &request, &caller) {
            Ok(done) => done,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let revision = json!({"revision": current.revision});
        if matches!(outcome, FlowAppend::Recovered) {
            return Ok(Ok(json!({
                "schema": "central.flow-append/v1", "outcome": "recovered", "location": loc_value,
                "revision": revision["revision"], "entry": entry, "changed": false,
                "attribution": entry.get("attribution"), "automatic_agent_or_model_invocation": false,
            })));
        }
        let content = flow_embed(&html, &next)?;
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
                    "previous_revision": current.revision, "revision": written["revision"],
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
    let reader = reader_key.and_then(by_key);
    let horizon = reader
        .and_then(|p| str_of(p, "historyFrom"))
        .and_then(|id| entries.iter().position(|e| str_of(e, "id") == Some(id)))
        .unwrap_or(0);
    let (mut before_horizon, mut private_to_others) = (0usize, 0usize);
    let mut visible = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if index < horizon {
            before_horizon += 1;
            continue;
        }
        let audience_ok = match (reader_key, entry.get("audience")) {
            (Some(key), Some(audience)) if audience.is_object() => {
                str_of(entry, "authorKey") == Some(key)
                    || audience
                        .get("keys")
                        .and_then(Value::as_array)
                        .is_some_and(|keys| keys.iter().any(|k| k.as_str() == Some(key)))
            }
            _ => true,
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
                description: "Append one distinct, validated contribution to a v0.4 Flow instance under Control/user/flows/ through the ordinary-file CAS. Idempotent by operation_ref: a replay recovers, a different payload under the same ref is refused. Verified attribution requires a host-held native credential; otherwise attribution is declared.".into(),
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
