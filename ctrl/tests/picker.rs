use central_ctrl::{
    create_core_action_registry, initialize_central, run_guided_action_picker, ActionExecutionContext,
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, FilesystemWorkConnector, NativeOpen, NativeOpenInput, NativeOpenOutput,
    PortContract, PortError, ResultStatus, RootOptions, TerminalSurface, CONNECTOR_API_VERSION,
    NATIVE_OPEN_PORT,
};
use serde_json::json;
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-picker-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[derive(Default)]
struct ScriptedSurface {
    inputs: VecDeque<String>,
    output: Vec<String>,
}

impl ScriptedSurface {
    fn new(inputs: &[&str]) -> Self {
        Self { inputs: inputs.iter().map(|value| (*value).to_owned()).collect(), output: Vec::new() }
    }
}

impl TerminalSurface for ScriptedSurface {
    fn write_line(&mut self, line: &str) {
        self.output.push(line.to_owned());
    }

    fn prompt(&mut self, message: &str) -> Option<String> {
        self.output.push(message.to_owned());
        self.inputs.pop_front()
    }
}

struct PickerNativeConnector {
    manifest: ConnectorManifest,
}

impl PickerNativeConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "test.picker-native".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Picker native open".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: NATIVE_OPEN_PORT.id.to_owned(),
                    version: NATIVE_OPEN_PORT.version.to_owned(),
                }],
                platforms: vec!["test".to_owned()],
                entrypoint: "test:PickerNativeConnector".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "externally-mutating".to_owned(),
            },
        }
    }
}

impl NativeOpen for PickerNativeConnector {
    fn open(&self, input: &NativeOpenInput) -> Result<NativeOpenOutput, PortError> {
        Ok(NativeOpenOutput { target: input.target.clone() })
    }
}

impl Connector for PickerNativeConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe {
        if context.platform == "test" && port.id == NATIVE_OPEN_PORT.id && port.version == NATIVE_OPEN_PORT.version {
            CapabilityProbe::available()
        } else {
            CapabilityProbe::unavailable("Picker native Connector is not eligible for this request.")
        }
    }

    fn native_open(&self) -> Option<&dyn NativeOpen> {
        Some(self)
    }
}

fn context<'a>(
    root: &'a PathBuf,
    connectors: &'a ConnectorRegistry,
    connector_context: &'a ConnectorContext,
    options: &'a mut RootOptions,
) -> ActionExecutionContext<'a> {
    *options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    ActionExecutionContext { root_options: options, connectors, connector_context }
}

fn filesystem_connectors() -> ConnectorRegistry {
    let mut connectors = ConnectorRegistry::default();
    connectors.register(FilesystemWorkConnector::new()).unwrap();
    connectors
}

fn project_entry_connectors() -> ConnectorRegistry {
    let mut connectors = filesystem_connectors();
    connectors.register(PickerNativeConnector::new()).unwrap();
    connectors
}

#[test]
fn descriptors_publish_static_and_action_backed_selectable_inputs() {
    let registry = create_core_action_registry();
    let control = &registry.get("control.open").unwrap().inputs[0];
    assert_eq!(control.choices.as_ref().unwrap(), &vec!["user".to_owned(), "agents".to_owned(), "machines".to_owned()]);
    let work = &registry.get("work.open").unwrap().inputs[0];
    let selection = work.selection.as_ref().unwrap();
    assert_eq!(selection.action, "work.list");
    assert_eq!(selection.collection, "items");
    assert_eq!(selection.value_field, "name");
}

#[test]
fn guided_project_entry_searches_registry_confirms_mutation_and_executes_the_same_work_open_action() {
    let root = temporary_directory("guided").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("project-a")).unwrap();
    let registry = create_core_action_registry();
    let connectors = project_entry_connectors();
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let mut options = RootOptions::default();
    let ctx = context(&root, &connectors, &connector_context, &mut options);

    let direct = registry.execute("work.open", &json!({ "query": "project-a" }), &ctx);
    assert_eq!(direct.status, ResultStatus::Success);
    let mut surface = ScriptedSurface::new(&["work.open", "1", "1", "yes"]);
    let guided = run_guided_action_picker(&registry, &ctx, &mut surface);
    assert_eq!(guided, direct);
    assert!(surface.output.iter().any(|line| line.contains("Matching Actions")));
    assert!(surface.output.iter().any(|line| line.contains("Choices for query")));
    assert!(surface.output.iter().any(|line| line.contains("externally-mutating")));
}

#[test]
fn cancellation_from_search_is_a_normal_structured_result() {
    let root = temporary_directory("cancel-search").join("Central");
    let registry = create_core_action_registry();
    let connectors = filesystem_connectors();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let mut options = RootOptions::default();
    let ctx = context(&root, &connectors, &connector_context, &mut options);
    let mut surface = ScriptedSurface::new(&["/cancel"]);
    let result = run_guided_action_picker(&registry, &ctx, &mut surface);
    assert_eq!(result.status, ResultStatus::Cancelled);
    assert_eq!(result.error.unwrap().code, "cancelled");
}

#[test]
fn cancellation_from_action_selection_is_a_normal_structured_result() {
    let root = temporary_directory("cancel-select").join("Central");
    let registry = create_core_action_registry();
    let connectors = filesystem_connectors();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let mut options = RootOptions::default();
    let ctx = context(&root, &connectors, &connector_context, &mut options);
    let mut surface = ScriptedSurface::new(&["work.open", "/cancel"]);
    let result = run_guided_action_picker(&registry, &ctx, &mut surface);
    assert_eq!(result.status, ResultStatus::Cancelled);
}

#[test]
fn back_from_selectable_value_returns_to_action_search_without_executing_it() {
    let root = temporary_directory("back").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work").join("project-a")).unwrap();
    let registry = create_core_action_registry();
    let connectors = filesystem_connectors();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let mut options = RootOptions::default();
    let ctx = context(&root, &connectors, &connector_context, &mut options);
    let mut surface = ScriptedSurface::new(&["work.open", "1", "/back", "work.search", "1", "project-a"]);
    let result = run_guided_action_picker(&registry, &ctx, &mut surface);
    assert_eq!(result.status, ResultStatus::Success);
    assert_eq!(result.action.as_deref(), Some("work.search"));
}

#[test]
fn mutating_action_requires_confirmation_and_rejection_does_not_mutate() {
    let root = temporary_directory("confirm").join("Central");
    let registry = create_core_action_registry();
    let connectors = filesystem_connectors();
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    let mut options = RootOptions::default();
    let ctx = context(&root, &connectors, &connector_context, &mut options);
    let mut surface = ScriptedSurface::new(&["central.init", "1", "no"]);
    let result = run_guided_action_picker(&registry, &ctx, &mut surface);
    assert_eq!(result.status, ResultStatus::Cancelled);
    assert!(!root.exists());
}

#[test]
fn real_ctrl_pick_process_uses_stdin_for_guidance_and_stdout_for_structured_result() {
    let root = temporary_directory("process").join("Central");
    initialize_central(&root).unwrap();
    let binary = env!("CARGO_BIN_EXE_ctrl");
    let mut child = Command::new(binary)
        .args(["--json", "--root", root.to_str().unwrap(), "pick"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(b"control.open\n1\n1\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["action"], "control.open");
    assert_eq!(payload["data"]["target"], "user");
    let guidance = String::from_utf8_lossy(&output.stderr);
    assert!(guidance.contains("Search Actions"));
    assert!(guidance.contains("Choices for target"));
}
