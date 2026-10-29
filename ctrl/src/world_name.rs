//! `central.world.name`: a project's world named by the person (O-I #743; manual §18.1 "Name: the name on the track"). The name is
//! kept where the world's own identity is, its ProjectCentral manifest (`ProjectCentral/project.json` `name`, beside its stable
//! `project_id`, which never changes here), and the world map discloses it (`world_name`, with the manifest's revision this act is
//! made against), so a world's rename shows wherever the world is named. The person's own act: it authenticates on their native
//! authority at the project. A stale revision answers conflict and writes nothing; the same name answers unchanged.
use crate::action::{ActionExecutionContext, ActionRegistry, MutationClass};
use crate::civil_time_adoption::{descriptor, failure, input, now, root_of};
use crate::continuous_work::{authority, source::Scope};
use crate::file_mutation::{atomic_record, open_native_file, RecordDisposition};
use crate::projectcentral::{parse_project_manifest, world_name_words};
use crate::result::ActionResult;
use crate::source_safety::content_revision_bytes;
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::Path;

pub const WORLD_NAME_ACTION: &str = "central.world.name";
pub const WORLD_NAME_SCHEMA: &str = "central.world-name/v1";
const MANIFEST: &str = "ProjectCentral/project.json";
const MANIFEST_LIMIT: u64 = 1024 * 1024;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// Name the world of `input.project` `input.name`, on the manifest revision `input.expected_revision`, as the person `token` names.
pub fn name_world(
    central: &Path,
    input: &Value,
    token: Option<&str>,
    now: u64,
) -> io::Result<Value> {
    let text = |field: &str| input.get(field).and_then(Value::as_str);
    let project = text("project")
        .filter(|project| !project.trim().is_empty())
        .ok_or_else(|| invalid("central.world.name names the project whose world it names"))?;
    let name = text("name")
        .ok_or_else(|| invalid("central.world.name names the world's name"))?
        .trim();
    world_name_words(name).map_err(|why| invalid(format!("A world's name {why}")))?;
    let expected = text("expected_revision").ok_or_else(|| {
        invalid("central.world.name is made against the manifest's expected_revision")
    })?;
    let scope = Scope::resolve(central, Some(project))?;
    let principal = authority::authenticate(
        &scope,
        token,
        WORLD_NAME_ACTION,
        text("expected_authority_revision"),
        now,
    )?;
    principal.require_human()?;
    let _lock = crate::source_safety::lock(&scope.root, "source-mutation.lock")?;
    let mut held = Vec::new();
    open_native_file(&scope.root, MANIFEST)?
        .take(MANIFEST_LIMIT + 1)
        .read_to_end(&mut held)?;
    if held.len() as u64 > MANIFEST_LIMIT {
        return Err(invalid("The project's manifest exceeds its bounded size"));
    }
    let revision = content_revision_bytes(&held);
    let world_ref = format!("project:{project}");
    let receipt = |outcome: &str, previous: Option<&str>, after: &str| {
        json!({"schema":WORLD_NAME_SCHEMA,"outcome":outcome,"project":project,"world_ref":world_ref,"name":name,
            "previous_name":previous,"previous_revision":revision,"revision":after,"principal_ref":principal.principal_ref,
            "automatic_agent_or_model_invocation":false})
    };
    if revision != expected {
        let mut conflict = receipt("conflict", None, &revision);
        conflict["expected_revision"] = json!(expected);
        return Ok(conflict);
    }
    let path = scope.root.join(MANIFEST);
    let manifest = parse_project_manifest(&held, &path)?;
    let previous = manifest.name.clone();
    if previous.as_deref() == Some(name) {
        return Ok(receipt("unchanged", previous.as_deref(), &revision));
    }
    // Only the name changes: every other field stays as the manifest keeps it, written as ProjectCentral writes its manifest.
    let mut document: Value = serde_json::from_slice(&held)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    document
        .as_object_mut()
        .ok_or_else(|| invalid("The project's manifest is not an object"))?
        .insert("name".into(), json!(name));
    let mut written = serde_json::to_vec_pretty(&document)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    written.push(b'\n');
    let kept = parse_project_manifest(&written, &path)?;
    if kept.project_id != manifest.project_id || !kept.validate().valid {
        return Err(invalid(
            "The named manifest would not validate; nothing was written",
        ));
    }
    atomic_record(
        &scope.root,
        Path::new(MANIFEST),
        &written,
        RecordDisposition::ReplaceOrCreate,
    )?;
    Ok(receipt(
        "named",
        previous.as_deref(),
        &content_revision_bytes(&written),
    ))
}

fn world_name_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let root = match root_of(WORLD_NAME_ACTION, context) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let token = std::env::var("CENTRAL_NATIVE_TOKEN").ok();
    match name_world(&root, input, token.as_deref(), now()) {
        Ok(data) => ActionResult::success(WORLD_NAME_ACTION, data),
        Err(error) => failure(WORLD_NAME_ACTION, error),
    }
}

pub fn register_world_name_action(registry: &mut ActionRegistry) {
    registry
        .register(
            descriptor(
                WORLD_NAME_ACTION,
                "Name a world",
                "Name a project's world as the person calls it: the name (1 to 120 characters, no control characters, trimmed) is kept in the project's ProjectCentral manifest as name, beside its project_id, which never changes, on the manifest's expected_revision (the world map's manifest_revision); the world map then discloses it as world_name. The person's own act: it authenticates on their native authority (CENTRAL_NATIVE_TOKEN) at the project. A stale revision answers conflict and writes nothing; the same name answers unchanged. Never invokes an Agent or model.",
                MutationClass::LocallyMutating,
                "central-world-name",
                vec![
                    input("project", true),
                    input("name", true),
                    input("expected_revision", true),
                    input("expected_authority_revision", false),
                ],
            ),
            world_name_action,
        )
        .expect("central.world.name is a valid Action id");
}
