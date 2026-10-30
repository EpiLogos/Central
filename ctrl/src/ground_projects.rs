//! A project the person brings into a ground Setup founded (O-I #740; manual §10.11, Setup's Projects step): their grant reaches it,
//! and its agents may write inside it. Setup's Ground Accept keeps the person's authority at `control:root` alone and a placement policy
//! that grants nothing writable (crate::ground_founding), so in a project the person brings in nothing they press authenticates and no
//! agent NOW or repository write is placed. Central extends both, previewed and adopted on the preview's digest, on the held Accept
//! that brings or makes the project:
//!
//! - the one human grant of the authority Setup founded covers `project:<id>`, and holds every Action that takes a project scope
//!   (`authority::PROJECT_AUTHENTICATED_ACTIONS`, and the host-checked `HOST_AUTHENTICATED_ACTIONS`): exactly what it lets its holder do
//!   there, since `authenticate` refuses any other Action at a project;
//! - the placement policy Setup founded grants the project's Work member (`Work/<name>`, a repository grant) writable, and nothing else.
//!
//! Only sources Setup founded are extended: each relation's every recognition is `setup-ground-acceptance:<ref>`. An authority or a
//! placement policy recognised otherwise — the owner's own, hand-authored — is refused by name, and nothing is written: it is changed
//! only by its owner. Each extension appends its own acceptance to the relation's recognition, so the source keeps every Accept
//! that shaped it.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{acceptance, descriptor, failure, ground, input, now, root_of};
use crate::continuous_work::authority::{
    self, AUTHORITY_ROLE, HOST_AUTHENTICATED_ACTIONS, PROJECT_AUTHENTICATED_ACTIONS,
};
use crate::continuous_work::placement::{self, POLICY_ROLE};
use crate::continuous_work::source::{self, Scope};
use crate::continuous_work::temporal;
use crate::ground_founding::{AUTHORITY_SOURCE, PLACEMENT_SOURCE};
use crate::result::ActionResult;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;

pub const PREVIEW: &str = "central.ground.project.preview";
pub const ADOPT: &str = "central.ground.project.adopt";
/// The recognition Setup gives what it founds (crate::ground_founding).
pub(crate) const SETUP_RECOGNITION: &str = "setup-ground-acceptance:";

/// Every Action a grant extended to a project holds there: the project-scoped ones Central authenticates, then the host-checked.
pub fn project_actions() -> Vec<&'static str> {
    PROJECT_AUTHENTICATED_ACTIONS
        .iter()
        .chain(HOST_AUTHENTICATED_ACTIONS)
        .copied()
        .collect()
}

/// The Work member named: one folder of Work, never a path.
fn member(input: &Value) -> io::Result<String> {
    input
        .get("project")
        .and_then(Value::as_str)
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 255
                && !name.starts_with('.')
                && !name.contains('/')
                && !name.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            source::invalid(
                "project names one folder of the ground's Work (its name, never a path)",
            )
        })
}

/// The project's identity: its manifest's where it is made, else the id the Accept will make it with (`project_id`).
fn identity(root: &Path, member: &str, input: &Value) -> io::Result<(String, bool)> {
    let asked = input
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let manifest = root
        .join("Work")
        .join(member)
        .join("ProjectCentral/project.json");
    if manifest.exists() {
        let scope = Scope::resolve(root, Some(member))?;
        let id = scope.relations_id.clone();
        if asked.is_some_and(|asked| asked != id) {
            return Err(source::invalid(format!(
                "Work/{member} is project:{id}, not project:{}",
                asked.unwrap_or_default()
            )));
        }
        return Ok((id, true));
    }
    let id = asked.ok_or_else(|| {
        source::invalid(format!(
            "Work/{member} is no project yet: name the project_id the Accept makes it with"
        ))
    })?;
    let validation = crate::projectcentral::ProjectCentralManifest::new(id).validate();
    if !validation.valid {
        return Err(source::invalid(validation.errors.join("; ")));
    }
    Ok((id.to_owned(), false))
}

/// One source Setup founded, as it stands: its content, revision and relation.
pub(crate) struct Founded {
    pub(crate) path: &'static str,
    pub(crate) value: Value,
    pub(crate) revision: String,
    pub(crate) recognition: String,
}

/// The source of a role Setup founded, or why Central extends none: a ground keeps none (`unfounded`), or keeps one recognised
/// otherwise (`hand-authored`). The ground's agreement with Setup reads the authority by the same test (crate::ground_agreement).
pub(crate) fn founded(
    scope: &Scope,
    relations: &Value,
    role: &str,
    path: &'static str,
) -> io::Result<Result<Founded, (&'static str, String)>> {
    let related: Vec<&Value> = relations["relations"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .iter()
        .filter(|r| {
            r["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|v| v == role))
        })
        .collect();
    let [relation] = related.as_slice() else {
        return Ok(Err(if related.is_empty() {
            ("unfounded", format!("this ground keeps no {role} source: Setup's Ground Accept keeps one first, and Central extends nothing. Nothing was written."))
        } else {
            ("hand-authored", format!("this ground keeps {} {role} sources: they are reconciled by hand, and Central extends none. Nothing was written.", related.len()))
        }));
    };
    let recognition = relation["recognition"].as_str().unwrap_or_default();
    let setup = !recognition.is_empty()
        && recognition.split(';').all(|part| {
            part.trim()
                .strip_prefix(SETUP_RECOGNITION)
                .is_some_and(|r| !r.trim().is_empty())
        });
    if relation["path"].as_str() != Some(path)
        || relation["provenance"] != "human-adopted"
        || !setup
    {
        return Ok(Err(("hand-authored", format!(
            "this ground's {role} source ({}) is recognised by {}, not by Setup's acceptance: it is changed only by its owner, and Central extends nothing in it. Nothing was written.",
            relation["path"].as_str().unwrap_or("its relation"),
            if recognition.is_empty() { "no one" } else { recognition }
        ))));
    }
    let content = crate::source_safety::read(&scope.root, path)?;
    Ok(Ok(Founded {
        path,
        value: serde_json::from_str(&content)?,
        revision: source::revision(&content),
        recognition: recognition.to_owned(),
    }))
}

/// The one human grant an authority Setup founded holds, at `control:root`; refused by name where it holds any other shape (mended by
/// hand).
pub(crate) fn setup_grant(authority: &Value) -> io::Result<&Value> {
    let grants = authority["grants"].as_array().filter(|_| {
        authority["schema"] == "central.native-action-authority/v1"
            && authority["scope_ref"] == crate::source_horizon::CONTROL_WORLD_REF
    });
    match grants.map(Vec::as_slice) {
        Some([grant]) if grant["actor_kind"] == "human" => Ok(grant),
        _ => Err(source::denied(
            "the authority Setup founded holds one human grant; this one does not: it is mended by hand, and Central extends nothing. Nothing was written.",
        )),
    }
}

/// What extending the person's grant to `scope_ref` writes: the one human grant Setup made, covering the project and holding every
/// project Action. None where it does already.
fn extended_authority(authority: &Value, scope_ref: &str) -> io::Result<Option<Value>> {
    let mut grant = setup_grant(authority)?.clone();
    let mut changed = false;
    let scopes = grant["scope_refs"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid native grant scope_refs"))?;
    if !scopes.iter().any(|s| s == scope_ref) {
        scopes.push(json!(scope_ref));
        changed = true;
    }
    let actions = grant["actions"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid native grant actions"))?;
    for action in project_actions() {
        if !actions.iter().any(|a| a == action) {
            actions.push(json!(action));
            changed = true;
        }
    }
    Ok(changed.then(|| {
        let mut written = authority.clone();
        written["grants"] = json!([grant]);
        written
    }))
}

/// What granting the project's Work member writes into the placement policy Setup founded: its writable bounds and that member.
fn extended_placement(policy: &Value, path: &str) -> io::Result<Option<Value>> {
    let writable = policy["writable"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid placement policy writable bounds"))?;
    if writable.iter().any(|grant| grant["path"] == path) {
        return Ok(None);
    }
    let mut written = policy.clone();
    written["writable"]
        .as_array_mut()
        .expect("writable is an array")
        .push(json!({"path":path,"class":"repository"}));
    Ok(Some(written))
}

/// The extension, as it would be written now.
struct Plan {
    value: Value,
    writes: Vec<(Founded, Value)>,
    digest: Option<String>,
    state: &'static str,
}

fn plan(root: &Path, input: &Value) -> io::Result<Plan> {
    let member = member(input)?;
    let path = format!("Work/{member}");
    let Some(scope) = ground(root)? else {
        return Ok(Plan {
            value: json!({"schema":"central.ground-project-preview/v1","state":"no-ground","project":member,"member":path,
                "reason":"there is no ground here yet: make the ground first","digest":null,"automatic_agent_or_model_invocation":false}),
            writes: vec![],
            digest: None,
            state: "no-ground",
        });
    };
    let (id, made) = identity(root, &member, input)?;
    let scope_ref = format!("project:{id}");
    let (relations, _) = scope.relations()?;
    let mut value = json!({"schema":"central.ground-project-preview/v1","project":member,"project_id":id,"project_made":made,
        "scope_ref":scope_ref,"member":path,"grant":{"scope_ref":scope_ref,"actions":project_actions()},
        "writable":{"path":path,"class":"repository"},"automatic_agent_or_model_invocation":false});
    let mut writes = Vec::new();
    for (role, source_path) in [
        (AUTHORITY_ROLE, AUTHORITY_SOURCE),
        (POLICY_ROLE, PLACEMENT_SOURCE),
    ] {
        let founded = match founded(&scope, &relations, role, source_path)? {
            Ok(founded) => founded,
            Err((state, reason)) => {
                value["state"] = json!(state);
                value["reason"] = json!(reason);
                value["digest"] = Value::Null;
                return Ok(Plan {
                    value,
                    writes: vec![],
                    digest: None,
                    state,
                });
            }
        };
        let written = if role == AUTHORITY_ROLE {
            extended_authority(&founded.value, &scope_ref)?
        } else {
            extended_placement(&founded.value, &path)?
        };
        value["extends"][if role == AUTHORITY_ROLE {
            "authority"
        } else {
            "placement"
        }] = json!(written.is_some());
        if role == AUTHORITY_ROLE {
            value["grant"]["principal_ref"] = founded.value["grants"][0]["principal_ref"].clone();
        }
        if let Some(written) = written {
            writes.push((founded, written));
        }
    }
    if writes.is_empty() {
        value["state"] = json!("present");
        value["reason"] = json!(format!("you act as you in {member} already, and its agents may write inside it: nothing is extended twice"));
        value["digest"] = Value::Null;
        return Ok(Plan {
            value,
            writes,
            digest: None,
            state: "present",
        });
    }
    // The digest covers the project, and each source extended as it stands (content and recognition): either moving refuses the adopt.
    let basis = writes
        .iter()
        .map(|(f, _)| format!("{}\n{}\n{}", f.path, f.revision, f.recognition))
        .collect::<Vec<_>>()
        .join("\n");
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{path}\n{scope_ref}\n{basis}").as_bytes())
    );
    value["state"] = json!("absent");
    value["reason"] = Value::Null;
    value["digest"] = json!(digest);
    Ok(Plan {
        value,
        writes,
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
    let plan = plan(root, input)?;
    let reason = plan.value["reason"].as_str().unwrap_or_default().to_owned();
    match plan.state {
        "absent" => {}
        "present" => return Err(source::conflict(format!("{reason}. Nothing was written."))),
        _ => return Err(source::denied(reason)),
    }
    if plan.value["project_made"] != true {
        return Err(source::conflict(format!(
            "{} is no project yet: Central extends nothing to it until it is made. Nothing was written.",
            plan.value["member"].as_str().unwrap_or_default()
        )));
    }
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if plan.digest.as_deref() != Some(expected) {
        return Err(source::conflict(
            "what Central would extend changed since its preview (or no preview was named): preview it again. Nothing was written.",
        ));
    }
    let (mut relations, basis) = scope.relations()?;
    let mut refs = Vec::new();
    for (founded, written) in &plan.writes {
        crate::source_safety::replace(
            &scope.root,
            founded.path,
            &founded.revision,
            &source::encoded(written)?,
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
        refs.push(scope.source_ref(founded.path));
    }
    temporal::save_relations(&scope, &relations, &basis)?;
    scope.reconcile(Some(("human:setup-acceptance", "human", None)), &refs)?;
    // Read back through the owners: the grant covers the project, and the project's effective placement grants its member.
    let member = plan.value["project"].as_str().unwrap_or_default();
    let scope_ref = plan.value["scope_ref"].as_str().unwrap_or_default();
    let root_scope = Scope::resolve(root, None)?;
    let (reading, _, authority_revision) =
        authority::recognised_source(&root_scope, AUTHORITY_ROLE)?;
    let kept: Value = serde_json::from_str(&reading.content)?;
    let grant = &kept["grants"][0];
    if !grant["scope_refs"]
        .as_array()
        .is_some_and(|s| s.iter().any(|s| s == scope_ref))
        || project_actions().iter().any(|a| {
            !grant["actions"]
                .as_array()
                .is_some_and(|l| l.iter().any(|x| x == a))
        })
    {
        return Err(io::Error::other(
            "the person's grant does not read back extended to the project",
        ));
    }
    let project = Scope::resolve(root, Some(member))?;
    let effective = placement::effective_policy(&project, now())?;
    let bounds = project.root.clone();
    if !effective
        .writable_destinations
        .iter()
        .any(|d| d.path == bounds)
    {
        return Err(io::Error::other(
            "the project's placement does not read back with its member writable",
        ));
    }
    Ok(
        json!({"adopted":true,"project":member,"scope_ref":scope_ref,"member":plan.value["member"],
        "authority_revision":authority_revision,"policy_revision":effective.revision,"actions":project_actions(),
        "writable":effective.writable_destinations.iter().map(|d| d.path.strip_prefix(&project.central_root).map(|p| p.display().to_string()).unwrap_or_default()).collect::<Vec<_>>()}),
    )
}

fn preview_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match plan(&root, input) {
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

pub(crate) fn register_ground_project_actions(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    let actions: [(_, Handler); 2] = [
        (
            descriptor(
                PREVIEW,
                "Preview extending the person's grant and the ground's bounds to a project",
                "Say what extending this ground's authority and placement to a project of its Work would write: the one human grant Setup founded covering project:<id> with every Action that takes a project scope, and the placement policy Setup founded granting Work/<project> writable, and the digest central.ground.project.adopt is pressed on. project names the Work member; project_id the id it is made with, where it is no project yet. An authority or placement policy Setup did not found (its recognition is not setup-ground-acceptance) is named and never changed. Read-only.",
                MutationClass::ReadOnly,
                "ground-project-preview",
                vec![input("project", true), input("project_id", false)],
            ),
            preview_action,
        ),
        (
            descriptor(
                ADOPT,
                "Extend the person's grant and the ground's bounds to a project",
                "Once the project is made, write the previewed extension: the person's grant covers project:<id> with every Action that takes a project scope, and Work/<project> is writable, nothing else; each relation's recognition gains the person's acceptance acceptance_ref names. Read back through the authority and the project's effective placement. A source Setup did not found is refused by name and nothing is written. Requires a declared human caller and acceptance human-accepted.",
                MutationClass::LocallyMutating,
                "ground-project-receipt",
                vec![
                    input("actor_kind", true),
                    input("acceptance", true),
                    input("acceptance_ref", true),
                    input("expected_digest", true),
                    input("project", true),
                    input("project_id", false),
                ],
            ),
            adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("ground project Action ids are valid");
    }
}
