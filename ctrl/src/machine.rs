use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::control::SourceClass;
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::{to_value, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MACHINE_DECLARATION_SCHEMA: &str = "central.machine";
pub const MACHINE_DECLARATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineSourceReference {
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRequirement {
    pub id: String,
    pub state: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationRequirement {
    pub id: String,
    pub state: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRequirement {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MachineSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MachineRequirements {
    #[serde(default)]
    pub packages: Vec<PackageRequirement>,
    #[serde(default)]
    pub configurations: Vec<ConfigurationRequirement>,
    #[serde(default)]
    pub services: Vec<ServiceRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineDeclaration {
    pub schema: String,
    pub version: u32,
    pub role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub requirements: MachineRequirements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineDeclarationSource {
    pub path: PathBuf,
    pub source_class: SourceClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthoredMachineDeclaration {
    pub declaration: MachineDeclaration,
    pub source: MachineDeclarationSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineDeclarationError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl MachineDeclarationError {
    fn new(
        code: &str,
        message: impl Into<String>,
        path: Option<PathBuf>,
        field: Option<&str>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            path,
            field: field.map(str::to_owned),
        }
    }
}

fn validate_role_name(role: &str) -> Result<&str, MachineDeclarationError> {
    let role = role.trim();
    if role.is_empty() {
        return Err(MachineDeclarationError::new(
            "invalid_role",
            "Machine role must be non-empty.",
            None,
            Some("role"),
        ));
    }
    if !role.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        || matches!(role, "." | "..")
    {
        return Err(MachineDeclarationError::new(
            "invalid_role",
            "Machine role may contain only letters, digits, '.', '-', and '_' and may not be '.' or '..'.",
            None,
            Some("role"),
        ));
    }
    Ok(role)
}

fn validate_nonempty(value: &str, field: &str, path: &Path) -> Result<(), MachineDeclarationError> {
    if value.trim().is_empty() {
        return Err(MachineDeclarationError::new(
            "invalid_declaration",
            format!("{field} must be non-empty."),
            Some(path.to_path_buf()),
            Some(field),
        ));
    }
    Ok(())
}

fn validate_source(
    source: &MachineSourceReference,
    field: &str,
    path: &Path,
) -> Result<(), MachineDeclarationError> {
    validate_nonempty(&source.kind, &format!("{field}.kind"), path)?;
    validate_nonempty(&source.reference, &format!("{field}.reference"), path)
}

fn validate_capabilities(declaration: &MachineDeclaration, path: &Path) -> Result<(), MachineDeclarationError> {
    let mut seen = BTreeSet::new();
    for capability in &declaration.capabilities {
        validate_nonempty(capability, "capabilities[]", path)?;
        if !seen.insert(capability) {
            return Err(MachineDeclarationError::new(
                "duplicate_capability",
                format!("Duplicate machine capability: {capability}"),
                Some(path.to_path_buf()),
                Some("capabilities"),
            ));
        }
    }
    Ok(())
}

fn validate_presence_requirements<T, FId, FSource>(
    kind: &str,
    items: &[T],
    path: &Path,
    id: FId,
    source: FSource,
) -> Result<(), MachineDeclarationError>
where
    FId: Fn(&T) -> &str,
    FSource: Fn(&T) -> Option<&MachineSourceReference>,
{
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let item_id = id(item);
        validate_nonempty(item_id, &format!("requirements.{kind}[{index}].id"), path)?;
        if !seen.insert(item_id) {
            return Err(MachineDeclarationError::new(
                "duplicate_requirement",
                format!("Duplicate machine {kind} requirement: {item_id}"),
                Some(path.to_path_buf()),
                Some(&format!("requirements.{kind}")),
            ));
        }
        if let Some(source) = source(item) {
            validate_source(source, &format!("requirements.{kind}[{index}].source"), path)?;
        }
    }
    Ok(())
}

fn validate_declaration(
    requested_role: &str,
    declaration: &MachineDeclaration,
    path: &Path,
) -> Result<(), MachineDeclarationError> {
    if declaration.schema != MACHINE_DECLARATION_SCHEMA {
        return Err(MachineDeclarationError::new(
            "unsupported_schema",
            format!(
                "Machine declaration schema '{}' is unsupported; expected '{MACHINE_DECLARATION_SCHEMA}'.",
                declaration.schema
            ),
            Some(path.to_path_buf()),
            Some("schema"),
        ));
    }
    if declaration.version != MACHINE_DECLARATION_VERSION {
        return Err(MachineDeclarationError::new(
            "unsupported_version",
            format!(
                "Machine declaration version {} is unsupported; expected version {MACHINE_DECLARATION_VERSION}.",
                declaration.version
            ),
            Some(path.to_path_buf()),
            Some("version"),
        ));
    }
    validate_nonempty(&declaration.role, "role", path)?;
    if declaration.role != requested_role {
        return Err(MachineDeclarationError::new(
            "role_mismatch",
            format!(
                "Machine declaration role '{}' does not match requested role '{requested_role}'.",
                declaration.role
            ),
            Some(path.to_path_buf()),
            Some("role"),
        ));
    }
    validate_capabilities(declaration, path)?;
    validate_presence_requirements(
        "packages",
        &declaration.requirements.packages,
        path,
        |item| item.id.as_str(),
        |item| item.source.as_ref(),
    )?;
    validate_presence_requirements(
        "configurations",
        &declaration.requirements.configurations,
        path,
        |item| item.id.as_str(),
        |item| item.source.as_ref(),
    )?;

    let mut services = BTreeSet::new();
    for (index, service) in declaration.requirements.services.iter().enumerate() {
        validate_nonempty(&service.id, &format!("requirements.services[{index}].id"), path)?;
        if !services.insert(service.id.as_str()) {
            return Err(MachineDeclarationError::new(
                "duplicate_requirement",
                format!("Duplicate machine service requirement: {}", service.id),
                Some(path.to_path_buf()),
                Some("requirements.services"),
            ));
        }
        if service.running.is_none() && service.enabled.is_none() {
            return Err(MachineDeclarationError::new(
                "invalid_service_requirement",
                format!(
                    "Service requirement '{}' must declare at least one of running or enabled.",
                    service.id
                ),
                Some(path.to_path_buf()),
                Some(&format!("requirements.services[{index}]")),
            ));
        }
        if let Some(source) = &service.source {
            validate_source(source, &format!("requirements.services[{index}].source"), path)?;
        }
    }
    Ok(())
}

pub fn read_machine_declaration(
    central_root: &Path,
    role: &str,
) -> Result<AuthoredMachineDeclaration, MachineDeclarationError> {
    let role = validate_role_name(role)?;
    let relative = PathBuf::from("Control").join("machines").join(format!("{role}.json"));
    let path = central_root.join(&relative);
    let text = fs::read_to_string(&path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            "missing_declaration"
        } else {
            "read_failure"
        };
        MachineDeclarationError::new(
            code,
            format!("Cannot read machine declaration for {role}: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    let raw = serde_json::from_str::<Value>(&text).map_err(|error| {
        MachineDeclarationError::new(
            "invalid_json",
            format!("Machine declaration for {role} is not valid JSON: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    let declaration = serde_json::from_value::<MachineDeclaration>(raw).map_err(|error| {
        MachineDeclarationError::new(
            "invalid_declaration",
            format!("Machine declaration for {role} has an invalid structured form: {error}"),
            Some(relative.clone()),
            None,
        )
    })?;
    validate_declaration(role, &declaration, &relative)?;
    Ok(AuthoredMachineDeclaration {
        declaration,
        source: MachineDeclarationSource {
            path: relative,
            source_class: SourceClass::Authored,
        },
    })
}

fn required_role(input: &Value) -> Result<String, ActionResult> {
    let Some(role) = input
        .get("role")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(ActionResult::failure(
            Some("machine.declaration"),
            ResultStatus::InvalidInput,
            "machine.declaration requires role.",
            None,
        ));
    };
    Ok(role.to_owned())
}

fn declaration_action(
    _registry: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let role = match required_role(input) {
        Ok(role) => role,
        Err(result) => return result,
    };
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => {
            return ActionResult::failure(
                Some("machine.declaration"),
                ResultStatus::InvalidInput,
                message,
                None,
            );
        }
    };
    match read_machine_declaration(&root.path, &role) {
        Ok(declaration) => ActionResult::success(
            "machine.declaration",
            to_value(declaration).expect("machine declaration serializes"),
        ),
        Err(error) => ActionResult::failure(
            Some("machine.declaration"),
            ResultStatus::InvalidInput,
            error.message.clone(),
            Some(to_value(error).expect("machine declaration error serializes")),
        ),
    }
}

pub(crate) fn register_machine_actions(registry: &mut ActionRegistry) {
    let descriptor = ActionDescriptor {
        id: "machine.declaration".to_owned(),
        title: "Read machine declaration".to_owned(),
        description: "Read and validate one versioned authored machine-role declaration from Control/machines.".to_owned(),
        inputs: vec![ActionInputDefinition {
            name: "role".to_owned(),
            input_type: "string".to_owned(),
            required: true,
            choices: None,
            selection: None,
        }],
        output: ActionOutputDefinition { output_type: "authored-machine-declaration".to_owned() },
        mutation_class: MutationClass::ReadOnly,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability { available: true, reason: None },
    };
    registry.register(descriptor, declaration_action).expect("machine Action id is valid");
}

fn render_reference(source: Option<&Value>) -> String {
    let Some(source) = source else { return String::new(); };
    let kind = source.get("kind").and_then(Value::as_str).unwrap_or_default();
    let reference = source.get("reference").and_then(Value::as_str).unwrap_or_default();
    if kind.is_empty() && reference.is_empty() {
        String::new()
    } else {
        format!(" ({kind}: {reference})")
    }
}

pub fn explain_machine_declaration(data: &Value) -> String {
    let declaration = data.get("declaration").unwrap_or(&Value::Null);
    let schema = declaration.get("schema").and_then(Value::as_str).unwrap_or_default();
    let version = declaration.get("version").and_then(Value::as_u64).unwrap_or_default();
    let role = declaration.get("role").and_then(Value::as_str).unwrap_or_default();
    let source = data
        .get("source")
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = vec![
        format!("Machine role: {role}"),
        format!("Declaration: {schema} v{version}"),
        format!("Source: {source} [authored]"),
    ];

    let capabilities = declaration
        .get("capabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if capabilities.is_empty() {
        lines.push("Capabilities: none".to_owned());
    } else {
        lines.push("Capabilities:".to_owned());
        for capability in capabilities {
            lines.push(format!("  - {}", capability.as_str().unwrap_or_default()));
        }
    }

    let requirements = declaration.get("requirements").unwrap_or(&Value::Null);
    for (label, key) in [("Packages", "packages"), ("Configurations", "configurations")] {
        lines.push(format!("{label}:"));
        let items = requirements.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
        if items.is_empty() {
            lines.push("  - none".to_owned());
        } else {
            for item in items {
                let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                let state = item.get("state").and_then(Value::as_str).unwrap_or_default();
                lines.push(format!("  - {id}: {state}{}", render_reference(item.get("source"))));
            }
        }
    }

    lines.push("Services:".to_owned());
    let services = requirements
        .get("services")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if services.is_empty() {
        lines.push("  - none".to_owned());
    } else {
        for service in services {
            let id = service.get("id").and_then(Value::as_str).unwrap_or_default();
            let mut states = Vec::new();
            if let Some(running) = service.get("running").and_then(Value::as_bool) {
                states.push(format!("running={running}"));
            }
            if let Some(enabled) = service.get("enabled").and_then(Value::as_bool) {
                states.push(format!("enabled={enabled}"));
            }
            lines.push(format!(
                "  - {id}: {}{}",
                states.join(", "),
                render_reference(service.get("source"))
            ));
        }
    }
    lines.join("\n")
}
