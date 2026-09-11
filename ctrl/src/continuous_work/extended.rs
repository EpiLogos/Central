use super::{authority, documents, migration, source::{self, Scope}};
use crate::action::{ActionExecutionContext, ActionRegistry};
use crate::result::ActionResult;
use serde_json::Value;
use std::io;

/// The parent dispatcher holds existing root-before-Project source locks.
/// Receiving is routed separately before those locks to preserve v1 lock order.
pub(crate) fn dispatch(scope: &Scope, operation: &str, input: &Value, token: Option<&str>, now: u64) -> io::Result<Value> {
    match operation {
        "migration_read"=>return migration::read(scope,input),
        "document_read"=>return documents::read(scope,input),
        "document_export"=>return documents::export(scope,input),
        _=>{},
    }
    let action = match operation {
        "migration_plan" => "central.migration.plan",
        "migration_apply" => "central.migration.apply",
        "migration_recover" => "central.migration.recover",
        "migration_rollback" => "central.migration.rollback",
        "document_create" => "central.document.create",
        "document_mutate" => "central.document.mutate",
        _ => return Err(source::invalid("unknown continuous-work operation")),
    };
    let principal = authority::authenticate(scope,token,action,input.get("expected_authority_revision").and_then(Value::as_str),now)?;
    match operation {
        "migration_plan"=>migration::plan(scope,input,&principal,now),
        "document_create"=>documents::create(scope,input,&principal,now),
        "document_mutate"=>documents::mutate(scope,input,&principal,now),
        _=>migration::transition(scope,operation,input,&principal,now),
    }
}
macro_rules! handler {
    ($name:ident,$action:literal,$operation:literal) => {
        fn $name(_: &ActionRegistry,input: &Value,context: &ActionExecutionContext<'_>) -> ActionResult {
            super::execute($action,$operation,input,context)
        }
    };
}
handler!(plan,"central.migration.plan","migration_plan");
handler!(read,"central.migration.read","migration_read");
handler!(apply,"central.migration.apply","migration_apply");
handler!(recover,"central.migration.recover","migration_recover");
handler!(rollback,"central.migration.rollback","migration_rollback");
handler!(document_create,"central.document.create","document_create");
handler!(document_read,"central.document.read","document_read");
handler!(document_mutate,"central.document.mutate","document_mutate");
handler!(document_export,"central.document.export","document_export");
handler!(receiving_submit,"central.receiving.submit","receiving_submit");
handler!(receiving_list,"central.receiving.list","receiving_list");
handler!(receiving_read,"central.receiving.read","receiving_read");
handler!(receiving_review,"central.receiving.review","receiving_review");
handler!(receiving_include,"central.receiving.include","receiving_include");
handler!(receiving_recover,"central.receiving.recover","receiving_recover");
pub(crate) fn register_actions(registry: &mut ActionRegistry) {
    let transition = &[("plan_ref","string",true),("expected_plan_revision","string",true),("expected_policy_revision","string",true),("expected_authority_revision","string",false)];
    let document = &[("source_ref","string",true),("document_id","string",true)];
    let inclusion = &[("return_ref","string",true),("expected_return_revision","string",true),("expected_source_revision","string",false),("expected_authority_revision","string",false)];
    super::register_definitions(registry,&[
        ("central.migration.plan","Plan selected temporal source migration","Persist inspectable exact-source/metadata intent without moving source bytes or resetting a repository.",true,plan,&[("request_id","string",true),("moves","array",true),("expected_policy_revision","string",true),("expected_authority_revision","string",false)]),
        ("central.migration.read","Read temporal migration state","Read the immutable plan and current recovery phase, not an inferred success receipt.",false,read,&[("plan_ref","string",true)]),
        ("central.migration.apply","Apply selected temporal migration","Move exact selected source identities and existing Flow/relation metadata without overwriting destinations.",true,apply,transition),
        ("central.migration.recover","Recover interrupted temporal migration","Complete the recorded exact apply/rollback intent or refuse conflicting human bytes and metadata.",true,recover,transition),
        ("central.migration.rollback","Roll back selected temporal migration","Reverse only exact retained source identities and metadata; newer human edits are never overwritten.",true,rollback,transition),
        ("central.document.create","Create native Day Flow or Dialogue document","Initialise a blank human Day or publish an identity-stable Flow through existing source/history ownership; retain only supplied template keys.",true,document_create,&[("document_id","string",true),("kind","string",true),("title","string",false),("day_ref","string",false),("expected_revision","string",false),("expected_policy_revision","string",true),("template_payload","object",false),("fields","array",false),("expected_authority_revision","string",false)]),
        ("central.document.read","Read native contribution document","Read actual fields, entries, contributions, source revision, last native revision and external-edit protection state.",false,document_read,document),
        ("central.document.mutate","Mutate an authenticated document contribution","Exact-revision entry/field and note operations, reviewed native snapshot restoration and external-source reconciliation; credentials never come from imported payloads.",true,document_mutate,&[("source_ref","string",true),("document_id","string",true),("expected_revision","string",true),("request_id","string",true),("operation","string",true),("entry_id","string",false),("contribution_id","string",false),("field_id","string",false),("html","string",false),("value","object",false),("reply_to","string",false),("occurred_at_unix_seconds","integer",false),("expected_authority_revision","string",false),("expected_native_revision","string",false),("before_entry_id","string",false),("note_id","string",false),("parent_note_id","string",false),("timing","string",false),("anchor","object",false)]),
        ("central.document.export","Export an inert retained document snapshot","Export readable human fields, ordered entries, attribution and notes alongside escaped retained JSON; not an autosave or original-template fidelity claim.",false,document_export,document),
        ("central.receiving.submit","Receive a proposed native contribution","Retain concurrent Returns, concise summaries and exact scoped draft evidence; arrival never changes or adopts human source.",true,receiving_submit,&[("producer_key","string",true),("source_ref","string",true),("document_id","string",true),("expected_source_revision","string",true),("proposal","object",true),("occurred_at_unix_seconds","integer",false),("now_ref","string",false),("day_ref","string",false),("task_ref","string",false),("run_ref","string",false),("session_ref","string",false),("expected_authority_revision","string",false),("summary","string",false),("evidence_refs","array",false),("artifacts","array",false)]),
        ("central.receiving.list","Read scoped receiving aperture","Read scoped summaries or an explicitly selected, credential-scoped root composition by owner refs; no copied prose or proposal bodies.",false,receiving_list,&[("after","integer",false),("limit","integer",false),("projects","array",false),("cursor","object",false),("expected_authority_revision","string",false)]),
        ("central.receiving.read","Read one retained Return","Read receiving revision, original evidence, acknowledgement, review and confirmed or uncertain inclusion after current source-privacy checks.",false,receiving_read,&[("return_ref","string",true)]),
        ("central.receiving.review","Review a received contribution","Authenticated accepted/rejected/pending review or separate explicit acknowledgement; neither review nor acknowledgement is document inclusion or Factory Recognition.",true,receiving_review,&[("return_ref","string",true),("expected_return_revision","string",true),("disposition","string",true),("expected_source_revision","string",false),("expected_authority_revision","string",false)]),
        ("central.receiving.include","Include an explicitly accepted contribution","Apply the accepting human's exact reviewed contribution through document ownership; retain original contributor, occurrence and receipt provenance.",true,receiving_include,inclusion),
        ("central.receiving.recover","Recover uncertain Return inclusion","Resume the recorded native document mutation or preserve a conflict; no receipt is called executed without an applied source revision.",true,receiving_recover,inclusion),
    ]);
}
