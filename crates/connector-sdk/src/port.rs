use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortOperationContract {
    pub name: &'static str,
    pub input_type: &'static str,
    pub output_type: &'static str,
    pub mutation_class: &'static str,
    pub preview_required: bool,
    pub idempotent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortContract {
    pub id: &'static str,
    pub version: &'static str,
    pub purpose: &'static str,
    pub operations: &'static [PortOperationContract],
}

pub const WORK_DISCOVERY_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "list",
    input_type: "WorkDiscoveryInput",
    output_type: "WorkDiscoveryOutput",
    mutation_class: "read-only",
    preview_required: false,
    idempotent: true,
}];

pub const WORK_DISCOVERY_PORT: PortContract = PortContract {
    id: "WorkDiscovery",
    version: "1.0.0",
    purpose: "Discover and resolve ordinary Work items without requiring a Central-specific project format.",
    operations: &WORK_DISCOVERY_OPERATIONS,
};

pub const AUTOMATION_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "run",
    input_type: "AutomationRunInput",
    output_type: "AutomationRunOutput",
    mutation_class: "externally-mutating",
    preview_required: false,
    idempotent: false,
}];

pub const AUTOMATION_PORT: PortContract = PortContract {
    id: "Automation",
    version: "1.0.0",
    purpose: "Invoke a named host automation without coupling the canonical Action to one automation provider.",
    operations: &AUTOMATION_OPERATIONS,
};

pub const NATIVE_OPEN_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "open",
    input_type: "NativeOpenInput",
    output_type: "NativeOpenOutput",
    mutation_class: "externally-mutating",
    preview_required: false,
    idempotent: false,
}];

pub const NATIVE_OPEN_PORT: PortContract = PortContract {
    id: "NativeOpen",
    version: "1.0.0",
    purpose: "Open a target through the normal host user experience without coupling a core Action to one platform.",
    operations: &NATIVE_OPEN_OPERATIONS,
};

pub const NATIVE_REVEAL_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "reveal",
    input_type: "NativeRevealInput",
    output_type: "NativeRevealOutput",
    mutation_class: "externally-mutating",
    preview_required: false,
    idempotent: false,
}];

pub const NATIVE_REVEAL_PORT: PortContract = PortContract {
    id: "NativeReveal",
    version: "1.0.0",
    purpose: "Reveal a target through the normal host filesystem surface without coupling core behavior to one platform.",
    operations: &NATIVE_REVEAL_OPERATIONS,
};

pub const TAG_STORE_OPERATIONS: [PortOperationContract; 2] = [
    PortOperationContract {
        name: "read",
        input_type: "TagReadInput",
        output_type: "TagReadOutput",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "replace",
        input_type: "TagReplaceInput",
        output_type: "TagReplaceOutput",
        mutation_class: "locally-mutating",
        preview_required: false,
        idempotent: true,
    },
];

pub const TAG_STORE_PORT: PortContract = PortContract {
    id: "TagStore",
    version: "1.0.0",
    purpose: "Read and replace optional local metadata tags without making those tags part of Work identity.",
    operations: &TAG_STORE_OPERATIONS,
};

pub const MACHINE_INSPECTOR_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "inspect",
    input_type: "MachineInspectionInput",
    output_type: "MachineInspectionOutput",
    mutation_class: "read-only",
    preview_required: false,
    idempotent: true,
}];

pub const MACHINE_INSPECTOR_PORT: PortContract = PortContract {
    id: "MachineInspector",
    version: "1.0.0",
    purpose: "Collect structured current-state observations required by machine planning and Connector eligibility.",
    operations: &MACHINE_INSPECTOR_OPERATIONS,
};

pub const PACKAGE_MANAGER_OPERATIONS: [PortOperationContract; 2] = [
    PortOperationContract {
        name: "preview",
        input_type: "PackageStateRequest",
        output_type: "StateChangePreview",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "apply",
        input_type: "PackageStateRequest",
        output_type: "StateChangeResult",
        mutation_class: "locally-mutating",
        preview_required: true,
        idempotent: true,
    },
];

pub const PACKAGE_MANAGER_PORT: PortContract = PortContract {
    id: "PackageManager",
    version: "1.0.0",
    purpose: "Inspect and reconcile package presence without prescribing one package provider.",
    operations: &PACKAGE_MANAGER_OPERATIONS,
};

pub const CONFIGURATION_MANAGER_OPERATIONS: [PortOperationContract; 2] = [
    PortOperationContract {
        name: "preview",
        input_type: "ConfigurationStateRequest",
        output_type: "StateChangePreview",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "apply",
        input_type: "ConfigurationStateRequest",
        output_type: "StateChangeResult",
        mutation_class: "locally-mutating",
        preview_required: true,
        idempotent: true,
    },
];

pub const CONFIGURATION_MANAGER_PORT: PortContract = PortContract {
    id: "ConfigurationManager",
    version: "1.0.0",
    purpose: "Inspect and reconcile portable configuration through a replaceable configuration mechanism.",
    operations: &CONFIGURATION_MANAGER_OPERATIONS,
};

pub const SERVICE_MANAGER_OPERATIONS: [PortOperationContract; 2] = [
    PortOperationContract {
        name: "preview",
        input_type: "ServiceStateRequest",
        output_type: "StateChangePreview",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "apply",
        input_type: "ServiceStateRequest",
        output_type: "StateChangeResult",
        mutation_class: "locally-mutating",
        preview_required: true,
        idempotent: true,
    },
];

pub const SERVICE_MANAGER_PORT: PortContract = PortContract {
    id: "ServiceManager",
    version: "1.0.0",
    purpose: "Inspect and reconcile service running and enablement state without prescribing one service provider.",
    operations: &SERVICE_MANAGER_OPERATIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkDiscoveryInput {
    pub work_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkItem {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkDiscoveryOutput {
    pub items: Vec<WorkItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationRunInput {
    pub automation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationRunOutput {
    pub automation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeOpenInput {
    pub target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeOpenOutput {
    pub target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeRevealInput {
    pub target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeRevealOutput {
    pub target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagReadInput {
    pub target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagReadOutput {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagReplaceInput {
    pub target: PathBuf,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagReplaceOutput {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MachineInspectionInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedPackage {
    pub id: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedConfiguration {
    pub id: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedService {
    pub id: String,
    pub present: bool,
    pub running: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineInspectionOutput {
    pub platform: String,
    pub architecture: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub packages: Vec<ObservedPackage>,
    #[serde(default)]
    pub configurations: Vec<ObservedConfiguration>,
    #[serde(default)]
    pub services: Vec<ObservedService>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationSourceReference {
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageStateRequest {
    pub id: String,
    pub present: bool,
    pub source: Option<ReconciliationSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationStateRequest {
    pub id: String,
    pub present: bool,
    pub source: Option<ReconciliationSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStateRequest {
    pub id: String,
    pub running: Option<bool>,
    pub enabled: Option<bool>,
    pub source: Option<ReconciliationSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChangePreview {
    pub changed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChangeResult {
    pub changed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortErrorCode {
    UnsupportedEnvironment,
    MissingDependency,
    InvalidConfiguration,
    CapabilityUnavailable,
    InvalidInput,
    ProviderOperationFailed,
    PermissionFailure,
    VerificationFailure,
    UnexpectedConnectorFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortError {
    pub code: PortErrorCode,
    pub message: String,
    pub provider_detail: Option<String>,
}

impl PortError {
    pub fn new(code: PortErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), provider_detail: None }
    }

    pub fn with_provider_detail(mut self, detail: impl Into<String>) -> Self {
        self.provider_detail = Some(detail.into());
        self
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self::new(PortErrorCode::ProviderOperationFailed, message)
    }
}

pub trait WorkDiscovery: Send + Sync {
    fn list(&self, input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError>;
}

pub trait Automation: Send + Sync {
    fn run(&self, input: &AutomationRunInput) -> Result<AutomationRunOutput, PortError>;
}

pub trait NativeOpen: Send + Sync {
    fn open(&self, input: &NativeOpenInput) -> Result<NativeOpenOutput, PortError>;
}

pub trait NativeReveal: Send + Sync {
    fn reveal(&self, input: &NativeRevealInput) -> Result<NativeRevealOutput, PortError>;
}

pub trait TagStore: Send + Sync {
    fn read(&self, input: &TagReadInput) -> Result<TagReadOutput, PortError>;
    fn replace(&self, input: &TagReplaceInput) -> Result<TagReplaceOutput, PortError>;
}

pub trait MachineInspector: Send + Sync {
    fn inspect(&self, input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError>;
}

pub trait PackageManager: Send + Sync {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError>;
    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError>;
}

pub trait ConfigurationManager: Send + Sync {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError>;
    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError>;
}

pub trait ServiceManager: Send + Sync {
    fn preview(&self, input: &ServiceStateRequest) -> Result<StateChangePreview, PortError>;
    fn apply(&self, input: &ServiceStateRequest) -> Result<StateChangeResult, PortError>;
}
