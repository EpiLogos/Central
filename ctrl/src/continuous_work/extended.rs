use super::{authority, migration, source::{self, Scope}};
use crate::action::{ActionExecutionContext, ActionRegistry};
use crate::result::ActionResult;
use serde_json::Value;
use std::io;

/// The parent dispatcher holds the existing root-before-Project source locks.
pub(crate) fn dispatch(scope: &Scope, operation: &str, input: &Value, token: Option<&str>, now: u64) -> io::Result<Value> {
    if operation == "migration_read" { return migration::read(scope,input); }
    let action = match operation {
        "migration_plan" => "central.migration.plan",
        "migration_apply" => "central.migration.apply",
        "migration_recover" => "central.migration.recover",
        "migration_rollback" => "central.migration.rollback",
        _ => return Err(source::invalid("unknown continuous-work operation")),
    };
    let principal = authority::authenticate(scope,token,action,input.get("expected_authority_revision").and_then(Value::as_str),now)?;
    if operation == "migration_plan" { migration::plan(scope,input,&principal,now) }
    else { migration::transition(scope,operation,input,&principal,now) }
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
pub(crate) fn register_actions(registry: &mut ActionRegistry) {
    let transition = &[("plan_ref","string",true),("expected_plan_revision","string",true),("expected_policy_revision","string",true),("expected_authority_revision","string",false)];
    super::register_definitions(registry,&[
        ("central.migration.plan","Plan selected temporal source migration","Persist inspectable exact-source/metadata intent without moving source bytes or resetting a repository.",true,plan,&[("request_id","string",true),("moves","array",true),("expected_policy_revision","string",true),("expected_authority_revision","string",false)]),
        ("central.migration.read","Read temporal migration state","Read the immutable plan and current recovery phase, not an inferred success receipt.",false,read,&[("plan_ref","string",true)]),
        ("central.migration.apply","Apply selected temporal migration","Move exact selected source identities and existing Flow/relation metadata without overwriting destinations.",true,apply,transition),
        ("central.migration.recover","Recover interrupted temporal migration","Complete the recorded exact apply/rollback intent or refuse conflicting human bytes and metadata.",true,recover,transition),
        ("central.migration.rollback","Roll back selected temporal migration","Reverse only exact retained source identities and metadata; newer human edits are never overwritten.",true,rollback,transition),
    ]);
}
