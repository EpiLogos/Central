use crate::central_computer::{CentralComputerAccessIntent, ComputerAccessSubject};
use crate::world::{WorldError, WorldGraph, WorldRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

pub const AGENT_PROFILE_SCHEMA: &str = "central.agent-profile/v1";

/// Authored residence of an Agent profile. Scope is a source relation, not a
/// runtime AIKit Profile and not a new Agent identity namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProfileScope {
    Personal,
    Project,
}

/// Durable Central source describing how an already-existing semantic Agent is
/// intended to inhabit a personal or Project World.
///
/// The record deliberately contains only refs/intents. AIKit still resolves the
/// effective Profile/ContextResolution and Actuation still owns Agent/Agency
/// semantics and authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub schema: String,
    #[serde(rename = "ref")]
    pub profile_ref: String,
    pub revision: String,
    pub agent_ref: String,
    pub scope: AgentProfileScope,
    pub world_ref: WorldRef,
    /// When this profile was intentionally derived from another authored profile,
    /// retain the source relation rather than presenting an independently-originated
    /// Project variant.
    #[serde(default)]
    pub source_profile_ref: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub governance_refs: Vec<String>,
    /// Authored/default praxis assignments. Trust, eligibility and effective
    /// projection remain AIKit-owned.
    #[serde(default)]
    pub skill_refs: Vec<String>,
    #[serde(default)]
    pub skill_set_refs: Vec<String>,
    #[serde(default)]
    pub method_refs: Vec<String>,
    /// Authored/default repeatable-praxis assignments. Central stores only the
    /// Routine refs: proof validity, enablement, trigger and Action authority
    /// remain AIKit-owned and provider material state remains downstream.
    #[serde(default)]
    pub routine_refs: Vec<String>,
    /// Worlds whose knowledge/source horizons this profile intends to make
    /// available for downstream resolution. Presence here is not disclosure.
    #[serde(default)]
    pub ratified_world_refs: Vec<WorldRef>,
    /// Optional explicit source-horizon relations. The payload/source content is
    /// not copied into AgentProfile.
    #[serde(default)]
    pub knowledge_source_refs: Vec<String>,
    /// Refs to `central.computer-access-intent/v1`; never embedded ACLs.
    #[serde(default)]
    pub computer_access_intent_refs: Vec<String>,
    /// Existing Central placement/material-intent relations only.
    #[serde(default)]
    pub placement_intent_refs: Vec<String>,
    /// Optional desired runtime/body relation refs. These are source-side wishes;
    /// effective AIKit state is not stored here.
    #[serde(default)]
    pub operative_requirement_refs: Vec<String>,
    #[serde(default)]
    pub material_requirement_refs: Vec<String>,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

impl AgentProfile {
    pub fn new(
        profile_ref: impl Into<String>,
        revision: impl Into<String>,
        agent_ref: impl Into<String>,
        scope: AgentProfileScope,
        world_ref: WorldRef,
    ) -> Result<Self, AgentProfileError> {
        let value = Self {
            schema: AGENT_PROFILE_SCHEMA.into(),
            profile_ref: required(profile_ref.into(), "Agent Profile ref")?,
            revision: required(revision.into(), "Agent Profile revision")?,
            agent_ref: required(agent_ref.into(), "Agent ref")?,
            scope,
            world_ref: world_ref.clone(),
            source_profile_ref: None,
            role: None,
            purpose: None,
            governance_refs: Vec::new(),
            skill_refs: Vec::new(),
            skill_set_refs: Vec::new(),
            method_refs: Vec::new(),
            routine_refs: Vec::new(),
            ratified_world_refs: vec![world_ref],
            knowledge_source_refs: Vec::new(),
            computer_access_intent_refs: Vec::new(),
            placement_intent_refs: Vec::new(),
            operative_requirement_refs: Vec::new(),
            material_requirement_refs: Vec::new(),
            provenance_refs: Vec::new(),
        };
        value.validate_shape()?;
        Ok(value)
    }

    /// Validate only Central-owned source structure and World placement. This does
    /// not perform AIKit trust/eligibility, Actuation authority or Workcell
    /// materialisation checks.
    pub fn validate_against(&self, graph: &WorldGraph) -> Result<(), AgentProfileError> {
        self.validate_shape()?;
        let profile_ancestry = graph.ancestry(&self.world_ref)?;
        match self.scope {
            AgentProfileScope::Personal if profile_ancestry.len() != 1 => {
                return Err(AgentProfileError::PersonalScopeNotRoot(self.world_ref.clone()));
            }
            AgentProfileScope::Project if profile_ancestry.len() < 2 => {
                return Err(AgentProfileError::ProjectScopeIsRoot(self.world_ref.clone()));
            }
            _ => {}
        }

        for world in &self.ratified_world_refs {
            let ancestry = graph.ancestry(world)?;
            if !ancestry.contains(&self.world_ref) {
                return Err(AgentProfileError::WorldOutsideProfileScope {
                    world: world.clone(),
                    profile_world: self.world_ref.clone(),
                });
            }
        }
        Ok(())
    }

    /// Validate that referenced Central Computer access intents actually belong to
    /// this Agent. This proves the profile references access source rather than
    /// copying access/ACL semantics into itself.
    pub fn validate_computer_access_intents(
        &self,
        intents: &[CentralComputerAccessIntent],
    ) -> Result<(), AgentProfileError> {
        let by_ref = intents
            .iter()
            .map(|intent| (intent.intent_ref.as_str(), intent))
            .collect::<std::collections::BTreeMap<_, _>>();

        for intent_ref in &self.computer_access_intent_refs {
            let intent = by_ref
                .get(intent_ref.as_str())
                .ok_or_else(|| AgentProfileError::MissingComputerAccessIntent(intent_ref.clone()))?;
            match &intent.subject {
                ComputerAccessSubject::Agent { agent_ref } if agent_ref == &self.agent_ref => {}
                _ => {
                    return Err(AgentProfileError::WrongComputerAccessSubject {
                        intent_ref: intent_ref.clone(),
                        agent_ref: self.agent_ref.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Create an authored Project variant while retaining the same semantic Agent
    /// and the provenance of the source profile. The caller still chooses which
    /// fields to narrow/refine before committing the variant.
    pub fn project_variant(
        &self,
        profile_ref: impl Into<String>,
        revision: impl Into<String>,
        project_world: WorldRef,
        graph: &WorldGraph,
    ) -> Result<Self, AgentProfileError> {
        self.validate_against(graph)?;
        let ancestry = graph.ancestry(&project_world)?;
        if !ancestry.contains(&self.world_ref) || project_world == self.world_ref {
            return Err(AgentProfileError::ProjectOutsideSourceProfile {
                project_world,
                source_world: self.world_ref.clone(),
            });
        }

        let mut next = self.clone();
        next.profile_ref = required(profile_ref.into(), "Agent Profile ref")?;
        next.revision = required(revision.into(), "Agent Profile revision")?;
        next.scope = AgentProfileScope::Project;
        next.world_ref = project_world.clone();
        next.source_profile_ref = Some(self.profile_ref.clone());
        next.ratified_world_refs = vec![project_world];
        if !next.provenance_refs.contains(&self.profile_ref) {
            next.provenance_refs.push(self.profile_ref.clone());
        }
        next.validate_against(graph)?;
        Ok(next)
    }

    /// Explicit cross-product ownership handoff for structured consumers.
    pub fn handoff(&self) -> AgentProfileHandoff {
        AgentProfileHandoff {
            profile_ref: self.profile_ref.clone(),
            revision: self.revision.clone(),
            agent_ref: self.agent_ref.clone(),
            source_world: self.world_ref.clone(),
            semantic_identity_owner: "Actuation".into(),
            operational_resolution_owner: "AIKit".into(),
            materialisation_owner: "Workcell".into(),
            source_profile_is_agent_identity: false,
            source_profile_is_effective_profile: false,
            source_profile_is_material_binding: false,
        }
    }

    fn validate_shape(&self) -> Result<(), AgentProfileError> {
        if self.schema != AGENT_PROFILE_SCHEMA {
            return Err(AgentProfileError::Schema(self.schema.clone()));
        }
        required(self.profile_ref.clone(), "Agent Profile ref")?;
        required(self.revision.clone(), "Agent Profile revision")?;
        required(self.agent_ref.clone(), "Agent ref")?;
        if self.source_profile_ref.as_ref() == Some(&self.profile_ref) {
            return Err(AgentProfileError::SelfSourceProfile(self.profile_ref.clone()));
        }
        if self.ratified_world_refs.is_empty() {
            return Err(AgentProfileError::NoRatifiedWorlds);
        }
        validate_world_refs(&self.ratified_world_refs)?;
        validate_optional_text(&self.role, "Agent Profile role")?;
        validate_optional_text(&self.purpose, "Agent Profile purpose")?;
        validate_refs("governance refs", &self.governance_refs)?;
        validate_refs("Skill refs", &self.skill_refs)?;
        validate_refs("SkillSet refs", &self.skill_set_refs)?;
        validate_refs("Method refs", &self.method_refs)?;
        validate_refs("Routine refs", &self.routine_refs)?;
        validate_refs("Knowledge source refs", &self.knowledge_source_refs)?;
        validate_refs(
            "Central Computer access intent refs",
            &self.computer_access_intent_refs,
        )?;
        validate_refs("placement intent refs", &self.placement_intent_refs)?;
        validate_refs(
            "operative requirement refs",
            &self.operative_requirement_refs,
        )?;
        validate_refs("material requirement refs", &self.material_requirement_refs)?;
        validate_refs("provenance refs", &self.provenance_refs)?;
        if let Some(source) = &self.source_profile_ref {
            required(source.clone(), "source Agent Profile ref")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfileHandoff {
    pub profile_ref: String,
    pub revision: String,
    pub agent_ref: String,
    pub source_world: WorldRef,
    pub semantic_identity_owner: String,
    pub operational_resolution_owner: String,
    pub materialisation_owner: String,
    pub source_profile_is_agent_identity: bool,
    pub source_profile_is_effective_profile: bool,
    pub source_profile_is_material_binding: bool,
}

fn required(value: String, field: &str) -> Result<String, AgentProfileError> {
    if value.trim().is_empty() {
        Err(AgentProfileError::InvalidText(field.into()))
    } else {
        Ok(value)
    }
}

fn validate_optional_text(value: &Option<String>, field: &str) -> Result<(), AgentProfileError> {
    if value.as_ref().is_some_and(|item| item.trim().is_empty()) {
        Err(AgentProfileError::InvalidText(field.into()))
    } else {
        Ok(())
    }
}

fn validate_refs(field: &str, refs: &[String]) -> Result<(), AgentProfileError> {
    let mut seen = BTreeSet::new();
    for value in refs {
        if value.trim().is_empty() {
            return Err(AgentProfileError::InvalidText(field.into()));
        }
        if !seen.insert(value) {
            return Err(AgentProfileError::DuplicateRef {
                field: field.into(),
                value: value.clone(),
            });
        }
    }
    Ok(())
}

fn validate_world_refs(worlds: &[WorldRef]) -> Result<(), AgentProfileError> {
    let mut seen = BTreeSet::new();
    for world in worlds {
        if !seen.insert(world) {
            return Err(AgentProfileError::DuplicateWorld(world.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProfileError {
    Schema(String),
    InvalidText(String),
    DuplicateRef { field: String, value: String },
    DuplicateWorld(WorldRef),
    SelfSourceProfile(String),
    NoRatifiedWorlds,
    PersonalScopeNotRoot(WorldRef),
    ProjectScopeIsRoot(WorldRef),
    WorldOutsideProfileScope { world: WorldRef, profile_world: WorldRef },
    ProjectOutsideSourceProfile { project_world: WorldRef, source_world: WorldRef },
    MissingComputerAccessIntent(String),
    WrongComputerAccessSubject { intent_ref: String, agent_ref: String },
    World(WorldError),
}

impl From<WorldError> for AgentProfileError {
    fn from(value: WorldError) -> Self {
        Self::World(value)
    }
}

impl fmt::Display for AgentProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(schema) => write!(formatter, "unsupported Agent Profile schema {schema}"),
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::DuplicateRef { field, value } => write!(formatter, "{field} repeats ref {value}"),
            Self::DuplicateWorld(world) => write!(formatter, "Agent Profile repeats World {world}"),
            Self::SelfSourceProfile(profile) => write!(formatter, "Agent Profile {profile} cannot source itself"),
            Self::NoRatifiedWorlds => formatter.write_str("Agent Profile requires at least one ratified World"),
            Self::PersonalScopeNotRoot(world) => write!(formatter, "personal Agent Profile World {world} is not the root World"),
            Self::ProjectScopeIsRoot(world) => write!(formatter, "Project Agent Profile World {world} cannot be the root World"),
            Self::WorldOutsideProfileScope { world, profile_world } => write!(formatter, "ratified World {world} is outside Agent Profile World {profile_world}"),
            Self::ProjectOutsideSourceProfile { project_world, source_world } => write!(formatter, "Project World {project_world} is not a descendant of source Agent Profile World {source_world}"),
            Self::MissingComputerAccessIntent(intent) => write!(formatter, "Agent Profile references missing Central Computer access intent {intent}"),
            Self::WrongComputerAccessSubject { intent_ref, agent_ref } => write!(formatter, "Central Computer access intent {intent_ref} does not belong to Agent {agent_ref}"),
            Self::World(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl Error for AgentProfileError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::central_computer::{
        CentralComputerAccessIntent, CentralComputerProjection, ComputerAccessScope,
        ComputerAccessSubject, WorkspaceIntent,
    };
    use crate::world::WorldRecord;

    fn world(value: &str) -> WorldRef {
        WorldRef::new(value).unwrap()
    }

    fn fixture() -> (WorldGraph, WorldRef, WorldRef, WorldRef) {
        let personal = world("world:personal");
        let project = world("world:project:factory");
        let nested = world("world:project:factory:research");
        let mut graph = WorldGraph::default();
        graph
            .insert(WorldRecord::new(personal.clone(), "p1", None))
            .unwrap();
        graph
            .insert(WorldRecord::new(
                project.clone(),
                "j1",
                Some(personal.clone()),
            ))
            .unwrap();
        graph
            .insert(WorldRecord::new(
                nested.clone(),
                "n1",
                Some(project.clone()),
            ))
            .unwrap();
        (graph, personal, project, nested)
    }

    #[test]
    fn personal_profile_is_source_not_agent_runtime_or_material_identity() {
        let (graph, personal, project, _) = fixture();
        let mut profile = AgentProfile::new(
            "agent-profile:guardian",
            "ap1",
            "agent:guardian",
            AgentProfileScope::Personal,
            personal,
        )
        .unwrap();
        profile.skill_refs = vec!["skill:review-return".into()];
        profile.skill_set_refs = vec!["skill-set:personal".into()];
        profile.method_refs = vec!["method:daily-orientation".into()];
        profile.routine_refs = vec!["routine:daily-orientation".into()];
        profile.ratified_world_refs.push(project);
        profile.validate_against(&graph).unwrap();

        let handoff = profile.handoff();
        assert_eq!(handoff.agent_ref, "agent:guardian");
        assert_eq!(handoff.semantic_identity_owner, "Actuation");
        assert_eq!(handoff.operational_resolution_owner, "AIKit");
        assert_eq!(handoff.materialisation_owner, "Workcell");
        assert!(!handoff.source_profile_is_agent_identity);
        assert!(!handoff.source_profile_is_effective_profile);
        assert!(!handoff.source_profile_is_material_binding);
    }

    #[test]
    fn project_variant_preserves_agent_and_source_profile_provenance() {
        let (graph, personal, project, _) = fixture();
        let mut personal_profile = AgentProfile::new(
            "agent-profile:researcher:personal",
            "p1",
            "agent:researcher",
            AgentProfileScope::Personal,
            personal,
        )
        .unwrap();
        personal_profile.skill_set_refs = vec!["skill-set:research".into()];
        personal_profile.method_refs = vec!["method:source-return".into()];
        personal_profile.routine_refs = vec!["routine:source-return".into()];

        let project_profile = personal_profile
            .project_variant(
                "agent-profile:researcher:factory",
                "j1",
                project,
                &graph,
            )
            .unwrap();

        assert_eq!(project_profile.agent_ref, personal_profile.agent_ref);
        assert_eq!(project_profile.scope, AgentProfileScope::Project);
        assert_eq!(
            project_profile.source_profile_ref.as_deref(),
            Some("agent-profile:researcher:personal")
        );
        assert_eq!(project_profile.skill_set_refs, personal_profile.skill_set_refs);
        assert_eq!(project_profile.method_refs, personal_profile.method_refs);
        assert_eq!(project_profile.routine_refs, personal_profile.routine_refs);
        assert!(project_profile
            .provenance_refs
            .contains(&personal_profile.profile_ref));
        project_profile.validate_against(&graph).unwrap();
    }

    #[test]
    fn legacy_v1_profile_without_routine_refs_loads_with_empty_assignment() {
        let profile: AgentProfile = serde_json::from_value(serde_json::json!({
            "schema": AGENT_PROFILE_SCHEMA,
            "ref": "agent-profile:legacy",
            "revision": "p1",
            "agent_ref": "agent:legacy",
            "scope": "personal",
            "world_ref": "world:personal",
            "ratified_world_refs": ["world:personal"]
        }))
        .unwrap();
        assert!(profile.routine_refs.is_empty());
    }

    #[test]
    fn profile_references_agent_scoped_computer_access_instead_of_copying_acl() {
        let (graph, personal, project, _) = fixture();
        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            personal.clone(),
            "machine-role:central-computer",
            vec![personal.clone(), project.clone()],
        )
        .unwrap();
        computer.validate_against(&graph).unwrap();
        let access = CentralComputerAccessIntent::new(
            "computer-access:researcher",
            "a1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent {
                agent_ref: "agent:researcher".into(),
            },
            vec![ComputerAccessScope::World {
                world_ref: project,
            }],
            WorkspaceIntent::SharedComputer,
        )
        .unwrap();

        let mut profile = AgentProfile::new(
            "agent-profile:researcher",
            "p1",
            "agent:researcher",
            AgentProfileScope::Personal,
            personal,
        )
        .unwrap();
        profile.computer_access_intent_refs = vec![access.intent_ref.clone()];
        profile
            .validate_computer_access_intents(&[access.clone()])
            .unwrap();

        let mut wrong = access;
        wrong.subject = ComputerAccessSubject::Agent {
            agent_ref: "agent:someone-else".into(),
        };
        assert!(matches!(
            profile.validate_computer_access_intents(&[wrong]),
            Err(AgentProfileError::WrongComputerAccessSubject { .. })
        ));
    }

    #[test]
    fn project_profile_cannot_ratify_a_sibling_or_parent_world() {
        let (graph, personal, project, nested) = fixture();
        let mut project_profile = AgentProfile::new(
            "agent-profile:project",
            "j1",
            "agent:project",
            AgentProfileScope::Project,
            project,
        )
        .unwrap();
        project_profile.ratified_world_refs.push(nested);
        project_profile.validate_against(&graph).unwrap();

        project_profile.ratified_world_refs.push(personal);
        assert!(matches!(
            project_profile.validate_against(&graph),
            Err(AgentProfileError::WorldOutsideProfileScope { .. })
        ));
    }

    #[test]
    fn profile_rejects_duplicate_or_empty_assignment_refs() {
        let (_, personal, _, _) = fixture();
        let mut profile = AgentProfile::new(
            "agent-profile:test",
            "p1",
            "agent:test",
            AgentProfileScope::Personal,
            personal,
        )
        .unwrap();
        profile.skill_refs = vec!["skill:a".into(), "skill:a".into()];
        assert!(matches!(
            profile.validate_shape(),
            Err(AgentProfileError::DuplicateRef { .. })
        ));

        profile.skill_refs.clear();
        profile.routine_refs = vec!["routine:a".into(), "routine:a".into()];
        assert!(matches!(
            profile.validate_shape(),
            Err(AgentProfileError::DuplicateRef { .. })
        ));

        profile.routine_refs = vec!["".into()];
        assert!(matches!(
            profile.validate_shape(),
            Err(AgentProfileError::InvalidText(_))
        ));
    }
}