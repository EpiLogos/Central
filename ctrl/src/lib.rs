pub mod action;
pub mod agent_governance;
pub mod agent_profile;
pub mod agent_profile_actions;
pub mod agent_profile_store;
pub mod agent_set_actions;
pub mod agent_set_store;
pub mod automation;
pub mod central_computer;
pub mod cli;
pub mod continuous_work;
pub mod control;
pub mod control_skills;
pub mod development_field;
pub mod engineering_ground;
pub mod machine;
pub mod machine_account;
pub mod names;
pub mod pasu;
pub mod personal;
pub mod picker;
pub mod projectcentral;
#[path = "projectcentral_flow_extensions.rs"]
pub mod projectcentral_flow;
pub mod projectcentral_ground;
pub mod projectcentral_now;
#[path = "projectcentral_ops.rs"]
mod projectcentral_ops_base;
pub mod source_history;
#[path = "source_horizon_extensions.rs"]
pub mod source_horizon;
pub mod template_stamp;
pub mod wiki_read;
pub mod world;
pub mod world_map;
pub mod world_source;
pub mod projectcentral_ops {
    pub use super::projectcentral_ops_base::{
        DoctorCheck, MutationPlan, PROJECT_PROVENANCE, ProjectCentralDoctor,
        ProjectCentralInspection, ProjectCentralMutation, ProjectCentralOutcome, ROOT_WIKI_REF,
        SourceSignal, WikiCandidate, adopt_in_place, doctor_projectcentral, ensure_root_federation,
        initialize_projectcentral, inspect_projectcentral, migrate_selected, preview_adopt,
        preview_migrate,
    };
    pub(crate) use super::projectcentral_ops_base::{
        project_space_ref, project_wiki_value, write_json_new,
    };

    pub fn register_projectcentral_actions(registry: &mut crate::action::ActionRegistry) {
        super::projectcentral_ops_base::register_projectcentral_actions(registry);
        super::development_field::register_development_field_actions(registry);
        super::projectcentral_ground::register_projectcentral_ground_actions(registry);
        super::projectcentral_now::register_projectcentral_now_actions(registry);
        super::projectcentral_flow::register_projectcentral_flow_actions(registry);
        super::source_horizon::register_source_horizon_actions(registry);
        super::source_history::register_source_history_actions(registry);
        super::world_source::register_world_source_actions(registry);
        super::wiki_read::register_projectcentral_wiki_read_action(registry);
        super::continuous_work::register_actions(registry);
    }
}
pub mod recovery;
pub mod remember;
pub mod remember_actions;
pub mod remember_store;
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
pub use test_tempfile::{tempdir, TempDir};

pub use action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionInputSelection, ActionOutputDefinition, ActionRegistry, MutationClass,
    create_core_action_registry,
};
pub use agent_governance::{
    GOVERNANCE_RELATIONS_SCHEMA, GOVERNANCE_RELATIONS_SOURCE, GovernanceApplyResult,
    GovernanceCandidate, GovernanceCompositionBoundary, GovernanceMaintenancePolicy,
    GovernancePlanItem, GovernanceProvenance, GovernanceSkippedSource, GovernanceSourceRecord,
    GovernanceSourceRelation, GovernanceTreatment, ProjectGovernanceInspection,
    ProjectGovernancePlan, RootGovernanceInspection, apply_project_governance_relation,
    inspect_project_governance, inspect_root_governance, plan_project_governance,
};
pub use agent_profile::{
    AGENT_PROFILE_PROVENANCE_SCHEMA, AGENT_PROFILE_SCHEMA, AgentProfile, AgentProfileAuthorship,
    AgentProfileError, AgentProfileHandoff, AgentProfileProvenance, AgentProfileRecognition,
    AgentProfileScope,
};
pub use agent_profile_actions::{
    AGENT_PROFILE_LIST_ACTION, AGENT_PROFILE_PROPOSE_ACTION, AGENT_PROFILE_READ_ACTION,
    AGENT_PROFILE_REMOVE_ACTION, AGENT_PROFILE_SAVE_ACTION, register_agent_profile_actions,
};
pub use agent_profile_store::{
    AgentProfileReading, AgentProfileStore, AgentProfileStoreError, AgentProfileWriteReceipt,
    PROJECT_AGENT_PROFILE_DIR, ROOT_AGENT_PROFILE_DIR,
};
pub use automation::register_automation_actions;
pub use central_computer::{
    CENTRAL_COMPUTER_ACCESS_SCHEMA, CENTRAL_COMPUTER_PROJECTION_SCHEMA,
    CentralComputerAccessIntent, CentralComputerError, CentralComputerHandoff,
    CentralComputerProjection, ComputerAccessScope, ComputerAccessSubject, WorkspaceIntent,
};
pub use central_connector_sdk::{
    AUTOMATION_PORT, Automation, AutomationConformanceFixture, AutomationRunInput,
    AutomationRunOutput, CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, CapabilityProbe,
    ConfigurationManager, ConfigurationManagerConformanceFixture, ConfigurationStateRequest,
    Connector, ConnectorContext, ConnectorDiagnostics, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, ConnectorSummary, MACHINE_INSPECTOR_PORT, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, MachineInspectorConformanceFixture,
    NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT, NativeOpen, NativeOpenInput, NativeOpenOutput,
    NativeReveal, NativeRevealInput, NativeRevealOutput, NativeTargetConformanceFixture,
    NotificationAuthorizationState, NotificationCapabilities, NotificationCapabilityRequest,
    NotificationDelivery, NotificationDeliveryState, NotificationRequest, ObservedConfiguration,
    ObservedPackage, ObservedService, PACKAGE_MANAGER_PORT, PackageManager,
    PackageManagerConformanceFixture, PackageStateRequest, PortContract, PortError, PortErrorCode,
    ReconciliationSourceReference, SERVICE_MANAGER_PORT, SOURCE_HISTORY_PORT, SYNCHRONIZER_PORT,
    ServiceManager, ServiceManagerConformanceFixture, ServiceStateRequest, SourceCompareOutput,
    SourceCompareRequest, SourceHistory, SourceHistoryEntry, SourceHistoryOutput,
    SourceHistoryRequest, SourceRevisionReadOutput, SourceRevisionReadRequest, StateChangePreview,
    StateChangeResult, SynchronizationRequest, Synchronizer, SynchronizerConformanceFixture,
    SynchronizerConformanceReport, TAG_STORE_PORT, TagReadInput, TagReadOutput, TagReplaceInput,
    TagReplaceOutput, TagStore, TagStoreConformanceFixture, USER_NOTIFICATION_PORT,
    UserNotification, WORK_DISCOVERY_PORT, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput,
    WorkItem, run_automation_conformance, run_configuration_manager_conformance,
    run_machine_inspector_conformance, run_native_open_conformance, run_native_reveal_conformance,
    run_package_manager_conformance, run_service_manager_conformance, run_synchronizer_conformance,
    run_tag_store_conformance, validate_connector_manifest,
};
pub use central_reference_connectors::{
    FilesystemWorkConnector, InMemoryMachineConnector, SharedMachineState,
    StaticMachineInspectorConnector, StaticWorkConnector, create_default_connector_registry,
};
pub use cli::{CliEnvironment, CliExecution, run_cli, run_cli_with_runtime, run_cli_with_surface};
pub use control::{
    AGENT_RETRIEVAL_DENY_MARKER, CONTROL_ROOTS, ControlSearchMatch, ControlSearchResult,
    ControlSkippedSource, ControlSourceRoot, SourceClass, locate_control_root, search_control,
};
pub use control_skills::{
    CONTROL_SKILL_TREATMENT, PERSONAL_SKILL_DIR, PROJECT_SKILL_DIR, SKILL_BODY, SKILL_MANIFEST,
    SKILL_MANIFEST_SCHEMA, SkillManifest, SkillMutationReceipt, SkillProjectionPolicy,
    SkillProvenance, SkillRecord, SkillRetirement, SkillScope, SkillScopeSurface, SkillStanding,
    SkillsInspection, inspect_control_skills, read_skill_manifest, register_control_skills_actions,
    restore_skill, retire_skill,
};
pub use development_field::{
    DEVELOPMENT_READING_SCHEMA, DEVELOPMENT_RELATIONS_SCHEMA, DevelopmentFieldReading,
    DevelopmentSourceRelations, ExBinding, ExReading, MachineOiSuitePolicyReading,
    OI_SUITE_POLICY_BINDING_KIND, PROJECT_DEVELOPMENT_RELATIONS, PROJECT_SELF_DIR,
    ROOT_DEVELOPMENT_RELATIONS, ROOT_SELF_DIR, ResolvedDevelopmentSource, SelfApertureReading,
    SelfApertureStatus, SelfEnsureReceipt, TierBinding, TierReading, UnboundSelfSource, UxBinding,
    UxReading, ensure_project_self, ensure_root_self, inspect_project_development_field,
    inspect_root_development_field, read_machine_oi_suite_policy, register_development_field_actions,
};
pub use engineering_ground::{
    ENGINEERING_GROUND_DIR, ENGINEERING_GROUND_OUTPUT, ENGINEERING_GROUND_STATEMENTS,
    EngineeringGroundRender, register_engineering_ground_actions, render_engineering_ground,
    write_engineering_ground,
};
pub use machine::{
    AuthoredMachineDeclaration, ConfigurationRequirement, MACHINE_ADOPTION_SCHEMA,
    MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION, MachineAdoption,
    MachineAdoptionOutcome, MachineApplyOperation, MachineApplyOutcome, MachineApplyReport,
    MachineBinding, MachineDeclaration, MachineDeclarationError, MachineDeclarationSource,
    MachineObservationSource, MachinePlan, MachinePlanEntry, MachinePlanStatus, MachinePlanSummary,
    MachineRequirements, MachineSourceReference, MachineVerification, ObservedMachine,
    PackageRequirement, PresenceState, ServiceRequirement, WORKCELL_BINDING_KIND,
    explain_machine_adoption, explain_machine_apply, explain_machine_declaration,
    explain_machine_inspection, explain_machine_plan, explain_machine_verification,
    read_machine_declaration,
};
pub use machine_account::{
    AuthoredRoleSummary, DriftStatus, MachineAccount, MachineDriftEntry, MachineIdentity,
    MachineObservationRecord, explain_account,
};
pub use personal::{create_personal_action_registry, register_personal_actions};
pub use picker::{
    NullTerminalSurface, StdioTerminalSurface, TerminalSurface, run_guided_action_picker,
    search_action_descriptors,
};
pub use projectcentral::{
    AGENT_DIR, AGENT_GOVERNANCE_DIR, HUMAN_SOURCE_DIR, ManifestValidation, PROJECT_MANIFEST,
    PROJECT_SCHEMA, PROJECTCENTRAL_DIR, ProjectCentralManifest, ProjectCentralPaths,
    ROOT_AGENT_DIR, ROOT_AGENT_GOVERNANCE_DIR, ROOT_HUMAN_SOURCE_DIR, ROOT_WIKI_DIR,
    ROOT_WIKI_SOURCE, WIKI_DIR, WIKI_PROFILE, WIKI_SOURCE, WikiBinding, projectcentral_paths,
    read_project_manifest,
};
pub use projectcentral_flow::{
    DEFAULT_FLOW_DIR, FLOW_DATE_BOUNDARY_LAW, FLOW_DAY_SCHEMA, FLOW_HISTORY_DIR, FLOW_REGISTRY,
    FLOW_REGISTRY_SCHEMA, FlowAtRestDisclosure, FlowDayFacts, FlowDayGroup, FlowDaySnapshot,
    FlowEngagement, FlowList, FlowNowEntry, FlowNowView, FlowReading, FlowRecord,
    FlowRevisionReceipt, FlowThinkingDisclosure, adopt_flow, create_flow, flow_now_view,
    inspect_flow, list_flows, read_flow, registered_flow_records, rename_flow, set_flow_lifecycle,
    snapshot_flows_for_day, write_flow,
};
pub use projectcentral_ground::{
    GROUND_RELATIONS_DIR, GROUND_RELATIONS_SCHEMA, GROUND_RELATIONS_SOURCE, GroundAccountHandoff,
    GroundApplyResult, GroundCandidate, GroundInspection, GroundPlan, GroundPlanItem,
    GroundReturnPolicy, GroundSkippedSource, GroundSourceRecord, GroundSourceRelation,
    GroundStatus, SourceProvenance, SourceStanding, SourceTreatment,
    apply_accepted_ground_relation, inspect_project_ground, plan_project_ground,
    register_projectcentral_ground_actions,
};
pub use projectcentral_now::{
    NOW_AGENT_DIR, NOW_DAY_DIR, NOW_DIR, NOW_POLICY, NOW_PROMOTIONS, NOW_USER_DIR, NowHandoff,
    NowInspection, NowPaths, NowPolicy, NowPromotion, PromotionReceipt, RolloverReport,
    WIKI_RETURN_DIR, initialize_now, inspect_now, promote as promote_now, rollover as rollover_now,
};
pub use projectcentral_ops::{
    DoctorCheck, MutationPlan, PROJECT_PROVENANCE, ProjectCentralDoctor, ProjectCentralInspection,
    ProjectCentralMutation, ProjectCentralOutcome, ROOT_WIKI_REF, SourceSignal, WikiCandidate,
    adopt_in_place, doctor_projectcentral, ensure_root_federation, initialize_projectcentral,
    inspect_projectcentral, migrate_selected, preview_adopt, preview_migrate,
};
pub use recovery::{
    AuthoredRecoveryDeclaration, RECOVERY_DECLARATION_SCHEMA, RECOVERY_DECLARATION_VERSION,
    RecoveryDeclaration, RecoveryDeclarationSource, RecoveryPlan, RecoverySynchronizationPlan,
    RecoverySynchronizationStatus, SynchronizationDeclaration, explain_recovery,
    explain_recovery_plan,
};
pub use remember::{
    REMEMBERED_DESTINATION, REMEMBERED_NOTE_PROVENANCE_SCHEMA, REMEMBERED_NOTE_SCHEMA,
    RememberError, RememberedNote, RememberedNoteAuthorship, RememberedNoteProvenance,
    RememberedNoteRecognition,
};
pub use remember_actions::{
    CENTRAL_REMEMBER_ACTION, PROJECTCENTRAL_REMEMBER_ACTION, register_remember_actions,
};
pub use remember_store::{
    PROJECT_REMEMBERED_DIR, ROOT_REMEMBERED_DIR, RememberStore, RememberStoreError,
    RememberedNoteReading, RememberedNoteReceipt,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{
    MixedRootDiagnostic, MixedRootSignal, ResolvedRoot, RootOptions, initialize_central,
    inspect_central, resolve_central_root,
};
pub use source_history::{
    CENTRAL_SOURCE_HISTORY_SCHEMA, CENTRAL_SOURCE_RECOVERY_PREVIEW_SCHEMA, CentralSourceCompare,
    CentralSourceHistory, SourceRecoveryPreview, compare_source_history, preview_source_recovery,
    read_source_history, register_source_history_actions,
};
pub use source_horizon::{
    CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_GROUND_RELATIONS_SOURCE, CONTROL_HORIZON_STATE,
    CompactionReport, GROUND_RELATIONS_SCHEMA as SOURCE_HORIZON_GROUND_RELATIONS_SCHEMA,
    GROUND_RELATIONS_SOURCE as SOURCE_HORIZON_GROUND_RELATIONS_SOURCE, ObservedSource,
    PROJECT_HORIZON_STATE, ReconcileReport, SOURCE_CHANGE_SCHEMA, SOURCE_HORIZON_PROVIDER,
    SOURCE_HORIZON_SCHEMA, SourceBinding, SourceChange, SourceChangeKind, SourceHorizon,
    SourceRevision, SourceWriteAttribution, acknowledge_project_cursor, compact_project_changes,
    content_revision, control_source_bindings, project_source_bindings,
    read_project_change_horizon, reconcile_control_sources, reconcile_project_source_writes,
    reconcile_project_sources,
};
pub use template_stamp::{
    StampFile, StampFileAction, StampScope, TemplateStampPlan, TemplateStampResult,
    register_template_stamp_actions, stamp_plan_for_project, stamp_plan_for_root, stamp_project,
    stamp_root,
};
pub use wiki_read::{
    WIKI_READING_SCHEMA, WikiCounts, WikiNodeReading, WikiReadFailure, WikiReading, WikiRelation,
    WikiSourcePointer, WikiSpaceReading, read_project_wiki, read_root_wiki,
    register_central_wiki_read_action, register_projectcentral_wiki_read_action,
};
pub use world::{
    AGENT_SET_CORRECTION_SCHEMA, AGENT_SET_SCHEMA, AgentSetCorrection, AgentSetCorrectionAuthorship,
    AgentSetMember, AgentSetRecord, AgentSetRegistry, AgentSetRecord, AgentSetRef, AgentSetRegistry, EffectiveSourceState,
    EffectiveWorldSource, PlacementIntent, PlacementPreference, PlacementStrength, PlacementSubject,
    ResolvedAgentSet, SourceProvenanceHop, SourceTreatment as WorldSourceTreatment,
    WORLD_RELATION_SCHEMA, WorldError, WorldGraph, WorldRecord, WorldRef, WorldReturnProposal,
    WorldSourceRelation,
};
pub use world_map::{
    ControlMap, FlowEntry, FlowState, GroundState, NowState, PROJECT_RELATIONS_DIR,
    ProjectCentralMap, ProjectCentralState, ProjectMap, ProjectPosition, ProjectSourceSummary,
    ProjectWorldMap, ProvenanceClassified, REPROJECT_PLAN_SCHEMA, REPROJECT_RECEIPT_SCHEMA,
    RelationsState, ReprojectPlan, ReprojectReceipt, ScaffoldKind, ScaffoldStep, SourceArea,
    WORLD_MAP_SCHEMA, WikiArea, WikiSpaceState, WorkMap, WorldMap, WorldProjection,
    apply_reproject, explain_project_world_map, explain_reproject_plan, explain_reproject_receipt,
    explain_world_map, map_project_world, map_world, plan_reproject, register_world_map_actions,
};
pub use world_source::{
    WORLD_SOURCE_READING_SCHEMA, WORLD_SOURCE_WRITE_RECEIPT_SCHEMA, WorldSourceReading,
    WorldSourceWriteReceipt, read_world_source, register_world_source_actions, write_world_source,
};

pub mod files;

pub mod file_mutation;

mod source_safety;

mod source_return;

pub mod recognition;
