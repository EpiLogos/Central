//! A grant Setup founded, brought into agreement with what Setup grants now (O-I #740; manual §10.11: "Run it over an older setup
//! and it brings the setup into agreement"). Setup's Ground Accept grants the person, at `control:root`, exactly the Actions Central
//! authenticates when it runs (`ground_founding::setup_root_actions`). That list grows as Central does; a grant an older Setup founded
//! never gains what came after, so in their own world the person cannot press what Central now authenticates. Central brings it into
//! agreement, previewed and adopted on the preview's digest, on Setup's held Ground Accept:
//!
//! - the one human grant of the authority Setup founded gains the root Actions Setup grants now that it lacks, in the order Setup
//!   grants them, after those it holds; nothing it holds is taken away, and its principal, credential digest, scopes and expiry stay;
//! - only an authority Setup founded is brought into agreement, by the test a project's extension reads it with
//!   (`ground_projects::founded`: its relation's every recognition is `setup-ground-acceptance:<ref>`) and covering the root. One
//!   recognised otherwise — the owner's own, hand-authored — or mended by hand into another shape is refused by name, and nothing is
//!   written: it is changed only by its owner.
//!
//! The agreement appends its own acceptance to the relation's recognition, so the source keeps every Accept that shaped it. A grant a
//! project's extension reached keeps that reach; the project Actions it holds there are the Projects step's to finish
//! (crate::ground_projects).
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{acceptance, descriptor, failure, ground, input, root_of};
use crate::continuous_work::authority::{self, AUTHORITY_ROLE};
use crate::continuous_work::source::{self, Scope};
use crate::continuous_work::temporal;
use crate::ground_founding::{setup_root_actions, AUTHORITY_SOURCE};
use crate::ground_projects::{founded, setup_grant, Founded, SETUP_RECOGNITION};
use crate::result::ActionResult;
use crate::source_horizon::CONTROL_WORLD_REF;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;

pub const PREVIEW: &str = "central.ground.agreement.preview";
pub const ADOPT: &str = "central.ground.agreement.adopt";
const SCHEMA: &str = "central.ground-agreement-preview/v1";

/// The agreement, as it would be written now: the preview, and the grant written over the source Setup founded.
struct Plan {
    value: Value,
    write: Option<(Founded, Value)>,
    adds: Vec<&'static str>,
    digest: Option<String>,
    state: &'static str,
}

fn without(value: Value, state: &'static str, reason: String) -> Plan {
    let mut value = value;
    value["state"] = json!(state);
    value["reason"] = json!(reason);
    value["digest"] = Value::Null;
    Plan {
        value,
        write: None,
        adds: vec![],
        digest: None,
        state,
    }
}

fn plan(root: &Path) -> io::Result<Plan> {
    let value = json!({"schema":SCHEMA,"source_path":AUTHORITY_SOURCE,"scope_ref":CONTROL_WORLD_REF,"actions":setup_root_actions(),
        "adds":[],"automatic_agent_or_model_invocation":false});
    let Some(scope) = ground(root)? else {
        return Ok(without(
            value,
            "no-ground",
            "there is no ground here yet: make the ground first".into(),
        ));
    };
    let (relations, _) = scope.relations()?;
    let founded = match founded(&scope, &relations, AUTHORITY_ROLE, AUTHORITY_SOURCE)? {
        Ok(founded) => founded,
        Err((state, reason)) => return Ok(without(value, state, reason)),
    };
    let grant = match setup_grant(&founded.value) {
        Ok(grant) => grant.clone(),
        Err(error) => return Ok(without(value, "hand-authored", error.to_string())),
    };
    if !grant["scope_refs"]
        .as_array()
        .is_some_and(|scopes| scopes.iter().any(|s| s == CONTROL_WORLD_REF))
    {
        return Ok(without(value, "hand-authored", format!(
            "the grant Setup founded no longer covers {CONTROL_WORLD_REF}: it is mended by hand, and Central brings nothing in it into agreement. Nothing was written."
        )));
    }
    let held = grant["actions"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid native grant actions"))?;
    let adds: Vec<&'static str> = setup_root_actions()
        .iter()
        .copied()
        .filter(|action| !held.iter().any(|a| a == action))
        .collect();
    let mut value = value;
    value["principal_ref"] = grant["principal_ref"].clone();
    value["adds"] = json!(adds);
    if adds.is_empty() {
        return Ok(without(
            value,
            "present",
            "your grant holds every Action Setup grants at the root now: nothing is added twice"
                .into(),
        ));
    }
    let mut written_grant = grant;
    written_grant["actions"]
        .as_array_mut()
        .expect("actions is an array")
        .extend(adds.iter().map(|action| json!(action)));
    let mut written = founded.value.clone();
    written["grants"] = json!([written_grant]);
    // The digest covers the source as it stands (content and recognition) and what is added: either moving refuses the adopt.
    let digest = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\n{}\n{}\n{}",
                founded.path,
                founded.revision,
                founded.recognition,
                adds.join("\n")
            )
            .as_bytes()
        )
    );
    value["state"] = json!("absent");
    value["reason"] = Value::Null;
    value["digest"] = json!(digest);
    Ok(Plan {
        value,
        write: Some((founded, written)),
        adds,
        digest: Some(digest),
        state: "absent",
    })
}

fn adopt(root: &Path, input: &Value) -> io::Result<Value> {
    let reference = acceptance(input)?;
    let scope = ground(root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "there is no ground here yet: make the ground first",
        )
    })?;
    let _locks = source::lock(&scope)?;
    let plan = plan(root)?;
    let reason = plan.value["reason"].as_str().unwrap_or_default().to_owned();
    match plan.state {
        "absent" => {}
        "present" => return Err(source::conflict(format!("{reason}. Nothing was written."))),
        _ => return Err(source::denied(reason)),
    }
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if plan.digest.as_deref() != Some(expected) {
        return Err(source::conflict(
            "what Central would bring into agreement changed since its preview (or no preview was named): preview it again. Nothing was written.",
        ));
    }
    let (founded, written) = plan.write.expect("an absent agreement writes");
    let (mut relations, basis) = scope.relations()?;
    crate::source_safety::replace(
        &scope.root,
        founded.path,
        &founded.revision,
        &source::encoded(&written)?,
    )?;
    let relation = relations["relations"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .iter_mut()
        .find(|r| r["path"] == founded.path)
        .ok_or_else(|| source::conflict("the founded source's relation moved"))?;
    relation["recognition"] = json!(format!(
        "{};{SETUP_RECOGNITION}{reference}",
        founded.recognition
    ));
    temporal::save_relations(&scope, &relations, &basis)?;
    let source_ref = scope.source_ref(founded.path);
    scope.reconcile(
        Some(("human:setup-acceptance", "human", None)),
        std::slice::from_ref(&source_ref),
    )?;
    // Read back through the owner: the ground's one recognised authority, its grant holding every Action Setup grants at the root.
    let root_scope = Scope::resolve(root, None)?;
    let (reading, _, authority_revision) =
        authority::recognised_source(&root_scope, AUTHORITY_ROLE)?;
    let kept: Value = serde_json::from_str(&reading.content)?;
    let actions = kept["grants"][0]["actions"].as_array();
    if setup_root_actions()
        .iter()
        .any(|a| !actions.is_some_and(|held| held.iter().any(|x| x == a)))
    {
        return Err(io::Error::other(
            "the person's grant does not read back in agreement with what Setup grants",
        ));
    }
    Ok(
        json!({"adopted":true,"source_ref":source_ref,"authority_revision":authority_revision,"added":plan.adds,
        "actions":setup_root_actions()}),
    )
}

fn preview_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match plan(&root) {
        Ok(plan) => ActionResult::success(PREVIEW, plan.value),
        Err(error) => failure(PREVIEW, error),
    }
}
fn adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(ADOPT, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match adopt(&root, input) {
        Ok(value) => ActionResult::success(ADOPT, value),
        Err(error) => failure(ADOPT, error),
    }
}

pub(crate) fn register_ground_agreement_actions(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    let actions: [(_, Handler); 2] = [
        (
            descriptor(
                PREVIEW,
                "Preview bringing the person's grant into agreement with Setup",
                "Say what bringing this ground's grant into agreement with what Setup grants now would write: the one human grant of the authority Setup founded gains the root Actions Setup's Ground Accept grants today that it lacks (adds), nothing it holds taken away, and the digest central.ground.agreement.adopt is pressed on. An authority Setup did not found (its recognition is not wholly setup-ground-acceptance) or mended by hand is named and never changed. Names no credential. Read-only.",
                MutationClass::ReadOnly,
                "ground-agreement-preview",
                Vec::new(),
            ),
            preview_action,
        ),
        (
            descriptor(
                ADOPT,
                "Bring the person's grant into agreement with Setup",
                "Write the previewed agreement: the person's grant gains the root Actions Setup grants now that it lacks, its principal, credential digest, scopes and expiry unchanged; the relation's recognition gains the person's acceptance acceptance_ref names. Read back as the ground's one recognised authority. A source Setup did not found is refused by name and nothing is written. Requires a declared human caller and acceptance human-accepted.",
                MutationClass::LocallyMutating,
                "ground-agreement-receipt",
                vec![
                    input("actor_kind", true),
                    input("acceptance", true),
                    input("acceptance_ref", true),
                    input("expected_digest", true),
                ],
            ),
            adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("ground agreement Action ids are valid");
    }
}
