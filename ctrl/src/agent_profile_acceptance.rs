//! Human acceptance of one exact Central AgentProfile source. Generation stays
//! generated/unrecognised in the original record. This separate native receipt
//! records an authenticated human act; it is neither Agency admission nor a
//! model/tool/credential grant. A JSON actor label can never manufacture it.
use crate::action::{ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition, ActionOutputDefinition, ActionRegistry, MutationClass};
use crate::agent_profile_actions::resolve_store;
use crate::agent_profile_store::AgentProfileStore;
use crate::continuous_work::{authority, source::{self, Scope}};
use crate::result::{ActionResult, ResultStatus};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io, path::Path, time::{SystemTime, UNIX_EPOCH}};

pub const REVIEW: &str = "agent-profile.review";
pub const ACCEPT: &str = "agent-profile.accept";
pub const ROSTER: &str = "agent-profile.roster";
const SCHEMA: &str = "central.agent-profile-acceptance/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acceptance {
    pub schema: String,
    pub acceptance_ref: String,
    pub profile_ref: String,
    pub agent_ref: String,
    pub profile_revision: String,
    pub content_digest: String,
    pub scope_ref: String,
    pub principal_ref: String,
    pub authority_ref: String,
    pub authority_revision: String,
    pub accepted_at_unix_seconds: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    scope: String,
    #[serde(default)] project: Option<String>,
    #[serde(default)] profile_ref: Option<String>,
    #[serde(default)] expected_revision: Option<String>,
    #[serde(default)] expected_content_digest: Option<String>,
    #[serde(default)] expected_authority_revision: Option<String>,
}
fn invalid(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, message.into()) }
fn conflict(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::AlreadyExists, message.into()) }
fn digest(text: &str) -> String { format!("sha256:{}", source::key(text)) }
fn receipt_path(profile_ref: &str, content_digest: &str) -> String {
    format!(".central/agent-profile-acceptances/{}-{}.json", source::key(profile_ref), source::key(content_digest))
}
fn profile_source(store: &AgentProfileStore, profile_ref: &str) -> io::Result<(crate::agent_profile_store::AgentProfileReading, String)> {
    let reading = store.read(profile_ref).map_err(|e| invalid(e.to_string()))?;
    let text = crate::source_safety::read(store.owner_root(), &reading.source_path)?;
    let profile: crate::agent_profile::AgentProfile = serde_json::from_str(&text)?;
    if profile != reading.profile { return Err(conflict("AgentProfile changed while being read; review the new source")); }
    if !crate::source_horizon::retrieval_allowed(store.owner_root(), &store.owner_root().join(&reading.source_path)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "AgentProfile source is withheld from Agent retrieval"));
    }
    Ok((reading, digest(&text)))
}
fn review(store: &AgentProfileStore, scope: &Scope, profile_ref: &str) -> io::Result<Value> {
    let (reading, content_digest) = profile_source(store, profile_ref)?;
    let path = receipt_path(profile_ref, &content_digest);
    let acceptance: Option<Acceptance> = match crate::source_safety::read(store.owner_root(), &path) {
        Ok(text) => Some(serde_json::from_str(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(a) = &acceptance {
        if a.schema != SCHEMA || a.profile_ref != profile_ref || a.agent_ref != reading.profile.agent_ref
            || a.profile_revision != reading.profile.revision || a.content_digest != content_digest
            || a.scope_ref != scope.world_ref || a.principal_ref.is_empty() || a.authority_ref.is_empty()
            || a.authority_revision.is_empty() || a.acceptance_ref != format!("agent-profile-acceptance:{}", source::key(&path)) {
            return Err(invalid("Native AgentProfile acceptance is not bound to this exact source"));
        }
    }
    Ok(json!({"schema":"central.agent-profile-review/v1", "profile":reading.profile,
        "source_path":reading.source_path, "content_digest":content_digest,
        "accepted":acceptance.is_some(), "acceptance":acceptance,
        "scope_ref":scope.world_ref, "execution_authority_granted":false,
        "standing_note":"Acceptance recognises this exact reusable Agent definition; source editing invalidates it. Original generated provenance is retained. Agency, model selection, task role and AgentSession remain separate."}))
}

/// Same production handler as the CLI, with an explicit credential channel and
/// controlled clock for tests. Credentials never appear in input/output JSON.
pub fn execute_with_token_at(central: &Path, action: &str, value: &Value, token: Option<&str>, now: u64) -> io::Result<Value> {
    let input: Input = serde_json::from_value(value.clone())?;
    if !matches!(input.scope.as_str(), "root" | "personal" | "project") {
        return Err(invalid("scope must be root or project"));
    }
    let project = if input.scope == "project" {
        Some(input.project.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| invalid("Project scope requires the existing Central Work member"))?)
    } else {
        if input.project.is_some() { return Err(invalid("Root Agent definition must not name a child Project")); }
        None
    };
    let scope = Scope::resolve(central, project)?;
    let store = if project.is_some() { AgentProfileStore::project(&scope.root) } else { AgentProfileStore::personal(&scope.root) };
    if action == ROSTER {
        if input.profile_ref.is_some() { return Err(invalid("Roster does not accept a selected profile")); }
        let mut rows = Vec::new();
        for row in store.list().map_err(|e| invalid(e.to_string()))? {
            rows.push(review(&store, &scope, &row.profile.profile_ref)?);
        }
        return Ok(json!({"schema":"central.agent-profile-roster/v1", "scope_ref":scope.world_ref,"profiles":rows,"execution_authority_granted":false}));
    }
    let profile_ref = input.profile_ref.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| invalid("profile_ref is required"))?;
    if action == REVIEW { return review(&store, &scope, profile_ref); }
    if action != ACCEPT { return Err(invalid("Unknown AgentProfile acceptance operation")); }
    // Order is shared source-authority lock, then one profile lock. Save/remove
    // take only the profile lock. No lock is held while waiting for a human UI.
    let _source = source::lock(&scope)?;
    let _profile = store.mutation_lock(profile_ref).map_err(|e| invalid(e.to_string()))?;
    let principal = authority::authenticate(&scope, token, ACCEPT, input.expected_authority_revision.as_deref(), now)?;
    principal.require_human()?;
    let current = review(&store, &scope, profile_ref)?;
    if input.expected_revision.as_deref() != current["profile"]["revision"].as_str()
        || input.expected_content_digest.as_deref() != current["content_digest"].as_str() {
        return Err(conflict("AgentProfile source differs from the reviewed revision/digest; no acceptance was recorded"));
    }
    if current["accepted"] == true { return Ok(current); }
    let content_digest = current["content_digest"].as_str().ok_or_else(|| invalid("Source digest unavailable"))?;
    let path = receipt_path(profile_ref, content_digest);
    let acceptance = Acceptance {
        schema: SCHEMA.into(), acceptance_ref: format!("agent-profile-acceptance:{}", source::key(&path)),
        profile_ref: profile_ref.into(), agent_ref: current["profile"]["agent_ref"].as_str().ok_or_else(|| invalid("Agent identity unavailable"))?.into(),
        profile_revision: current["profile"]["revision"].as_str().ok_or_else(|| invalid("Profile revision unavailable"))?.into(),
        content_digest: content_digest.into(), scope_ref: scope.world_ref.clone(), principal_ref:principal.principal_ref,
        authority_ref:principal.authority_ref, authority_revision:principal.authority_revision, accepted_at_unix_seconds:now,
    };
    // Immutable evidence lives in Central's native state, not the generated
    // profile directory or a renderer Agent store. Descriptor-relative native
    // publication refuses redirected ancestors and retains interrupted staging.
    source::put_new(store.owner_root(), &path, &source::encoded(&acceptance)?)?;
    review(&store, &scope, profile_ref)
}
fn run(action: &str, value: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let result = (|| {
        // Preserve the existing native scope validation / Project residency law.
        resolve_store(action, value, context).map_err(|result| invalid(format!("{result:?}")))?;
        let root = crate::root::resolve_central_root(context.root_options).map_err(invalid)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs();
        let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
        execute_with_token_at(&root.path, action, value, token.as_deref(), now)
    })();
    match result {
        Ok(data) => ActionResult::success(action, data),
        Err(error) => {
            let (status, code) = match error.kind() {
                io::ErrorKind::PermissionDenied => (ResultStatus::UnavailableCapability, "agent_profile_acceptance_denied"),
                io::ErrorKind::AlreadyExists => (ResultStatus::VerificationFailure, "agent_profile_review_stale"),
                _ => (ResultStatus::InvalidInput, "agent_profile_acceptance_invalid"),
            };
            ActionResult::failure_coded(Some(action), status, code, error.to_string(), Some(json!({"setup":{"owner":"central","section":"native-authority","action":ACCEPT,"credential_channel":"native-protected-process","raw_credentials_accepted":false},"automatic_retry":false})))
        }
    }
}
fn review_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult { run(REVIEW,input,context) }
fn roster_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult { run(ROSTER,input,context) }
fn accept_action(_: &ActionRegistry, input: &Value, context: &ActionExecutionContext<'_>) -> ActionResult { run(ACCEPT,input,context) }
pub fn register(registry: &mut ActionRegistry) {
    for (action,title,mutating,handler) in [
        (REVIEW,"Review exact Agent definition",false,review_action as crate::action::ActionHandler),
        (ROSTER,"Read native reusable Agent roster",false,roster_action as crate::action::ActionHandler),
        (ACCEPT,"Accept reviewed reusable Agent definition",true,accept_action as crate::action::ActionHandler),
    ] {
        let mut inputs = vec![("scope",true),("project",false)];
        if action != ROSTER { inputs.push(("profile_ref",true)); }
        if mutating { inputs.extend([("expected_revision",true),("expected_content_digest",true),("expected_authority_revision",false)]); }
        registry.register(ActionDescriptor {
            id:action.into(),title:title.into(),description:"Central-native exact-source review/acceptance. Acceptance requires the existing authenticated human native-action-authority credential for the exact scope/action. It never changes generated provenance or grants execution, model, tool or child-Agent authority.".into(),
            inputs:inputs.into_iter().map(|(name,required)| ActionInputDefinition{name:name.into(),input_type:"string".into(),required,choices:None,selection:None}).collect(),
            output:ActionOutputDefinition{output_type:if action==ROSTER {"agent-profile-roster"} else {"agent-profile-review"}.into()},
            mutation_class:if mutating {MutationClass::LocallyMutating} else {MutationClass::ReadOnly}, preview_supported:false,
            required_ports:vec![], availability:ActionAvailability{available:true,reason:None}
        },handler).expect("Unique AgentProfile acceptance Action");
    }
}
