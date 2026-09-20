# Temporary, exact-base publisher. Removed in its own application commit.
from pathlib import Path
import hashlib
root = Path('.')
expected = {
 'ctrl/src/world_source.rs':'d71d181166edcbcfcd22feeabfbd0ada4421cab26d131840e5e6af94545644e7',
 'ctrl/src/source_horizon.rs':'22948005629a802723cd4464be150f75fa27308aef26ba962fadc69056d2362b',
 'ctrl/src/source_horizon_extensions.rs':'2d6ca27b7f9632fe3499d456f6ee74de8f30e9c3290ab391e3642038229147e9',
 'ctrl/src/continuous_work/temporal.rs':'9279707a16857c517a8e1e637cc7b24e5bb6b9c69fc7bdef41af6df0b9412d47',
}
for name, digest in expected.items():
 assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, 'Source moved: '+name
p=root/'ctrl/src/world_source.rs';s=p.read_text()
s=s.replace('read_project_change_horizon, reconcile_project_source_writes, SourceBinding, SourceRevision,','read_project_change_horizon, reconcile_project_source_writes, reconcile_control_sources,\n    reconcile_control_source_writes, SourceBinding, SourceRevision,')
s=s.replace('role == "agent-governance-source" || role == "project-human-source-aperture"','role == "agent-governance-source" || role == "project-human-source-aperture"\n                || role == "personal-human-source-aperture"')
s=s.replace('pub fn read_world_source(project_root: &Path, source_ref: &str) -> io::Result<WorldSourceReading> {\n    let horizon = read_project_change_horizon(project_root, None)?;', '''pub fn read_world_source(project_root: &Path, source_ref: &str) -> io::Result<WorldSourceReading> {
    read_scoped_source(project_root, source_ref, false)
}

/// The root meta-Project uses its existing Control horizon. No ProjectCentral
/// manifest, adoption, copied source, or path-derived SourceRef is needed.
pub fn read_control_world_source(root: &Path, source_ref: &str) -> io::Result<WorldSourceReading> {
    read_scoped_source(root, source_ref, true)
}

fn read_scoped_source(project_root: &Path, source_ref: &str, root_register: bool) -> io::Result<WorldSourceReading> {
    let horizon = if root_register {
        reconcile_control_sources(project_root)?.horizon
    } else {
        read_project_change_horizon(project_root, None)?
    };''')
s=s.replace('"source_ref is not a participating World source of this Project"','"source_ref is not a participating source of the requested World"')
idx=s.index('    let _lock = crate::source_safety::lock(project_root, "source-mutation.lock")?;')
s=s[:idx]+'''    write_scoped_source(project_root, source_ref, expected_revision, content, actor, actor_kind, agent_session_ref, false)
}

pub fn write_control_world_source(
    root: &Path, source_ref: &str, expected_revision: &str, content: &str,
    actor: &str, actor_kind: &str, agent_session_ref: Option<String>,
) -> io::Result<WorldSourceWriteReceipt> {
    write_scoped_source(root, source_ref, expected_revision, content, actor, actor_kind, agent_session_ref, true)
}

#[allow(clippy::too_many_arguments)]
fn write_scoped_source(
    project_root: &Path, source_ref: &str, expected_revision: &str, content: &str,
    actor: &str, actor_kind: &str, agent_session_ref: Option<String>, root_register: bool,
) -> io::Result<WorldSourceWriteReceipt> {
'''+s[idx:]
s=s.replace('    let horizon = read_project_change_horizon(project_root, None)?;', '''    let horizon = if root_register {
        reconcile_control_sources(project_root)?.horizon
    } else {
        read_project_change_horizon(project_root, None)?
    };''')
s=s.replace('    let report = reconcile_project_source_writes(project_root, &attributions)?;', '''    let report = if root_register {
        reconcile_control_source_writes(project_root, &attributions)?
    } else {
        reconcile_project_source_writes(project_root, &attributions)?
    };''')
s=s.replace('"written World source left its Project horizon"','"written World source left its requested horizon"')
s=s.replace('let root = match project_root(action, input, context) {','let (root, root_register) = match source_scope(action, input, context) {')
s=s.replace('    read_world_source(&root, &source_ref)\n','    read_scoped_source(&root, &source_ref, root_register)\n')
s=s.replace('write_world_source(&root,&source_ref,&expected_revision,content,&actor,&actor_kind,optional(input, "agent_session_ref"))','write_scoped_source(&root,&source_ref,&expected_revision,content,&actor,&actor_kind,optional(input, "agent_session_ref"),root_register)')
idx=s.index('fn io_failure(')
s=s[:idx]+'''/// Absent/null is the explicit root scope; malformed or empty project input
/// is not absence. The legacy action spelling remains wire compatible.
fn source_scope(action: &str, input: &Value, context: &ActionExecutionContext<'_>) -> Result<(PathBuf, bool), ActionResult> {
    if input.get("project").is_none_or(Value::is_null) {
        let root = resolve_central_root(context.root_options)
            .map_err(|message| ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None))?.path;
        let scope = crate::continuous_work::source::Scope::resolve(&root, None)
            .map_err(|error| io_failure(action, error))?;
        Ok((scope.root, true))
    } else {
        project_root(action, input, context).map(|root| (root, false))
    }
}

'''+s[idx:]
s=s.replace('("project", true), ("source_ref", true)','("project", false), ("source_ref", true)').replace('("project",true),("source_ref",true)','("project",false),("source_ref",true)')
s=s.replace('Read one participating Project World source','Read one participating World source (omit project for the Central root meta-Project)').replace('one participating Project World source:', 'one participating World source (omit project for the Central root meta-Project):')
p.write_text(s)
p=root/'ctrl/src/source_horizon_extensions.rs'
s=p.read_text()+'''
/// Root counterpart of read_project_change_horizon. The same Control horizon
/// records external edits and native write attribution; reading does not adopt.
pub fn read_control_change_horizon(central_root: &std::path::Path, since: Option<u64>) -> std::io::Result<SourceHorizon> {
    reconcile_control_sources(central_root)?;
    let state = load_state(&central_root.join(CONTROL_HORIZON_STATE))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Control source horizon state was not created"))?;
    Ok(public_horizon(&state, since))
}
'''
p.write_text(s)
p=root/'ctrl/src/source_horizon.rs';s=p.read_text()
a=s.index('fn horizon_action(');b=s.index('\nfn reconcile_action(',a)
s=s[:a]+'''fn horizon_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.change.horizon";
    let since = input.get("cursor").and_then(Value::as_u64);
    let result = if input.get("project").is_none_or(Value::is_null) {
        let root = match resolve_central_root(context.root_options) {
            Ok(root) => root.path,
            Err(message) => return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None),
        };
        crate::continuous_work::source::Scope::resolve(&root, None)
            .and_then(|scope| read_control_change_horizon(&scope.root, since))
    } else {
        let root = match project_root(action, input, context) {
            Ok(root) => root,
            Err(result) => return result,
        };
        read_project_change_horizon(&root, since)
    };
    result.map(|value| ActionResult::success(action, serde_json::to_value(value).expect("horizon serializes")))
        .unwrap_or_else(|error| io_failure(action, error))
}
''' +s[b:]
s=s.replace('Reconcile participating Project sources into deterministic revisions', 'Reconcile participating sources (omit project for the Central root meta-Project) into deterministic revisions')
s=s.replace('vec![action_input("project", true), action_input("cursor", false)]','vec![action_input("project", false), action_input("cursor", false)]')
p.write_text(s)
p=root/'ctrl/src/continuous_work/temporal.rs';s=p.read_text()
old='''    let source = scope.read(text(entry, "ref")?)?;
    Ok(
        json!({"schema":"central.day-reading/v1","day_ref":entry["temporal"]["day_ref"],"source":source.source,"revision":source.revision,"content":source.content,"temporal":entry["temporal"],"relations_revision":relation_basis,"today":relations["temporal"]["today"],"automatic_agent_or_model_invocation":false}),
    )'''
new='''    let source = scope.read(text(entry, "ref")?)?;
    // The temporal carrier is not automatically a Daily Die. Only the native
    // document relation selects a document; no filename/template search and
    // no catch-and-empty downgrade of a broken/denied document reading.
    let document = if let Some(meta) = entry.get("document") {
        if meta["kind"] != "day" {
            return Err(invalid("human Day source is bound to a non-Day document"));
        }
        let value = super::documents::read(scope, &json!({
            "source_ref": source.source.source_ref, "document_id": text(meta, "document_id")?
        }))?;
        if value["source"]["ref"] != source.source.source_ref
            || value["revision"]["revision"] != source.revision.revision
            || value["document"]["day_ref"] != entry["temporal"]["day_ref"] {
            return Err(conflict("Day/document binding or revision changed during resolution"));
        }
        value
    } else {
        Value::Null
    };
    Ok(
        json!({"schema":"central.day-reading/v1","day_ref":entry["temporal"]["day_ref"],"source":source.source,"revision":source.revision,"content":source.content,"temporal":entry["temporal"],"relations_revision":relation_basis,"today":relations["temporal"]["today"],"document_state":if document.is_null(){"uninitialised"}else{"ready"},"document":document,"automatic_agent_or_model_invocation":false}),
    )'''
assert old in s;s=s.replace(old,new);p.write_text(s)
print('Applied exactly four native owner files; existing document/receiving files untouched.')
