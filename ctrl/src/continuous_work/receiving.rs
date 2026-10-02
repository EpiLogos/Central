//! Receiving extends the existing source-return owner and lock. Acceptance and
//! inclusion are separate native mutations; arrival never edits a human Day.
//!
//! One ledger carries two kinds of Return. A `contribution` proposes one
//! operation on a native Day/Flow/Dialogue document. A `request` asks the
//! owner to decide (a proposal of work, or a question) and targets no
//! document: the owner's decision is recorded on the Return itself, where the
//! producing Agent's NOW reading composes it. The owner is addressed here, not
//! through a second inbox.
use super::{
    authority::{self, Principal},
    documents::{self, ContributionAuthor},
    source::{self, conflict, denied, invalid, text, Scope},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io, path::Path};

const AREA: &str = ".central/source-returns/contributions";
const CONTRIBUTION: &str = "contribution";
const REQUEST: &str = "request";
const BOUNDED_TEXT: usize = 16 * 1024;
/// Operations `documents::edit` can apply. Anything else would be retained at
/// submit and could never be included, so it is refused at arrival.
const DOCUMENT_OPERATIONS: &[&str] = &[
    "entry.add",
    "entry.append",
    "field.append",
    "contribution.patch",
    "contribution.remove",
    "field.set",
    "title.set",
    "summary.set",
    "lifecycle.set",
];
const HUMAN_ONLY_OPERATIONS: &[&str] = &["field.set", "title.set", "summary.set", "lifecycle.set"];
fn contribution_kind() -> String {
    CONTRIBUTION.into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Review {
    reviewer_ref: String,
    authority_ref: String,
    authority_revision: String,
    disposition: String,
    /// The target document's reviewed basis. A request has no document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_revision: Option<String>,
    reviewed_at_unix_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
}
/// What an Agent asks the owner to decide. Plain text only; the owner's
/// answer is recorded on the Return, never written into this request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    kind: String,
    subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    /// The native owner that would realise an accepted proposal (e.g.
    /// `factory`). Absent: acceptance itself settles the proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposed_owner_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposal_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    options: Vec<String>,
}
/// Who actually produced a Return that a credentialed carrier delivered. The
/// carrier stays the authenticated `author`; this is its declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredProducer {
    #[serde(rename = "ref")]
    reference: String,
    actor_kind: String,
    attribution: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Received {
    schema: String,
    return_ref: String,
    scope_ref: String,
    sequence: u64,
    request_digest: String,
    #[serde(default = "contribution_kind")]
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    document_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposed_source_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposal: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    request: Option<Request>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence_refs: Vec<String>,
    /// Exact draft bytes retained from scoped sources at submit. Evidence for
    /// the owner's review, never adopted ground and never Day prose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    declared_producer: Option<DeclaredProducer>,
    /// The message this Return answers or continues (e.g. a Gateway
    /// Communique ref). Opaque: Central keeps the thread, not the transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
    author: ContributionAuthor,
    authority_ref: String,
    authority_revision: String,
    occurred_at_unix_seconds: Option<u64>,
    received_at_unix_seconds: u64,
    now_ref: Option<String>,
    day_ref: Option<String>,
    task_ref: Option<String>,
    run_ref: Option<String>,
    session_ref: Option<String>,
    status: String,
    stale_at_arrival: bool,
    review: Option<Review>,
    inclusion_request: Option<Value>,
    /// Inclusion intents the document owner established were never committed;
    /// kept visible after the Return goes back to review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    abandoned_inclusion_requests: Vec<Value>,
    applied_source_revision: Option<String>,
    last_error: Option<String>,
    /// Seen, not decided. Independent of review, inclusion and Recognition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acknowledgement: Option<Value>,
    /// The proposed owner's act that realised an accepted proposal (e.g. the
    /// Factory Run it created), recorded by the accepting human.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    realisation: Option<Value>,
}
impl Received {
    fn is_request(&self) -> bool {
        self.kind == REQUEST
    }
}
/// Settled Returns no longer hold their NOW open or wait in the owner's
/// Inbox. An accepted proposal that names an owner waits for realisation.
pub(crate) fn settled(item: &Value) -> bool {
    match item["status"].as_str() {
        Some("included" | "rejected" | "cancelled" | "answered") => true,
        Some("accepted") => {
            item["kind"] == REQUEST
                && item["request"]["kind"] == "proposal"
                && item["request"]["proposed_owner_ref"].is_null()
        }
        _ => false,
    }
}
fn path(reference: &str) -> String {
    format!("{AREA}/{}.json", source::key(reference))
}
fn read_record(scope: &Scope, reference: &str) -> io::Result<(Received, String)> {
    let raw = crate::source_safety::read(&scope.root, &path(reference))?;
    let record: Received = serde_json::from_str(&raw)?;
    if record.schema != "central.received-contribution/v1"
        || record.return_ref != reference
        || record.scope_ref != scope.world_ref
    {
        return Err(invalid("receiving identity/scope mismatch"));
    }
    Ok((record, source::revision(&raw)))
}
fn write(scope: &Scope, record: &Received) -> io::Result<String> {
    source::directories(&scope.root, Path::new(AREA))?;
    let raw = source::encoded(record)?;
    if raw.len() > crate::source_safety::MAX_SOURCE {
        return Err(invalid(
            "receiving record exceeds native bounded persistence",
        ));
    }
    crate::file_mutation::atomic_record(
        &scope.root.join(path(&record.return_ref)),
        raw.as_bytes(),
    )?;
    Ok(source::revision(&raw))
}
/// A Return is disclosed only while every source it carries is still readable
/// here: its target document and each retained artifact's origin. Revoking
/// retrieval on any of them withholds the Return rather than leaking its copy.
fn require_disclosure(scope: &Scope, record: &Received) -> io::Result<()> {
    if let Some(source) = &record.source_ref {
        scope.read(source)?;
    }
    for artifact in &record.artifacts {
        scope.read(text(&artifact["source"], "ref")?)?;
    }
    Ok(())
}
fn response(record: &Received, revision: &str) -> Value {
    json!({"schema":"central.receiving-reading/v1","return_ref":record.return_ref,"revision":revision,"record":record,
        "source_changed_by_arrival_or_review":false,"included":record.status=="included","automatic_agent_or_model_invocation":false})
}
fn optional(input: &Value, key: &str) -> io::Result<Option<String>> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= 4096 => {
            Ok(Some(value.clone()))
        }
        _ => Err(invalid(format!(
            "{key} requires bounded non-empty reference text"
        ))),
    }
}
fn next_sequence(scope: &Scope) -> io::Result<u64> {
    source::directories(&scope.root, Path::new(AREA))?;
    let cursor_path = format!("{AREA}/cursor.json");
    let previous = match crate::source_safety::read(&scope.root, &cursor_path) {
        Ok(raw) => {
            let value: Value = serde_json::from_str(&raw)?;
            if value["schema"] != "central.receiving-cursor/v1" {
                return Err(invalid("invalid receiving cursor schema"));
            }
            value["sequence"]
                .as_u64()
                .ok_or_else(|| invalid("invalid receiving sequence"))?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    let next = previous
        .checked_add(1)
        .ok_or_else(|| invalid("receiving sequence exhausted"))?;
    crate::file_mutation::atomic_record(
        &scope.root.join(cursor_path),
        &serde_json::to_vec(&json!({"schema":"central.receiving-cursor/v1","sequence":next}))?,
    )?;
    Ok(next)
}
fn receipt_reference(scope: &Scope, principal: &Principal, producer_key: &str) -> String {
    format!("central:return:{}:{}", scope.world_ref,
        source::key(&format!("{}\n{}", principal.principal_ref, producer_key)))
}
fn request_digest(input: &Value) -> io::Result<String> {
    Ok(source::key(&serde_json::to_string(input)?))
}
/// Recover a producer's existing receipt without repeating arrival. Claimed
/// producer labels never choose the authenticated carrier or its address.
fn read_return(scope: &Scope, input: &Value, token: Option<&str>, now: u64) -> io::Result<Value> {
    let reference = optional(input, "return_ref")?;
    let producer_key = optional(input, "producer_key")?;
    let expected_authority = optional(input, "expected_authority_revision")?;
    let original = match input.get("original_request") {
        None | Some(Value::Null) => None,
        Some(value @ Value::Object(_)) => Some(value),
        _ => return Err(invalid("original_request requires an object or null")),
    };
    if let (Some(key), Some(original)) = (producer_key.as_deref(), original) {
        if text(original, "producer_key")? != key {
            return Err(invalid("original_request producer_key differs from the lookup key"));
        }
    }
    match (reference, producer_key) {
        (Some(reference), None) => {
            if original.is_some() || expected_authority.is_some() {
                return Err(invalid("original_request and expected_authority_revision require the producer_key selector"));
            }
            let (record, revision) = read_record(scope, &reference)?;
            require_disclosure(scope, &record)?;
            Ok(response(&record, &revision))
        }
        (None, Some(producer_key)) => {
            // Current permission to originate this scope's receipt establishes
            // the carrier identity. Current disclosure remains a separate gate.
            let principal = authority::authenticate(scope, token, "central.receiving.submit",
                expected_authority.as_deref(), now)?;
            let reference = receipt_reference(scope, &principal, &producer_key);
            let (record, revision) = read_record(scope, &reference)?;
            if record.author.principal_ref != principal.principal_ref {
                return Err(conflict("Return receipt attribution differs from its authenticated producer"));
            }
            if let Some(original) = original {
                if request_digest(original)? != record.request_digest {
                    return Err(conflict("Return producer key has a different original request"));
                }
            }
            require_disclosure(scope, &record)?;
            let mut reading = response(&record, &revision);
            reading["lookup"] = json!({"selector":"authenticated_producer_key",
                "original_request_verified":original.is_some()});
            Ok(reading)
        }
        _ => Err(invalid("receiving.read requires exactly one return_ref or producer_key selector")),
    }
}
fn submit(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    let reference = receipt_reference(scope, principal, text(input, "producer_key")?);
    let digest = request_digest(input)?;
    match read_record(scope, &reference) {
        Ok((existing, revision)) => {
            if existing.request_digest != digest
                || existing.author.principal_ref != principal.principal_ref
            {
                return Err(conflict(
                    "Return producer key already has different content or attribution",
                ));
            }
            require_disclosure(scope, &existing)?;
            return Ok(response(&existing, &revision));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let now_ref = optional(input, "now_ref")?;
    if let Some(reference) = &now_ref {
        super::placement::read_now(scope, reference)?;
    }
    let day_ref = optional(input, "day_ref")?;
    if let Some(reference) = &day_ref {
        super::temporal::day_read(scope, &json!({"day_ref":reference}))?;
    }
    let summary = bounded_text(input, "summary", 8192)?;
    let evidence_refs: Vec<String> =
        serde_json::from_value(input.get("evidence_refs").cloned().unwrap_or(json!([])))
            .map_err(|_| invalid("evidence_refs must be an array of reference text"))?;
    if evidence_refs.len() > 128
        || evidence_refs
            .iter()
            .any(|r| r.trim().is_empty() || r.len() > 4096)
    {
        return Err(invalid("evidence refs exceed bounded receiving limits"));
    }
    let declared_producer = declared_producer(input, principal)?;
    let artifacts = retain_artifacts(scope, input, principal)?;
    if input
        .get("occurred_at_unix_seconds")
        .is_some_and(|v| !v.is_null() && v.as_u64().is_none())
    {
        return Err(invalid("occurrence time must be an integer or absent"));
    }
    let mut record = Received {
        schema: "central.received-contribution/v1".into(),
        return_ref: reference,
        scope_ref: scope.world_ref.clone(),
        sequence: 0,
        request_digest: digest,
        kind: CONTRIBUTION.into(),
        source_ref: None,
        document_id: None,
        proposed_source_revision: None,
        proposal: None,
        request: None,
        summary,
        evidence_refs,
        artifacts,
        declared_producer,
        reply_to: optional(input, "reply_to")?,
        author: principal.into(),
        authority_ref: principal.authority_ref.clone(),
        authority_revision: principal.authority_revision.clone(),
        occurred_at_unix_seconds: input
            .get("occurred_at_unix_seconds")
            .and_then(Value::as_u64),
        received_at_unix_seconds: now,
        now_ref,
        day_ref,
        task_ref: optional(input, "task_ref")?,
        run_ref: optional(input, "run_ref")?,
        session_ref: optional(input, "session_ref")?,
        status: "pending".into(),
        stale_at_arrival: false,
        review: None,
        inclusion_request: None,
        abandoned_inclusion_requests: Vec::new(),
        applied_source_revision: None,
        last_error: None,
        acknowledgement: None,
        realisation: None,
    };
    if input.get("request").is_some() {
        admit_request(input, principal, &mut record)?;
    } else {
        admit_contribution(scope, input, principal, &mut record)?;
    }
    record.sequence = next_sequence(scope)?;
    let revision = write(scope, &record)?;
    Ok(response(&record, &revision))
}
/// Evidence is retained from the actual scoped SourceRef at the supplied
/// revision. It does not become included Day prose or adopted human ground.
fn retain_artifacts(scope: &Scope, input: &Value, principal: &Principal) -> io::Result<Vec<Value>> {
    let items = match input.get("artifacts") {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(value) => value
            .as_array()
            .ok_or_else(|| invalid("artifacts must be an array"))?,
    };
    if items.len() > 16 {
        return Err(invalid("at most 16 explicit source artifacts per Return"));
    }
    let mut retained = Vec::new();
    let mut total = 0;
    for item in items {
        let reading = scope.read(text(item, "source_ref")?)?;
        if reading.revision.revision != text(item, "expected_revision")? {
            return Err(conflict("draft artifact changed before receiving"));
        }
        total += reading.content.len();
        if total > 512 * 1024 {
            return Err(invalid(
                "retained draft evidence exceeds bounded receiving size",
            ));
        }
        retained.push(
            json!({"source":reading.source,"revision":reading.revision,"content":reading.content,
            "content_sha256":source::key(&reading.content),"submitted_by":principal.principal_ref,
            "declared_original_producer_ref":optional(item,"producer_ref")?,
            "proposed_target_ref":optional(item,"proposed_target_ref")?,
            "standing":"retained-source-evidence-not-human-adoption"}),
        );
    }
    Ok(retained)
}
fn bounded_text(input: &Value, key: &str, limit: usize) -> io::Result<Option<String>> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= limit => {
            Ok(Some(value.clone()))
        }
        _ => Err(invalid(format!("{key} must be bounded non-empty text"))),
    }
}
/// A credential authenticates its carrier. An Agent credential speaks only
/// for itself; a human or native-service carrier may declare the Agent whose
/// work it delivers, and the record keeps both.
fn declared_producer(input: &Value, principal: &Principal) -> io::Result<Option<DeclaredProducer>> {
    let Some(value) = input.get("declared_producer").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let producer: DeclaredProducer = serde_json::from_value(value.clone())
        .map_err(|_| invalid("declared_producer is {ref, actor_kind, attribution}"))?;
    if producer.reference.trim().is_empty() || producer.reference.len() > 4096 {
        return Err(invalid("declared producer requires bounded reference text"));
    }
    if !matches!(producer.actor_kind.as_str(), "agent" | "native-service") {
        return Err(invalid("a declared producer is an agent or native-service"));
    }
    if !matches!(producer.attribution.as_str(), "verified" | "claimed") {
        return Err(invalid(
            "declared producer attribution is verified or claimed",
        ));
    }
    if principal.actor_kind == "agent" && producer.reference != principal.principal_ref {
        return Err(denied(
            "an Agent credential cannot declare a different producer",
        ));
    }
    Ok(Some(producer))
}
fn admit_request(input: &Value, principal: &Principal, record: &mut Received) -> io::Result<()> {
    for document_key in [
        "source_ref",
        "document_id",
        "expected_source_revision",
        "proposal",
    ] {
        if input.get(document_key).is_some() {
            return Err(invalid(
                "a request Return asks the owner to decide and targets no document",
            ));
        }
    }
    let request: Request = serde_json::from_value(input["request"].clone()).map_err(|_| {
        invalid("request is {kind, subject, body?, proposed_owner_ref?, proposal_ref?, options?}")
    })?;
    if request.subject.trim().is_empty()
        || request.subject.len() > 280
        || request.subject.contains('\n')
    {
        return Err(invalid("request subject is one bounded line"));
    }
    if request
        .body
        .as_ref()
        .is_some_and(|b| b.trim().is_empty() || b.len() > BOUNDED_TEXT)
    {
        return Err(invalid("request body must be bounded non-empty text"));
    }
    for reference in [&request.proposed_owner_ref, &request.proposal_ref]
        .into_iter()
        .flatten()
    {
        if reference.trim().is_empty() || reference.len() > 4096 {
            return Err(invalid("request refs require bounded reference text"));
        }
    }
    match request.kind.as_str() {
        "proposal" => {
            if !request.options.is_empty() {
                return Err(invalid(
                    "a proposal is accepted or rejected; options belong to questions",
                ));
            }
        }
        "question" => {
            if request.proposed_owner_ref.is_some() || request.proposal_ref.is_some() {
                return Err(invalid(
                    "a question has no proposed owner; ask a proposal instead",
                ));
            }
            if request.options.len() > 8
                || request
                    .options
                    .iter()
                    .any(|o| o.trim().is_empty() || o.len() > 280)
            {
                return Err(invalid("a question offers at most 8 bounded options"));
            }
        }
        _ => return Err(invalid("request kind is proposal or question")),
    }
    // An Agent's request is answerable from the NOW it was working in; that
    // NOW cannot archive while the owner's decision is outstanding.
    if !principal.is_human() && record.now_ref.is_none() {
        return Err(invalid(
            "an Agent request names the NOW it belongs to (now_ref)",
        ));
    }
    record.kind = REQUEST.into();
    record.request = Some(request);
    Ok(())
}
fn admit_contribution(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    record: &mut Received,
) -> io::Result<()> {
    let (source, document, _) = documents::reading(scope, input)?;
    let proposal = input
        .get("proposal")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| invalid("proposal must be a native document operation object"))?;
    let operation = text(&proposal, "operation")?;
    if !DOCUMENT_OPERATIONS.contains(&operation) {
        return Err(invalid(format!(
            "unsupported native contribution operation {operation}; supported: {}",
            DOCUMENT_OPERATIONS.join(", ")
        )));
    }
    if HUMAN_ONLY_OPERATIONS.contains(&operation) && !principal.is_human() {
        return Err(denied(format!(
            "{operation} is a human document operation; an Agent contribution adds, appends or patches its own part"
        )));
    }
    for reserved in [
        "source_ref",
        "document_id",
        "expected_revision",
        "request_id",
        "project",
        "actor_kind",
        "author_ref",
        "actor",
    ] {
        if proposal.get(reserved).is_some() {
            return Err(invalid("proposal cannot supply owner-resolved identity, basis or authenticated attribution fields"));
        }
    }
    if serde_json::to_vec(&proposal)?.len() > 512 * 1024 {
        return Err(invalid("proposal exceeds bounded receiving size"));
    }
    let expected = text(input, "expected_source_revision")?;
    let stale = source.revision.revision != expected;
    record.source_ref = Some(source.source.source_ref);
    record.document_id = Some(text(&document, "document_id")?.into());
    record.proposed_source_revision = Some(expected.into());
    record.proposal = Some(proposal);
    record.status = if stale { "needs-review" } else { "pending" }.into();
    record.stale_at_arrival = stale;
    Ok(())
}
fn target(record: &Received) -> io::Result<&str> {
    record
        .source_ref
        .as_deref()
        .ok_or_else(|| invalid("a request Return has no target document"))
}
fn checked(scope: &Scope, input: &Value) -> io::Result<Received> {
    let (record, revision) = read_record(scope, text(input, "return_ref")?)?;
    if revision != text(input, "expected_return_revision")? {
        return Err(conflict("receiving record changed since review/selection"));
    }
    require_disclosure(scope, &record)?;
    Ok(record)
}
fn review(scope: &Scope, input: &Value, principal: &Principal, now: u64) -> io::Result<Value> {
    principal.require_human()?;
    let mut record = checked(scope, input)?;
    let disposition = text(input, "disposition")?;
    if disposition == "acknowledged" {
        // Seen, not decided: status, review and inclusion are untouched.
        record.acknowledgement = Some(json!({"principal_ref":principal.principal_ref,
            "authority_ref":principal.authority_ref,"authority_revision":principal.authority_revision,
            "recorded_at_unix_seconds":now}));
        let revision = write(scope, &record)?;
        return Ok(response(&record, &revision));
    }
    if !matches!(
        record.status.as_str(),
        "pending" | "needs-review" | "accepted" | "rejected" | "answered"
    ) {
        return Err(conflict(
            "Return has applied or uncertain effects; review cannot reset them",
        ));
    }
    if disposition == "pending" {
        // Leave it for later: an explicit undecided state, not a rejection.
        record.status = if record.stale_at_arrival {
            "needs-review"
        } else {
            "pending"
        }
        .into();
        record.review = None;
        let revision = write(scope, &record)?;
        return Ok(response(&record, &revision));
    }
    let note = bounded_text(input, "note", BOUNDED_TEXT)?;
    let mut answer = None;
    let source_revision = match (record.request.as_ref(), disposition) {
        (None, "accepted") => {
            let current = scope.read(target(&record)?)?;
            if current.revision.revision != text(input, "expected_source_revision")? {
                return Err(conflict("reviewed target source changed"));
            }
            Some(current.revision.revision)
        }
        (None, "rejected") => record.proposed_source_revision.clone(),
        (None, _) => {
            return Err(invalid(
                "a contribution is accepted, rejected, left pending or acknowledged; inclusion is a separate Action",
            ))
        }
        (Some(request), "accepted" | "rejected") if request.kind == "proposal" => None,
        (Some(request), "answered" | "rejected") if request.kind == "question" => {
            if disposition == "answered" {
                answer = Some(
                    bounded_text(input, "answer", BOUNDED_TEXT)?
                        .ok_or_else(|| invalid("an answered question carries the answer"))?,
                );
            }
            None
        }
        (Some(request), _) => {
            return Err(invalid(if request.kind == "question" {
                "a question is answered or declined (rejected)"
            } else {
                "a proposal is accepted or rejected"
            }))
        }
    };
    record.review = Some(Review {
        reviewer_ref: principal.principal_ref.clone(),
        authority_ref: principal.authority_ref.clone(),
        authority_revision: principal.authority_revision.clone(),
        disposition: disposition.into(),
        source_revision,
        reviewed_at_unix_seconds: now,
        note,
        answer,
    });
    record.status = disposition.into();
    let revision = write(scope, &record)?;
    Ok(response(&record, &revision))
}
fn include(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
    recover: bool,
) -> io::Result<Value> {
    principal.require_human()?;
    let mut record = checked(scope, input)?;
    if record.is_request() {
        return realise(scope, input, principal, now, recover, record);
    }
    if record.status == "included" {
        return Ok(response(&record, text(input, "expected_return_revision")?));
    }
    let review = record
        .review
        .as_ref()
        .ok_or_else(|| denied("unreviewed Return cannot be included"))?;
    if review.disposition != "accepted" || review.reviewer_ref != principal.principal_ref {
        return Err(denied(
            "inclusion requires the authenticated accepting reviewer or a new explicit review",
        ));
    }
    let source_ref = target(&record)?.to_string();
    let reviewed = review
        .source_revision
        .clone()
        .ok_or_else(|| invalid("accepted contribution has no reviewed source basis"))?;
    let request = if recover {
        if !matches!(record.status.as_str(), "including" | "uncertain") {
            return Err(conflict("Return has no interrupted inclusion to recover"));
        }
        record
            .inclusion_request
            .clone()
            .ok_or_else(|| invalid("recorded inclusion intent missing"))?
    } else {
        if record.status != "accepted" {
            return Err(denied(
                "only an explicitly accepted Return may begin inclusion",
            ));
        }
        let current = scope.read(&source_ref)?;
        if current.revision.revision != reviewed
            || current.revision.revision != text(input, "expected_source_revision")?
        {
            return Err(conflict(
                "source changed since explicit acceptance; re-review exact current basis",
            ));
        }
        let mut request = record
            .proposal
            .clone()
            .ok_or_else(|| invalid("contribution Return lost its proposal"))?;
        request["source_ref"] = json!(source_ref);
        request["document_id"] = json!(record.document_id);
        request["expected_revision"] = json!(reviewed);
        // An abandoned intent keeps its key: a later attempt needs its own.
        request["request_id"] = json!(match record.abandoned_inclusion_requests.len() {
            0 => format!("include:{}", record.return_ref),
            n => format!("include:{}:attempt-{}", record.return_ref, n + 1),
        });
        // Inclusion retains the original occurrence and receipt, not the later
        // review clock. An absent occurrence stays absent, never fabricated.
        request["occurred_at_unix_seconds"] = json!(record.occurred_at_unix_seconds);
        request["received_at_unix_seconds"] = json!(record.received_at_unix_seconds);
        request
    };
    record.inclusion_request = Some(request.clone());
    record.status = "including".into();
    record.last_error = None;
    write(scope, &record)?;
    let result = documents::mutate_reviewed(scope, &request, &record.author, principal, now);
    match result {
        Ok(result) => {
            let applied = result["operation_receipt"]["revision"]
                .as_str()
                .or_else(|| result["replayed_operation"]["applied_revision"].as_str())
                .ok_or_else(|| {
                    invalid("native document did not return an actual applied revision")
                })?;
            record.status = "included".into();
            record.applied_source_revision = Some(applied.into());
            record.last_error = None;
            let revision = write(scope, &record)?;
            let mut response = response(&record, &revision);
            response["document_result"] = result;
            Ok(response)
        }
        Err(error) => {
            // Recovery that cannot complete asks the document owner whether the
            // intent committed. Uncommitted: back to review at the current basis,
            // with the failure kept visible, never retargeted silently.
            if recover
                && matches!(
                    documents::settle_interrupted(scope, &request, &record.author, principal),
                    Ok(documents::Interrupted::NotCommitted)
                )
            {
                record.status = "needs-review".into();
                record.abandoned_inclusion_requests.push(request);
                record.inclusion_request = None;
                record.last_error = Some(error.to_string());
                let revision = write(scope, &record)?;
                let mut response = response(&record, &revision);
                response["recovery"] = json!({"intent_committed":false,"returned_to_review":true});
                return Ok(response);
            }
            record.status = "uncertain".into();
            record.last_error = Some(error.to_string());
            write(scope, &record)?;
            Err(io::Error::new(
                error.kind(),
                format!(
                    "inclusion not confirmed; inspect/recover {}: {error}",
                    record.return_ref
                ),
            ))
        }
    }
}
/// An accepted proposal is included where it was proposed to land: with its
/// proposed owner. Central does not call that owner. The accepting human
/// records the owner's own realisation (e.g. the Factory Run its commission
/// created); the ref is the owner's claim, retained with who recorded it.
fn realise(
    scope: &Scope,
    input: &Value,
    principal: &Principal,
    now: u64,
    recover: bool,
    mut record: Received,
) -> io::Result<Value> {
    if recover {
        return Err(invalid(
            "a request Return has no document inclusion to recover",
        ));
    }
    let request = record
        .request
        .clone()
        .ok_or_else(|| invalid("request Return lost its request"))?;
    let owner = request.proposed_owner_ref.clone().ok_or_else(|| {
        invalid("only a proposal naming its owner is realised; acceptance already settled it")
    })?;
    let realisation_ref = text(input, "realisation_ref")?;
    if realisation_ref.len() > 4096 {
        return Err(invalid("realisation ref exceeds bounded reference text"));
    }
    if text(input, "realisation_owner_ref")? != owner {
        return Err(denied(format!(
            "this proposal is realised by its proposed owner {owner}"
        )));
    }
    if record.status == "included" {
        // Exact replay returns the recorded realisation; a different one is a
        // conflict, never a silent retarget.
        if record.realisation.as_ref().map(|r| &r["ref"]) == Some(&json!(realisation_ref)) {
            return Ok(response(&record, text(input, "expected_return_revision")?));
        }
        return Err(conflict("proposal was already realised by a different ref"));
    }
    let review = record
        .review
        .as_ref()
        .ok_or_else(|| denied("an unreviewed proposal cannot be realised"))?;
    if record.status != "accepted"
        || review.disposition != "accepted"
        || review.reviewer_ref != principal.principal_ref
    {
        return Err(denied(
            "realisation requires the authenticated accepting reviewer of an accepted proposal",
        ));
    }
    record.realisation = Some(json!({"ref":realisation_ref,"owner_ref":owner,
        "recorded_by":principal.principal_ref,"recorded_at_unix_seconds":now,
        "standing":"owner-reported-realisation-recorded-by-accepting-human"}));
    record.status = "included".into();
    record.last_error = None;
    let revision = write(scope, &record)?;
    Ok(response(&record, &revision))
}
fn list(scope: &Scope, input: &Value) -> io::Result<Value> {
    let after = input.get("after").and_then(Value::as_u64).unwrap_or(0);
    let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(50);
    if limit == 0 || limit > 200 {
        return Err(invalid("receiving page limit must be 1..200"));
    }
    let open_only = match input.get("open") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(open)) => *open,
        _ => return Err(invalid("open is a boolean filter")),
    };
    let entries = match fs::read_dir(scope.root.join(AREA)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(
                json!({"schema":"central.receiving-page/v1","scope_ref":scope.world_ref,"returns":[],"more":false,"next_after":null,"open_total":0}),
            )
        }
        Err(error) => return Err(error),
    };
    let mut records = Vec::new();
    let mut withheld = 0u64;
    let mut open_total = 0u64;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "cursor.json" || !name.ends_with(".json") {
            continue;
        }
        let raw = crate::source_safety::read(&scope.root, &format!("{AREA}/{name}"))?;
        let record: Received = serde_json::from_str(&raw)?;
        if record.schema != "central.received-contribution/v1"
            || record.scope_ref != scope.world_ref
        {
            return Err(invalid("unexpected record in receiving owner store"));
        }
        if require_disclosure(scope, &record).is_err() {
            withheld += 1;
            continue;
        }
        let row = json!({"return_ref":record.return_ref,"revision":source::revision(&raw),"sequence":record.sequence,"status":record.status,
            "kind":record.kind,"source_ref":record.source_ref,"document_id":record.document_id,"author":record.author,
            "declared_producer":record.declared_producer,"summary":record.summary,
            "request":record.request.as_ref().map(|r| json!({"kind":r.kind,"subject":r.subject,"proposed_owner_ref":r.proposed_owner_ref,"proposal_ref":r.proposal_ref})),
            "acknowledged":record.acknowledgement.is_some(),"artifact_count":record.artifacts.len(),
            "occurred_at_unix_seconds":record.occurred_at_unix_seconds,"received_at_unix_seconds":record.received_at_unix_seconds,
            "now_ref":record.now_ref,"day_ref":record.day_ref,"task_ref":record.task_ref,"run_ref":record.run_ref,"session_ref":record.session_ref});
        let settled = settled(&row);
        if !settled {
            open_total += 1;
        }
        if record.sequence <= after || (open_only && settled) {
            continue;
        }
        let mut row = row;
        row["settled"] = json!(settled);
        records.push(row);
    }
    records.sort_by_key(|record| record["sequence"].as_u64().unwrap_or(0));
    let more = records.len() > limit as usize;
    records.truncate(limit as usize);
    let next = if more {
        records.last().map(|record| record["sequence"].clone())
    } else {
        None
    };
    Ok(
        json!({"schema":"central.receiving-page/v1","scope_ref":scope.world_ref,"returns":records,"more":more,"next_after":next,"withheld_unavailable_sources":withheld,"proposal_bodies_in_page":false,"open_total":open_total}),
    )
}
/// Called before the generic source lock. This preserves the legacy owner's
/// return-lock → source-lock ordering and cannot deadlock against v1 acceptance.
pub(crate) fn dispatch(
    scope: &Scope,
    operation: &str,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let _receiving = crate::source_safety::lock(&scope.root, "source-return.lock")?;
    let _sources = source::lock(scope)?;
    match operation {
        "receiving_list" => return list(scope, input),
        "receiving_read" => return read_return(scope, input, token, now),
        _ => {}
    }
    let action = match operation {
        "receiving_submit" => "central.receiving.submit",
        "receiving_review" => "central.receiving.review",
        "receiving_include" => "central.receiving.include",
        "receiving_recover" => "central.receiving.recover",
        _ => return Err(invalid("unknown native receiving operation")),
    };
    let principal = authority::authenticate(
        scope,
        token,
        action,
        input
            .get("expected_authority_revision")
            .and_then(Value::as_str),
        now,
    )?;
    match operation {
        "receiving_submit" => submit(scope, input, &principal, now),
        "receiving_review" => review(scope, input, &principal, now),
        "receiving_include" => include(scope, input, &principal, now, false),
        _ => include(scope, input, &principal, now, true),
    }
}
