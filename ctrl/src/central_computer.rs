use crate::world::{AgentSetRef, WorldError, WorldGraph, WorldRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

pub const CENTRAL_COMPUTER_PROJECTION_SCHEMA: &str = "central.computer-projection/v1";
pub const CENTRAL_COMPUTER_ACCESS_SCHEMA: &str = "central.computer-access-intent/v1";

/// Source-side designation of the personal/root Central World's primary computer
/// projection. The relation has its own revision but does not mint a new World,
/// Machine, VM or Workcell identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentralComputerProjection {
    pub schema: String,
    #[serde(rename = "ref")]
    pub relation_ref: String,
    pub revision: String,
    pub root_world: WorldRef,
    /// Existing Central machine-role / machine relation. Provider and VM ids remain
    /// downstream material facts rather than this relation's identity.
    pub machine_role_ref: String,
    /// Worlds intended to be materially present/addressable through this computer.
    /// Presence here is not disclosure to every Agent.
    pub projected_worlds: Vec<WorldRef>,
    pub primary_account_computer: bool,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

impl CentralComputerProjection {
    pub fn new(
        relation_ref: impl Into<String>,
        revision: impl Into<String>,
        root_world: WorldRef,
        machine_role_ref: impl Into<String>,
        projected_worlds: Vec<WorldRef>,
    ) -> Result<Self, CentralComputerError> {
        let value = Self {
            schema: CENTRAL_COMPUTER_PROJECTION_SCHEMA.into(),
            relation_ref: required(relation_ref.into(), "Central Computer relation ref")?,
            revision: required(revision.into(), "Central Computer revision")?,
            root_world,
            machine_role_ref: required(machine_role_ref.into(), "Central Computer machine role")?,
            projected_worlds,
            primary_account_computer: true,
            provenance_refs: Vec::new(),
        };
        value.validate_shape()?;
        Ok(value)
    }

    pub fn validate_against(&self, graph: &WorldGraph) -> Result<(), CentralComputerError> {
        self.validate_shape()?;
        graph
            .get(&self.root_world)
            .ok_or_else(|| CentralComputerError::MissingProjectedWorld(self.root_world.clone()))?;
        for world in &self.projected_worlds {
            let ancestry = graph.ancestry(world)?;
            if !ancestry.contains(&self.root_world) {
                return Err(CentralComputerError::OutsideRoot {
                    world: world.clone(),
                    root: self.root_world.clone(),
                });
            }
        }
        Ok(())
    }

    /// Change intended machine/material placement without changing the semantic
    /// Central World or the identity of this projection relation.
    pub fn rebind_machine_role(
        &self,
        revision: impl Into<String>,
        machine_role_ref: impl Into<String>,
    ) -> Result<Self, CentralComputerError> {
        let mut next = self.clone();
        next.revision = required(revision.into(), "Central Computer revision")?;
        next.machine_role_ref = required(machine_role_ref.into(), "Central Computer machine role")?;
        Ok(next)
    }

    fn validate_shape(&self) -> Result<(), CentralComputerError> {
        if self.schema != CENTRAL_COMPUTER_PROJECTION_SCHEMA {
            return Err(CentralComputerError::Schema(self.schema.clone()));
        }
        if self.projected_worlds.is_empty() {
            return Err(CentralComputerError::NoProjectedWorlds);
        }
        if !self.projected_worlds.contains(&self.root_world) {
            return Err(CentralComputerError::RootNotProjected(self.root_world.clone()));
        }
        let mut seen = BTreeSet::new();
        for world in &self.projected_worlds {
            if !seen.insert(world.clone()) {
                return Err(CentralComputerError::DuplicateWorld(world.clone()));
            }
        }
        Ok(())
    }
}

/// Semantic subject whose *intended* Central access is authored here. Actual
/// Agency/authority remains Actuation-owned and effective disclosure remains AIKit-owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ComputerAccessSubject {
    Human { human_ref: String },
    RootAgency { agency_ref: String },
    Agent { agent_ref: String },
    AgentSet { agent_set_ref: AgentSetRef },
}

/// Source-side intended scope. This is deliberately not an ACL verdict and not a
/// Context payload. AIKit may narrow it further according to current source,
/// privacy, authority and disclosure conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ComputerAccessScope {
    WholeWorld { world_ref: WorldRef },
    World { world_ref: WorldRef },
    SelectedSources {
        world_ref: WorldRef,
        source_refs: Vec<String>,
    },
}

impl ComputerAccessScope {
    fn world(&self) -> &WorldRef {
        match self {
            Self::WholeWorld { world_ref }
            | Self::World { world_ref }
            | Self::SelectedSources { world_ref, .. } => world_ref,
        }
    }
}

/// Sharing/isolation is orthogonal to source breadth. It expresses an authored
/// material intention only; Workcell decides/observes the realised material body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WorkspaceIntent {
    SharedComputer,
    IsolatedWorkspace {
        #[serde(default)]
        requirement_ref: Option<String>,
    },
    StructuredAccessOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentralComputerAccessIntent {
    pub schema: String,
    #[serde(rename = "ref")]
    pub intent_ref: String,
    pub revision: String,
    pub computer_relation_ref: String,
    pub subject: ComputerAccessSubject,
    pub scopes: Vec<ComputerAccessScope>,
    /// Explicit local/source narrowing. This cannot make an otherwise-private source
    /// eligible; existing Central source/privacy law still applies downstream.
    #[serde(default)]
    pub excluded_source_refs: Vec<String>,
    pub workspace: WorkspaceIntent,
    /// Provider-neutral material requirements passed onward without making Central
    /// a Workcell scheduler/provider registry.
    #[serde(default)]
    pub material_requirement_refs: Vec<String>,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

impl CentralComputerAccessIntent {
    pub fn new(
        intent_ref: impl Into<String>,
        revision: impl Into<String>,
        computer_relation_ref: impl Into<String>,
        subject: ComputerAccessSubject,
        scopes: Vec<ComputerAccessScope>,
        workspace: WorkspaceIntent,
    ) -> Result<Self, CentralComputerError> {
        let value = Self {
            schema: CENTRAL_COMPUTER_ACCESS_SCHEMA.into(),
            intent_ref: required(intent_ref.into(), "Central Computer access intent ref")?,
            revision: required(revision.into(), "Central Computer access revision")?,
            computer_relation_ref: required(
                computer_relation_ref.into(),
                "Central Computer relation ref",
            )?,
            subject,
            scopes,
            excluded_source_refs: Vec::new(),
            workspace,
            material_requirement_refs: Vec::new(),
            provenance_refs: Vec::new(),
        };
        value.validate_shape()?;
        Ok(value)
    }

    pub fn validate_against(
        &self,
        computer: &CentralComputerProjection,
        graph: &WorldGraph,
    ) -> Result<(), CentralComputerError> {
        self.validate_shape()?;
        computer.validate_against(graph)?;
        if self.computer_relation_ref != computer.relation_ref {
            return Err(CentralComputerError::WrongComputerRelation {
                expected: computer.relation_ref.clone(),
                actual: self.computer_relation_ref.clone(),
            });
        }
        for scope in &self.scopes {
            if !computer.projected_worlds.contains(scope.world()) {
                return Err(CentralComputerError::ScopeOutsideProjection(
                    scope.world().clone(),
                ));
            }
            if let ComputerAccessScope::SelectedSources { source_refs, .. } = scope {
                if source_refs.is_empty() || source_refs.iter().any(|item| item.trim().is_empty()) {
                    return Err(CentralComputerError::InvalidSelectedSources);
                }
            }
        }
        Ok(())
    }

    /// Explicit ownership handoff so consumers cannot mistake source intent for
    /// effective disclosure or realised material isolation.
    pub fn handoff(&self) -> CentralComputerHandoff {
        CentralComputerHandoff {
            computer_relation_ref: self.computer_relation_ref.clone(),
            access_intent_ref: self.intent_ref.clone(),
            operational_resolution_owner: "AIKit".into(),
            materialisation_owner: "Workcell".into(),
            source_intent_is_effective_disclosure: false,
            source_intent_is_material_binding: false,
        }
    }

    fn validate_shape(&self) -> Result<(), CentralComputerError> {
        if self.schema != CENTRAL_COMPUTER_ACCESS_SCHEMA {
            return Err(CentralComputerError::Schema(self.schema.clone()));
        }
        if self.scopes.is_empty() {
            return Err(CentralComputerError::NoAccessScopes);
        }
        validate_subject(&self.subject)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentralComputerHandoff {
    pub computer_relation_ref: String,
    pub access_intent_ref: String,
    pub operational_resolution_owner: String,
    pub materialisation_owner: String,
    pub source_intent_is_effective_disclosure: bool,
    pub source_intent_is_material_binding: bool,
}

fn validate_subject(subject: &ComputerAccessSubject) -> Result<(), CentralComputerError> {
    let value = match subject {
        ComputerAccessSubject::Human { human_ref } => human_ref,
        ComputerAccessSubject::RootAgency { agency_ref } => agency_ref,
        ComputerAccessSubject::Agent { agent_ref } => agent_ref,
        ComputerAccessSubject::AgentSet { agent_set_ref } => &agent_set_ref.0,
    };
    if value.trim().is_empty() {
        return Err(CentralComputerError::InvalidSubject);
    }
    Ok(())
}

fn required(value: String, field: &str) -> Result<String, CentralComputerError> {
    if value.trim().is_empty() {
        Err(CentralComputerError::InvalidText(field.into()))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CentralComputerError {
    Schema(String),
    InvalidText(String),
    InvalidSubject,
    NoProjectedWorlds,
    RootNotProjected(WorldRef),
    DuplicateWorld(WorldRef),
    MissingProjectedWorld(WorldRef),
    OutsideRoot { world: WorldRef, root: WorldRef },
    NoAccessScopes,
    InvalidSelectedSources,
    WrongComputerRelation { expected: String, actual: String },
    ScopeOutsideProjection(WorldRef),
    World(WorldError),
}

impl From<WorldError> for CentralComputerError {
    fn from(value: WorldError) -> Self {
        Self::World(value)
    }
}

impl fmt::Display for CentralComputerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(schema) => write!(formatter, "unsupported Central Computer schema {schema}"),
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::InvalidSubject => formatter.write_str("Central Computer access subject cannot be empty"),
            Self::NoProjectedWorlds => formatter.write_str("Central Computer projection requires at least one World"),
            Self::RootNotProjected(world) => write!(formatter, "Central Computer root World {world} is not projected"),
            Self::DuplicateWorld(world) => write!(formatter, "Central Computer projection repeats World {world}"),
            Self::MissingProjectedWorld(world) => write!(formatter, "Central Computer projection references missing World {world}"),
            Self::OutsideRoot { world, root } => write!(formatter, "projected World {world} is not inside root World {root}"),
            Self::NoAccessScopes => formatter.write_str("Central Computer access intent requires at least one scope"),
            Self::InvalidSelectedSources => formatter.write_str("selected-source scope requires non-empty source refs"),
            Self::WrongComputerRelation { expected, actual } => write!(formatter, "access intent belongs to Central Computer {actual}, expected {expected}"),
            Self::ScopeOutsideProjection(world) => write!(formatter, "access scope World {world} is not projected on this Central Computer"),
            Self::World(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl Error for CentralComputerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{AgentSetMember, AgentSetRecord, AgentSetRegistry, WorldRecord};

    fn world(value: &str) -> WorldRef {
        WorldRef::new(value).unwrap()
    }

    fn fixture() -> (WorldGraph, WorldRef, WorldRef, WorldRef) {
        let root = world("world:personal");
        let project_a = world("world:project:a");
        let project_b = world("world:project:b");
        let mut graph = WorldGraph::default();
        graph.insert(WorldRecord::new(root.clone(), "r1", None)).unwrap();
        graph
            .insert(WorldRecord::new(project_a.clone(), "a1", Some(root.clone())))
            .unwrap();
        graph
            .insert(WorldRecord::new(project_b.clone(), "b1", Some(root.clone())))
            .unwrap();
        (graph, root, project_a, project_b)
    }

    #[test]
    fn one_primary_computer_can_hold_full_tree_without_disclosing_full_tree_to_each_agent() {
        let (graph, root, project_a, project_b) = fixture();
        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            root.clone(),
            "machine-role:central-computer",
            vec![root.clone(), project_a.clone(), project_b.clone()],
        )
        .unwrap();
        computer.validate_against(&graph).unwrap();

        let agent_a = CentralComputerAccessIntent::new(
            "computer-access:agent-a",
            "aa1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent { agent_ref: "agent:a".into() },
            vec![ComputerAccessScope::World { world_ref: project_a.clone() }],
            WorkspaceIntent::SharedComputer,
        )
        .unwrap();
        agent_a.validate_against(&computer, &graph).unwrap();

        let agent_b = CentralComputerAccessIntent::new(
            "computer-access:agent-b",
            "ab1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent { agent_ref: "agent:b".into() },
            vec![
                ComputerAccessScope::World { world_ref: project_b.clone() },
                ComputerAccessScope::SelectedSources {
                    world_ref: root.clone(),
                    source_refs: vec!["source:shared-working-rule".into()],
                },
            ],
            WorkspaceIntent::SharedComputer,
        )
        .unwrap();
        agent_b.validate_against(&computer, &graph).unwrap();

        assert_eq!(computer.projected_worlds.len(), 3);
        assert_eq!(agent_a.scopes.len(), 1);
        assert_eq!(agent_b.scopes.len(), 2);
        assert_eq!(agent_a.computer_relation_ref, agent_b.computer_relation_ref);
        assert_eq!(agent_a.handoff().operational_resolution_owner, "AIKit");
        assert!(!agent_a.handoff().source_intent_is_effective_disclosure);
    }

    #[test]
    fn AgentSet_can_request_isolated_workspace_without_forking_Central_or_AgentSet_identity() {
        let (graph, root, project_a, project_b) = fixture();
        let set_ref = AgentSetRef::new("agent-set:development").unwrap();
        let mut registry = AgentSetRegistry::default();
        let mut set = AgentSetRecord::new(set_ref.clone(), "set-r1");
        set.members.push(AgentSetMember::Agent { agent_ref: "agent:builder".into() });
        registry.insert(set).unwrap();

        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            root,
            "machine-role:central-computer",
            vec![world("world:personal"), project_a.clone(), project_b],
        )
        .unwrap();
        let access = CentralComputerAccessIntent::new(
            "computer-access:development",
            "ca1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::AgentSet { agent_set_ref: set_ref.clone() },
            vec![ComputerAccessScope::World { world_ref: project_a }],
            WorkspaceIntent::IsolatedWorkspace {
                requirement_ref: Some("workcell-requirement:isolated-development".into()),
            },
        )
        .unwrap();
        access.validate_against(&computer, &graph).unwrap();

        assert_eq!(registry.resolve(&set_ref, None).unwrap().revision, "set-r1");
        assert_eq!(access.handoff().materialisation_owner, "Workcell");
        assert!(!access.handoff().source_intent_is_material_binding);
    }

    #[test]
    fn machine_provider_rebinding_does_not_change_Central_World_or_projection_identity() {
        let (graph, root, project_a, project_b) = fixture();
        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            root.clone(),
            "machine-role:central-computer-vm",
            vec![root.clone(), project_a, project_b],
        )
        .unwrap();
        computer.validate_against(&graph).unwrap();
        let moved = computer
            .rebind_machine_role("cp2", "machine-role:central-computer-bare-metal")
            .unwrap();
        assert_eq!(moved.relation_ref, computer.relation_ref);
        assert_eq!(moved.root_world, root);
        assert_ne!(moved.machine_role_ref, computer.machine_role_ref);
    }

    #[test]
    fn structured_only_and_isolated_material_intent_are_independent_of_source_scope() {
        let (graph, root, project_a, project_b) = fixture();
        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            root.clone(),
            "machine-role:central-computer",
            vec![root, project_a.clone(), project_b],
        )
        .unwrap();
        let structured = CentralComputerAccessIntent::new(
            "computer-access:structured",
            "cs1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent { agent_ref: "agent:reader".into() },
            vec![ComputerAccessScope::SelectedSources {
                world_ref: project_a.clone(),
                source_refs: vec!["source:architecture".into()],
            }],
            WorkspaceIntent::StructuredAccessOnly,
        )
        .unwrap();
        structured.validate_against(&computer, &graph).unwrap();

        let isolated = CentralComputerAccessIntent::new(
            "computer-access:isolated",
            "ci1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent { agent_ref: "agent:builder".into() },
            vec![ComputerAccessScope::World { world_ref: project_a }],
            WorkspaceIntent::IsolatedWorkspace { requirement_ref: None },
        )
        .unwrap();
        isolated.validate_against(&computer, &graph).unwrap();
        assert_ne!(structured.workspace, isolated.workspace);
        assert_eq!(structured.handoff().operational_resolution_owner, "AIKit");
    }

    #[test]
    fn scope_outside_projected_tree_is_refused() {
        let (mut graph, root, project_a, project_b) = fixture();
        let outside = world("world:other-root");
        graph.insert(WorldRecord::new(outside.clone(), "o1", None)).unwrap();
        let computer = CentralComputerProjection::new(
            "computer-projection:personal",
            "cp1",
            root.clone(),
            "machine-role:central-computer",
            vec![root, project_a, project_b],
        )
        .unwrap();
        let intent = CentralComputerAccessIntent::new(
            "computer-access:outside",
            "co1",
            computer.relation_ref.clone(),
            ComputerAccessSubject::Agent { agent_ref: "agent:outside".into() },
            vec![ComputerAccessScope::World { world_ref: outside.clone() }],
            WorkspaceIntent::SharedComputer,
        )
        .unwrap();
        assert_eq!(
            intent.validate_against(&computer, &graph).unwrap_err(),
            CentralComputerError::ScopeOutsideProjection(outside)
        );
    }
}
