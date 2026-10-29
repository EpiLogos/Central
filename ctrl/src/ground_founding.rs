//! A new ground's founding sources, adopted on the person's acceptance (O-I #740; manual §10.11 Ground): the person's native-action
//! authority and the ground's root placement policy. A ground Setup makes (`ctrl init`) keeps neither, so nothing in it authenticates
//! (`continuous_work::authority::authenticate`) and no document can be placed (`continuous_work::placement::effective_policy`): its
//! Day cannot be made. Central adopts neither by itself; each is previewed and adopted on the preview's digest, at first establishment
//! only — a ground that keeps one (its source file, or any relation of its role) is refused by name, and nothing is written.
//!
//! - `central.ground.authority.preview` / `.adopt`: one human grant, at `control:root`, of exactly
//!   `authority::AUTHENTICATED_ACTIONS` (every Action Central authenticates; `authenticate` refuses any other), to the principal
//!   named. Only the digest of the person's credential crosses (`token_sha256`); the host makes and keeps the credential itself
//!   (the O:I kernel's ground_authority route). The receipt carries no digest.
//! - `central.ground.placement.preview` / `.adopt`: the root placement policy whose bounds are the ground itself: nothing writable
//!   granted (a root grant names a Work member, and a new ground's Work is empty; a Day's and a document's placement needs only a
//!   recognised policy), the ground's own Control sources protected, enforced by native actions.
//!
//! - The ground's Flows home (`FLOWS_HOME`, O-I #742): `Control/user/flows`, the one place inside the protected `Control/user` where a
//!   document is created (`file_mutation`'s flow-instance law) — the person's Flows, and the works the O:I Expression owner keeps (a walk,
//!   a kept view). Founded with the placement policy (`central.ground.placement.adopt`), on the person's Setup acceptance; `ctrl init`
//!   imposes nothing below the Control roots. A ground founded before gets it on first need through `central.flows.ensure` — made when
//!   absent, nothing that stands moved — whose answer names its location as `central.files.list` discloses it, the `parent` of a first
//!   `central.files.create` into it. Ensure needs no acceptance (it adds an empty folder the ground's own Flows already write into) and
//!   refuses a folder that is not a ground.
//!
//! The authority and the placement both require a declared human caller and acceptance (`civil_time_adoption::acceptance`), and are recognised by the acceptance
//! named: provenance `human-adopted`, standing `architecture-contract`, recognition `setup-ground-acceptance:<acceptance_ref>`.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{acceptance, descriptor, failure, ground, input, now, root_of};
use crate::continuous_work::authority::{self, AUTHENTICATED_ACTIONS, AUTHORITY_ROLE};
use crate::continuous_work::placement::{self, POLICY_ROLE, POLICY_SCHEMA};
use crate::continuous_work::source::{self, Scope};
use crate::continuous_work::temporal;
use crate::result::ActionResult;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

pub const AUTHORITY_SOURCE: &str = "Control/user/native-action-authority.json";
/// The ground's Flows home (module doc).
pub const FLOWS_HOME: &str = "Control/user/flows";
pub const PLACEMENT_SOURCE: &str = "Control/user/placement.json";
/// The person's grant stands until they replace it: 2100-01-01T00:00:00Z (a grant must name its expiry).
pub const GRANT_EXPIRES_AT: u64 = 4_102_444_800;
/// The ground's own sources a placement policy protects, beside what placement always protects (`.central`, `Control/user`, the
/// relations): its agent governance and wiki, and its machines.
const PROTECTED: [&str; 5] = [
    "Control/user",
    "Control/relations",
    "Control/agents/governance",
    "Control/agents/wiki",
    "Control/machines",
];

/// One founding source: where it is written, and the role its relation names.
struct Founding {
    path: &'static str,
    role: &'static str,
    preview_schema: &'static str,
}
const AUTHORITY: Founding = Founding {
    path: AUTHORITY_SOURCE,
    role: AUTHORITY_ROLE,
    preview_schema: "central.ground-authority-preview/v1",
};
const PLACEMENT: Founding = Founding {
    path: PLACEMENT_SOURCE,
    role: POLICY_ROLE,
    preview_schema: "central.ground-placement-preview/v1",
};

/// Why the ground keeps a source of this kind already (any relation of its role, or its file), or None.
fn standing(scope: &Scope, founding: &Founding) -> io::Result<Option<String>> {
    let (relations, _) = scope.relations()?;
    let related = relations["relations"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .iter()
        .find(|relation| {
            relation["roles"]
                .as_array()
                .is_some_and(|roles| roles.iter().any(|role| role == founding.role))
        });
    if let Some(relation) = related {
        return Ok(Some(format!(
            "this ground keeps a {} source already ({}): Central adopts none over it, and nothing was written",
            founding.role,
            relation["path"].as_str().unwrap_or("its relation")
        )));
    }
    match crate::source_safety::read(&scope.root, founding.path) {
        Ok(_) => Ok(Some(format!(
            "{} stands in this ground already: Central adopts no {} over it, and nothing was written",
            founding.path, founding.role
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn digest(founding: &Founding, document: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}\n{document}", founding.path).as_bytes())
    )
}

/// The human principal the grant is made to.
fn principal(input: &Value) -> io::Result<String> {
    input
        .get("principal_ref")
        .and_then(Value::as_str)
        .filter(|p| {
            p.strip_prefix("human:")
                .is_some_and(|name| !name.trim().is_empty())
                && p.len() <= 512
                && !p.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            source::invalid("principal_ref names the human the grant is made to (human:…, at most 512 characters)")
        })
}
/// The grant, as previewed: everything but the credential's digest.
fn grant(principal: &str) -> Value {
    json!({"principal_ref":principal,"actor_kind":"human","scope_refs":[crate::source_horizon::CONTROL_WORLD_REF],
        "actions":AUTHENTICATED_ACTIONS,"expires_at_unix_seconds":GRANT_EXPIRES_AT})
}
fn token_digest(input: &Value) -> io::Result<String> {
    input
        .get("token_sha256")
        .and_then(Value::as_str)
        .filter(|d| {
            d.len() == 64
                && d.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            source::invalid("token_sha256 is the lowercase hex sha256 of the person's credential, never the credential")
        })
}
fn placement_policy() -> Value {
    json!({"schema":POLICY_SCHEMA,"scope_ref":crate::source_horizon::CONTROL_WORLD_REF,"writable":[],"protected":PROTECTED,
        "enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":3600})
}

fn preview(root: &Path, founding: &Founding, previewed: Value, key: &str) -> io::Result<Value> {
    let state = match ground(root)? {
        None => ("no-ground", None),
        Some(scope) => match standing(&scope, founding)? {
            Some(reason) => ("present", Some(reason)),
            None => ("absent", None),
        },
    };
    let digest = (state.0 != "present").then(|| digest(founding, &previewed));
    let mut value = json!({"schema":founding.preview_schema,"state":state.0,"source_path":founding.path,"role":founding.role,
        "digest":digest,"reason":state.1,"automatic_agent_or_model_invocation":false});
    value[key] = previewed;
    Ok(value)
}

/// Writes one founding source and its relation, as previewed, then reads it back through its owner.
fn adopt(
    root: &Path,
    input: &Value,
    founding: &Founding,
    previewed: &Value,
    written: &Value,
) -> io::Result<String> {
    let reference = acceptance(input)?;
    let scope = ground(root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "there is no ground here yet: make the ground first",
        )
    })?;
    let source_ref = scope.source_ref(founding.path);
    let at = now();
    let _locks = source::lock(&scope)?;
    if let Some(reason) = standing(&scope, founding)? {
        return Err(source::conflict(reason));
    }
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if digest(founding, previewed) != expected {
        return Err(source::conflict(format!(
            "the {} Central would adopt changed since its preview (or no preview was named): preview it again. Nothing was written.",
            founding.role
        )));
    }
    source::put_new(&scope.root, founding.path, &source::encoded(written)?)?;
    let (mut relations, basis) = scope.relations()?;
    relations["relations"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .push(json!({"ref":source_ref,"path":founding.path,"roles":[founding.role],"provenance":"human-adopted",
            "standing":"architecture-contract","treatment":"projectcentral-user",
            "recognition":format!("setup-ground-acceptance:{reference}"),"recorded_at_unix_seconds":at}));
    temporal::save_relations(&scope, &relations, &basis)?;
    scope.reconcile(
        Some(("human:setup-acceptance", "human", None)),
        std::slice::from_ref(&source_ref),
    )?;
    Ok(source_ref)
}

/// Where a ground keeps the document forms the application gives it (`central.ground.form.*`): a Day is made from its form, and its
/// page reads that form where it stands (`_oi_form_source`), so a ground with no project carrying the forms keeps its own copy here.
pub const FORMS_DIR: &str = "Control/forms";
const FORM_ROLE: &str = "document-form";
fn form_name(input: &Value) -> io::Result<String> {
    input
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| {
            name.len() <= 128
                && name.ends_with(".html")
                && name
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
                && !name.contains("..")
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            source::invalid("name is one form's file name (lowercase letters, digits, - and ., ending .html), never a path")
        })
}
fn form_content(input: &Value) -> io::Result<String> {
    input
        .get("content")
        .and_then(Value::as_str)
        .filter(|c| !c.trim().is_empty() && c.len() <= 4 * 1024 * 1024 && !c.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| source::invalid("content is the form's UTF-8 text, at most 4 MiB"))
}
/// Whether a form stands at this path already: its file, or a relation naming it.
fn form_stands(scope: &Scope, path: &str) -> io::Result<bool> {
    let (relations, _) = scope.relations()?;
    if relations["relations"]
        .as_array()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .iter()
        .any(|relation| relation["path"] == path)
    {
        return Ok(true);
    }
    match crate::source_safety::read(&scope.root, path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
fn form_digest(path: &str, content: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{path}\n{content}").as_bytes())
    )
}
fn form_preview(root: &Path, input: &Value) -> io::Result<Value> {
    let path = format!("{FORMS_DIR}/{}", form_name(input)?);
    let content = form_content(input)?;
    let state = match ground(root)? {
        None => "no-ground",
        Some(scope) if form_stands(&scope, &path)? => "present",
        Some(_) => "absent",
    };
    Ok(
        json!({"schema":"central.ground-form-preview/v1","state":state,"source_path":path,"role":FORM_ROLE,
        "digest":(state != "present").then(|| form_digest(&path, &content)),
        "reason":(state == "present").then(|| format!("{path} stands in this ground already: Central keeps it, and replaces nothing")),
        "automatic_agent_or_model_invocation":false}),
    )
}
fn form_adopt(root: &Path, input: &Value) -> io::Result<Value> {
    let reference = acceptance(input)?;
    let path = format!("{FORMS_DIR}/{}", form_name(input)?);
    let content = form_content(input)?;
    let scope = ground(root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "there is no ground here yet: make the ground first",
        )
    })?;
    let source_ref = scope.source_ref(&path);
    let _locks = source::lock(&scope)?;
    if form_stands(&scope, &path)? {
        return Err(source::conflict(format!(
            "{path} stands in this ground already: Central keeps it, replaces nothing, and nothing was written"
        )));
    }
    let expected = input
        .get("expected_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if form_digest(&path, &content) != expected {
        return Err(source::conflict(
            "the form Central would keep changed since its preview (or no preview was named): preview it again. Nothing was written.",
        ));
    }
    source::put_new(&scope.root, &path, &content)?;
    let (mut relations, basis) = scope.relations()?;
    relations["relations"]
        .as_array_mut()
        .ok_or_else(|| source::invalid("invalid source relations"))?
        .push(json!({"ref":source_ref,"path":path,"roles":[FORM_ROLE],"provenance":"generated-derived",
            "standing":"implementation-fact","treatment":"projectcentral-user",
            "recognition":format!("setup-ground-acceptance:{reference}"),"recorded_at_unix_seconds":now()}));
    temporal::save_relations(&scope, &relations, &basis)?;
    scope.reconcile(
        Some(("human:setup-acceptance", "human", None)),
        std::slice::from_ref(&source_ref),
    )?;
    Ok(json!({"adopted":true,"source_ref":source_ref,"source_path":path}))
}
const FLOWS_ENSURE: &str = "central.flows.ensure";

/// Make the ground's Flows home when it is absent (`Control/user` must stand, a directory reached through no symlink); nothing that
/// stands is moved. Answers whether it was made or stood already, and its location.
pub fn ensure_flows_home(root: &Path) -> io::Result<Value> {
    let root = root.canonicalize()?;
    let user = root.join(crate::projectcentral::ROOT_HUMAN_SOURCE_DIR);
    match fs::symlink_metadata(&user) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Control/user is not a folder of this ground, so it keeps no Flows home",
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "This folder is no ground Central keeps (it has no Control/user): accept your ground in Setup first",
            ))
        }
        Err(error) => return Err(error),
    }
    let home = root.join(FLOWS_HOME);
    let made = match fs::create_dir(&home) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
        Err(error) => return Err(error),
    };
    if !fs::symlink_metadata(&home)?.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Control/user/flows stands in this ground but is not a folder",
        ));
    }
    let location = crate::files::CentralPathRef::new(&root, FLOWS_HOME.to_owned())?;
    Ok(
        json!({"schema":"central.flows-home/v1","outcome":if made {"made"} else {"present"},
        "path":FLOWS_HOME,"location":location}),
    )
}
fn flows_ensure_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(FLOWS_ENSURE, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match ensure_flows_home(&root) {
        Ok(value) => ActionResult::success(FLOWS_ENSURE, value),
        Err(error) => failure(FLOWS_ENSURE, error),
    }
}

const FORM_PREVIEW: &str = "central.ground.form.preview";
const FORM_ADOPT: &str = "central.ground.form.adopt";
fn form_preview_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(FORM_PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match form_preview(&root, input) {
        Ok(value) => ActionResult::success(FORM_PREVIEW, value),
        Err(error) => failure(FORM_PREVIEW, error),
    }
}
fn form_adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(FORM_ADOPT, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match form_adopt(&root, input) {
        Ok(value) => ActionResult::success(FORM_ADOPT, value),
        Err(error) => failure(FORM_ADOPT, error),
    }
}

const AUTHORITY_PREVIEW: &str = "central.ground.authority.preview";
const AUTHORITY_ADOPT: &str = "central.ground.authority.adopt";
const PLACEMENT_PREVIEW: &str = "central.ground.placement.preview";
const PLACEMENT_ADOPT: &str = "central.ground.placement.adopt";

fn authority_preview_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(AUTHORITY_PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match principal(input).and_then(|p| preview(&root, &AUTHORITY, grant(&p), "grant")) {
        Ok(value) => ActionResult::success(AUTHORITY_PREVIEW, value),
        Err(error) => failure(AUTHORITY_PREVIEW, error),
    }
}
fn authority_adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(AUTHORITY_ADOPT, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let result = (|| {
        let person = principal(input)?;
        let token = token_digest(input)?;
        let previewed = grant(&person);
        let mut written_grant = previewed.clone();
        written_grant["token_sha256"] = json!(token);
        let written = json!({"schema":"central.native-action-authority/v1","scope_ref":crate::source_horizon::CONTROL_WORLD_REF,
            "grants":[written_grant]});
        let source_ref = adopt(&root, input, &AUTHORITY, &previewed, &written)?;
        // Read back through the owner: the source is recognised as the ground's one authority.
        let scope = Scope::resolve(&root, None)?;
        let (_, _, revision) = authority::recognised_source(&scope, AUTHORITY_ROLE)?;
        Ok(
            json!({"adopted":true,"source_ref":source_ref,"authority_revision":revision,"principal_ref":person,
            "scope_refs":[crate::source_horizon::CONTROL_WORLD_REF],"actions":AUTHENTICATED_ACTIONS}),
        )
    })();
    match result {
        Ok(value) => ActionResult::success(AUTHORITY_ADOPT, value),
        Err(error) => failure(AUTHORITY_ADOPT, error),
    }
}
fn placement_preview_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(PLACEMENT_PREVIEW, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match preview(&root, &PLACEMENT, placement_policy(), "policy") {
        Ok(value) => ActionResult::success(PLACEMENT_PREVIEW, value),
        Err(error) => failure(PLACEMENT_PREVIEW, error),
    }
}
fn placement_adopt_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(PLACEMENT_ADOPT, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let result = (|| {
        let policy = placement_policy();
        let source_ref = adopt(&root, input, &PLACEMENT, &policy, &policy)?;
        // Read back through the owner: the ground's effective placement resolves on it.
        let effective = placement::effective_policy(&Scope::resolve(&root, None)?, now())?;
        // The ground's Flows home is founded with its bounds: the one place inside Control/user a document is created.
        let flows_home = ensure_flows_home(&root)?;
        Ok(
            json!({"adopted":true,"source_ref":source_ref,"policy_revision":effective.revision,"policy":policy,"flows_home":flows_home}),
        )
    })();
    match result {
        Ok(value) => ActionResult::success(PLACEMENT_ADOPT, value),
        Err(error) => failure(PLACEMENT_ADOPT, error),
    }
}

pub(crate) fn register_ground_founding_actions(registry: &mut ActionRegistry) {
    type Handler = fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult;
    let gate = || {
        vec![
            input("actor_kind", true),
            input("acceptance", true),
            input("acceptance_ref", true),
            input("expected_digest", true),
        ]
    };
    let actions: [(_, Handler); 6] = [
        (
            descriptor(
                FORM_PREVIEW,
                "Preview keeping a document form in the ground",
                "Say whether this ground keeps the named document form (Control/forms/<name>: its file, or a relation naming it), and the digest central.ground.form.adopt is pressed on for the content given. A form that stands is never replaced. Read-only.",
                MutationClass::ReadOnly,
                "ground-form-preview",
                vec![input("name", true), input("content", true)],
            ),
            form_preview_action,
        ),
        (
            descriptor(
                FORM_ADOPT,
                "Keep a document form in the ground",
                "Write the previewed document form at Control/forms/<name> (never replacing one that stands) and its relation (generated-derived, role document-form), recognised by the person's acceptance acceptance_ref names, so a Day can be made from it and its page read it where it stands. Requires a declared human caller and acceptance human-accepted.",
                MutationClass::LocallyMutating,
                "ground-form-receipt",
                [gate(), vec![input("name", true), input("content", true)]].concat(),
            ),
            form_adopt_action,
        ),
        (
            descriptor(
                AUTHORITY_PREVIEW,
                "Preview adopting the person's native-action authority",
                "Say what adopting this ground's native-action authority would write: Control/user/native-action-authority.json with one human grant to principal_ref, at control:root, of exactly the Actions Central authenticates, and the digest central.ground.authority.adopt is pressed on; or that the ground keeps an authority already (its file, or any relation of the role), which is never replaced. Names no credential. Read-only.",
                MutationClass::ReadOnly,
                "ground-authority-preview",
                vec![input("principal_ref", true)],
            ),
            authority_preview_action,
        ),
        (
            descriptor(
                AUTHORITY_ADOPT,
                "Adopt the person's native-action authority",
                "At first establishment only, write the previewed grant with the digest of the person's credential (token_sha256: the credential never crosses) and its source relation, recognised by the person's acceptance acceptance_ref names; read it back as the ground's one recognised authority. A ground that keeps an authority is refused by name and nothing is written. Requires a declared human caller and acceptance human-accepted.",
                MutationClass::LocallyMutating,
                "ground-authority-receipt",
                [gate(), vec![input("principal_ref", true), input("token_sha256", true)]].concat(),
            ),
            authority_adopt_action,
        ),
        (
            descriptor(
                PLACEMENT_PREVIEW,
                "Preview adopting the ground's placement policy",
                "Say what adopting this ground's root placement policy would write: Control/user/placement.json bounded to the ground itself (nothing writable granted outside or inside it yet, its own Control sources protected, native-action enforcement), and the digest central.ground.placement.adopt is pressed on; or that the ground keeps a placement policy already, which is never replaced. Read-only.",
                MutationClass::ReadOnly,
                "ground-placement-preview",
                Vec::new(),
            ),
            placement_preview_action,
        ),
        (
            descriptor(
                PLACEMENT_ADOPT,
                "Adopt the ground's placement policy",
                "At first establishment only, write the previewed placement policy and its source relation, recognised by the person's acceptance acceptance_ref names; read it back as the ground's effective placement; and found the ground's Flows home, Control/user/flows (the receipt's flows_home: made or present, and its location). A ground that keeps a placement policy is refused by name and nothing is written. Requires a declared human caller and acceptance human-accepted.",
                MutationClass::LocallyMutating,
                "ground-placement-receipt",
                gate(),
            ),
            placement_adopt_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("ground founding Action ids are valid");
    }
    registry
        .register(
            descriptor(
                FLOWS_ENSURE,
                "Ensure the ground's Flows home",
                "Make Control/user/flows, the ground's Flows home, when it is absent; nothing that stands is moved. A ground founds it with its placement policy (central.ground.placement.adopt, on the person's Setup acceptance); one founded before gets it here on first need. Answers outcome made or present and the home's location as central.files.list discloses it (the parent of a first central.files.create into it). Refuses a folder that is no ground (no Control/user).",
                MutationClass::LocallyMutating,
                "central-flows-home",
                vec![],
            ),
            flows_ensure_action,
        )
        .expect("ground founding Action ids are valid");
}
