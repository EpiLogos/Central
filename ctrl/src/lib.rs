pub mod action;
pub mod agent_governance;
pub mod automation;
pub mod cli;
pub mod control;
pub mod machine;
pub mod machine_account;
pub mod personal;
pub mod picker;
pub mod projectcentral;
pub mod projectcentral_ground;
pub mod projectcentral_now;
pub mod projectcentral_flow;
pub mod source_horizon;
pub mod source_history;
pub mod world;
#[path = "projectcentral_ops.rs"]
mod projectcentral_ops_base;
pub mod projectcentral_ops {
    pub use super::projectcentral_ops_base::{
        adopt_in_place, doctor_projectcentral, ensure_root_federation, initialize_projectcentral,
        inspect_projectcentral, migrate_selected, preview_adopt, preview_migrate, DoctorCheck,
        MutationPlan, ProjectCentralDoctor, ProjectCentralInspection, ProjectCentralMutation,
        ProjectCentralOutcome, SourceSignal, WikiCandidate, PROJECT_PROVENANCE, ROOT_WIKI_REF,
    };

    pub fn register_projectcentral_actions(registry: &mut crate::action::ActionRegistry) {
        super::projectcentral_ops_base::register_projectcentral_actions(registry);
        super::projectcentral_ground::register_projectcentral_ground_actions(registry);
        super::projectcentral_now::register_projectcentral_now_actions(registry);
        super::projectcentral_flow::register_projectcentral_flow_actions(registry);
        super::source_horizon::register_source_horizon_actions(registry);
        super::source_history::register_source_history_actions(registry);
    }
}
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

    pub struct TempDir { path: PathBuf }
    impl TempDir { pub fn path(&self) -> &Path { &self.path } }
    impl Drop for TempDir { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.path); } }

    pub fn tempdir() -> io::Result<TempDir> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-projectcentral-test-{}-{nonce}-{sequence}", std::process::id()
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
pub use agent_governance::{
    apply_project_governance_relation, inspect_project_governance, inspect_root_governance,
    plan_project_governance, GovernanceApplyResult, GovernanceCandidate,
    GovernanceCompositionBoundary, GovernanceMaintenancePolicy, GovernancePlanItem,
    GovernanceProvenance, GovernanceSkippedSource, GovernanceSourceRecord,
    GovernanceSourceRelation, GovernanceTreatment, ProjectGovernanceInspection,
    ProjectGovernancePlan, RootGovernanceInspection, GOVERNANCE_RELATIONS_SCHEMA,
    GOVERNANCE_RELATIONS_SOURCE,
};
pub use automation::register_automation_actions;
pub use cli::{run_cli, run_cli_with_runtime, run_cli_with_surface, CliEnvironment, CliExecution};
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
pub use machine_account::{
    explain_account, AuthoredRoleSummary, DriftStatus, MachineAccount, MachineDriftEntry,
    MachineIdentity, MachineObservationRecord,
};
pub use personal::{create_personal_action_registry, register_personal_actions};
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
pub use projectcentral_flow::{
    adopt_flow, create_flow, list_flows, read_flow, registered_flow_records, rename_flow,
    set_flow_lifecycle, snapshot_flows_for_day, write_flow, FlowDaySnapshot, FlowList,
    FlowReading, FlowRecord, FlowRevisionReceipt, DEFAULT_FLOW_DIR, FLOW_DAY_SCHEMA,
    FLOW_HISTORY_DIR, FLOW_REGISTRY, FLOW_REGISTRY_SCHEMA,
};
pub use projectcentral_now::{
    initialize_now, inspect_now, promote as promote_now, rollover as rollover_now, NowHandoff,
    NowInspection, NowPaths, NowPolicy, NowPromotion, PromotionReceipt, RolloverReport,
    NOW_AGENT_DIR, NOW_DAY_DIR, NOW_DIR, NOW_POLICY, NOW_PROMOTIONS, NOW_USER_DIR,
    WIKI_RETURN_DIR,
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
pub use source_horizon::{
    acknowledge_project_cursor, compact_project_changes, control_source_bindings,
    project_source_bindings, read_project_change_horizon, reconcile_control_sources,
    reconcile_project_sources, CompactionReport, ObservedSource, ReconcileReport, SourceBinding,
    SourceChange, SourceChangeKind, SourceHorizon, SourceRevision, CONTROL_HORIZON_STATE,
    GROUND_RELATIONS_SCHEMA as SOURCE_HORIZON_GROUND_RELATIONS_SCHEMA,
    GROUND_RELATIONS_SOURCE as SOURCE_HORIZON_GROUND_RELATIONS_SOURCE, PROJECT_HORIZON_STATE,
    SOURCE_CHANGE_SCHEMA, SOURCE_HORIZON_PROVIDER, SOURCE_HORIZON_SCHEMA,
};
pub use source_history::{
    compare_source_history, preview_source_recovery, read_source_history,
    register_source_history_actions, CentralSourceCompare, CentralSourceHistory,
    SourceRecoveryPreview, CENTRAL_SOURCE_HISTORY_SCHEMA,
    CENTRAL_SOURCE_RECOVERY_PREVIEW_SCHEMA,
};
pub use world::{
    AgentSetMember, AgentSetRecord, AgentSetRef, AgentSetRegistry, EffectiveSourceState,
    EffectiveWorldSource, PlacementIntent, PlacementPreference, PlacementStrength, PlacementSubject,
    ResolvedAgentSet, SourceProvenanceHop, SourceTreatment as WorldSourceTreatment,
    WorldError, WorldGraph, WorldRecord, WorldRef, WorldReturnProposal, WorldSourceRelation,
    AGENT_SET_SCHEMA, WORLD_RELATION_SCHEMA,
};
pub use central_connector_sdk::{
    run_automation_conformance, run_configuration_manager_conformance,
    run_machine_inspector_conformance, run_native_open_conformance, run_native_reveal_conformance,
    run_package_manager_conformance, run_service_manager_conformance, run_synchronizer_conformance,
    run_tag_store_conformance, validate_connector_manifest, Automation, AutomationConformanceFixture,
    AutomationRunInput, AutomationRunOutput, CapabilityProbe, ConfigurationManager,
    ConfigurationManagerConformanceFixture, ConfigurationStateRequest, Connector,
    ConnectorContext, ConnectorDiagnostics, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, ConnectorSummary, MachineInspectionInput, MachineInspectionOutput,
    MachineInspector, MachineInspectorConformanceFixture, NativeOpen, NativeOpenInput,
    NativeOpenOutput, NativeReveal, NativeRevealInput, NativeRevealOutput,
    NativeTargetConformanceFixture, NotificationAuthorizationState, NotificationCapabilities,
    NotificationCapabilityRequest, NotificationDelivery, NotificationDeliveryState,
    NotificationRequest, ObservedConfiguration, ObservedPackage, ObservedService, PackageManager,
    PackageManagerConformanceFixture, PackageStateRequest, PortContract, PortError, PortErrorCode,
    ReconciliationSourceReference, ServiceManager, ServiceManagerConformanceFixture,
    ServiceStateRequest, SourceCompareOutput, SourceCompareRequest, SourceHistory,
    SourceHistoryEntry, SourceHistoryOutput, SourceHistoryRequest, SourceRevisionReadOutput,
    SourceRevisionReadRequest, StateChangePreview, StateChangeResult, SynchronizationRequest,
    Synchronizer, SynchronizerConformanceFixture, SynchronizerConformanceReport, TagReadInput,
    TagReadOutput, TagReplaceInput, TagReplaceOutput, TagStore, TagStoreConformanceFixture,
    UserNotification, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput, WorkItem,
    AUTOMATION_PORT, CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
    NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT, PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT,
    SOURCE_HISTORY_PORT, SYNCHRONIZER_PORT, TAG_STORE_PORT, USER_NOTIFICATION_PORT,
    WORK_DISCOVERY_PORT,
};
pub use central_reference_connectors::{
    create_default_connector_registry, FilesystemWorkConnector, InMemoryMachineConnector,
    SharedMachineState, StaticMachineInspectorConnector, StaticWorkConnector,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{
    inspect_central, initialize_central, resolve_central_root, MixedRootDiagnostic,
    MixedRootSignal, ResolvedRoot, RootOptions,
};
