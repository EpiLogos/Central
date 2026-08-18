pub mod action;
pub mod cli;
pub mod control;
pub mod machine;
pub mod picker;
pub mod projectcentral;
pub mod projectcentral_ground;
pub mod projectcentral_ops;
pub mod recovery;
pub mod result;
pub mod root;

#[cfg(test)]
extern crate self as tempfile;

#[cfg(test)]
mod test_tempfile {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    pub fn tempdir() -> io::Result<TempDir> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-projectcentral-test-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(TempDir { path })
    }
}

#[cfg(test)]
pub use test_tempfile::tempdir;

pub use action::{
    create_core_action_registry, ActionAvailability, ActionDescriptor, ActionExecutionContext,
    ActionInputDefinition, ActionInputSelection, ActionOutputDefinition, ActionRegistry,
    MutationClass,
};
pub use cli::{run_cli, run_cli_with_surface, CliEnvironment, CliExecution};
pub use control::{
    locate_control_root, search_control, ControlSearchMatch, ControlSearchResult,
    ControlSkippedSource, ControlSourceRoot, SourceClass, AGENT_RETRIEVAL_DENY_MARKER,
    CONTROL_ROOTS,
};
pub use machine::{
    explain_machine_apply, explain_machine_declaration, explain_machine_inspection,
    explain_machine_plan, explain_machine_verification, read_machine_declaration,
    AuthoredMachineDeclaration, ConfigurationRequirement, MachineApplyOperation,
    MachineApplyOutcome, MachineApplyReport, MachineDeclaration, MachineDeclarationError,
    MachineDeclarationSource, MachineObservationSource, MachinePlan, MachinePlanEntry,
    MachinePlanStatus, MachinePlanSummary, MachineRequirements, MachineSourceReference,
    MachineVerification, ObservedMachine, PackageRequirement, PresenceState, ServiceRequirement,
    MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION,
};
pub use picker::{
    run_guided_action_picker, search_action_descriptors, NullTerminalSurface, StdioTerminalSurface,
    TerminalSurface,
};
pub use projectcentral::{
    projectcentral_paths, read_project_manifest, ManifestValidation, ProjectCentralManifest,
    ProjectCentralPaths, WikiBinding, AGENT_DIR, AGENT_GOVERNANCE_DIR, HUMAN_SOURCE_DIR,
    PROJECTCENTRAL_DIR, PROJECT_MANIFEST, PROJECT_SCHEMA, ROOT_AGENT_DIR,
    ROOT_AGENT_GOVERNANCE_DIR, ROOT_HUMAN_SOURCE_DIR, ROOT_WIKI_DIR, ROOT_WIKI_SOURCE,
    WIKI_DIR, WIKI_PROFILE, WIKI_SOURCE,
};
pub use projectcentral_ground::{
    apply_accepted_ground_relation, inspect_project_ground, plan_project_ground,
    register_projectcentral_ground_actions, GroundAccountHandoff, GroundApplyResult,
    GroundCandidate, GroundInspection, GroundPlan, GroundPlanItem, GroundReturnPolicy,
    GroundSkippedSource, GroundSourceRecord, GroundSourceRelation, GroundStatus,
    SourceProvenance, SourceStanding, SourceTreatment, GROUND_RELATIONS_DIR,
    GROUND_RELATIONS_SCHEMA, GROUND_RELATIONS_SOURCE,
};
pub use projectcentral_ops::{
    adopt_in_place, doctor_projectcentral, ensure_root_federation, initialize_projectcentral,
    inspect_projectcentral, migrate_selected, preview_adopt, preview_migrate, DoctorCheck,
    MutationPlan, ProjectCentralDoctor, ProjectCentralInspection, ProjectCentralMutation,
    ProjectCentralOutcome, SourceSignal, WikiCandidate, PROJECT_PROVENANCE, ROOT_WIKI_REF,
};
pub use recovery::{
    explain_recovery, explain_recovery_plan, AuthoredRecoveryDeclaration, RecoveryDeclaration,
    RecoveryDeclarationSource, RecoveryPlan, RecoverySynchronizationPlan,
    RecoverySynchronizationStatus, SynchronizationDeclaration, RECOVERY_DECLARATION_SCHEMA,
    RECOVERY_DECLARATION_VERSION,
};
pub use central_connector_sdk::{
    run_configuration_manager_conformance, run_machine_inspector_conformance,
    run_native_open_conformance, run_native_reveal_conformance, run_package_manager_conformance,
    run_service_manager_conformance, run_synchronizer_conformance, run_tag_store_conformance,
    validate_connector_manifest, CapabilityProbe, ConfigurationManager,
    ConfigurationManagerConformanceFixture, ConfigurationStateRequest, Connector,
    ConnectorContext, ConnectorDiagnostics, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, ConnectorSummary, MachineInspectionInput, MachineInspectionOutput,
    MachineInspector, MachineInspectorConformanceFixture, NativeOpen, NativeOpenInput,
    NativeOpenOutput, NativeReveal, NativeRevealInput, NativeRevealOutput,
    NativeTargetConformanceFixture, ObservedConfiguration, ObservedPackage, ObservedService,
    PackageManager, PackageManagerConformanceFixture, PackageStateRequest, PortContract, PortError,
    PortErrorCode, ReconciliationSourceReference, ServiceManager, ServiceManagerConformanceFixture,
    ServiceStateRequest, StateChangePreview, StateChangeResult, SynchronizationRequest,
    Synchronizer, SynchronizerConformanceFixture, SynchronizerConformanceReport, TagReadInput,
    TagReadOutput, TagReplaceInput, TagReplaceOutput, TagStore, TagStoreConformanceFixture,
    WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput, WorkItem,
    CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT, NATIVE_OPEN_PORT,
    NATIVE_REVEAL_PORT, PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT, SYNCHRONIZER_PORT,
    TAG_STORE_PORT, WORK_DISCOVERY_PORT,
};
pub use central_reference_connectors::{
    create_default_connector_registry, FilesystemWorkConnector, InMemoryMachineConnector,
    SharedMachineState, StaticMachineInspectorConnector, StaticWorkConnector,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
