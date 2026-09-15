use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

pub const WORLD_RELATION_SCHEMA: &str = "central.world-relations/v1";
pub const AGENT_SET_SCHEMA: &str = "central.agent-set/v1";

/// Error code for a World ref that has no authored record at all.
///
/// This is *absence*, not malformed input: it is the ordinary state of a
/// project that declares no world of its own, and the answer to it is to
/// inherit the root lineage by convention. It shares the `invalid_input`
/// status with every genuinely malformed request, so the distinction is
/// carried in the code — a consumer that cannot tell absence from
/// unreadability must guess from prose, and an unreadable declaration must
/// never widen what a turn receives the way an absent one may.
pub const WORLD_DECLARATION_ABSENT_CODE: &str = "central.world_declaration_absent";

/// Canonical schema for the correction block that a corrective agent-set
/// revision carries. A separate schema string keeps the correction
/// distinguishable from the record it corrects.
pub const AGENT_SET_CORRECTION_SCHEMA: &str = "central.agent-set-correction/v1";

/// Canonical schema for the proposal block an agent-proposed agent-set
/// carries on original authorship. A generated Action can propose a
/// composition into the store, but the standing of that proposal stays
/// visible in the record: `generated-proposal` / `unrecognised` until the
/// human owner's separate act says otherwise.
pub const AGENT_SET_PROPOSAL_SCHEMA: &str = "central.agent-set-proposal/v1";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldRef(pub String);

impl WorldRef {
    pub fn new(value: impl Into<String>) -> Result<Self, WorldError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorldError::InvalidRef("WorldRef cannot be empty".into()));
        }
        Ok(Self(value))
    }
}

impl fmt::Display for WorldRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentSetRef(pub String);

impl AgentSetRef {
    pub fn new(value: impl Into<String>) -> Result<Self, WorldError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorldError::InvalidRef("AgentSetRef cannot be empty".into()));
        }
        Ok(Self(value))
    }
}

impl fmt::Display for AgentSetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceTreatment {
    Canonical,
    RetainNative,
    AgentMaintained,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSourceRelation {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub revision: String,
    pub authority: String,
    pub treatment: SourceTreatment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenanceHop {
    pub world: WorldRef,
    pub revision: String,
    pub authority: String,
    pub treatment: SourceTreatment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectiveSourceState {
    Available,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveWorldSource {
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub effective_source_world: WorldRef,
    pub effective_revision: String,
    pub authority: String,
    pub source_treatment: SourceTreatment,
    pub effective_treatment: SourceTreatment,
    pub state: EffectiveSourceState,
    pub propagation_path: Vec<WorldRef>,
    pub provenance: Vec<SourceProvenanceHop>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRecord {
    pub schema: String,
    #[serde(rename = "ref")]
    pub world_ref: WorldRef,
    pub revision: String,
    pub parent: Option<WorldRef>,
    #[serde(default)]
    pub sources: Vec<WorldSourceRelation>,
    #[serde(default)]
    pub excluded_sources: BTreeSet<String>,
    #[serde(default)]
    pub agent_sets: Vec<AgentSetRef>,
    #[serde(default)]
    pub placements: Vec<PlacementIntent>,
    /// The agent set this world offers as its default formation when no
    /// project-scope profile names one. Absent means the world expresses no
    /// default, which is the state of every record written before this field
    /// existed.
    ///
    /// A default is a proposal, not an election: naming a set here tells a
    /// reader what formation the world offers. It grants the set no authority
    /// over this world or any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent_set_ref: Option<AgentSetRef>,
}

impl WorldRecord {
    pub fn new(world_ref: WorldRef, revision: impl Into<String>, parent: Option<WorldRef>) -> Self {
        Self {
            schema: WORLD_RELATION_SCHEMA.into(),
            world_ref,
            revision: revision.into(),
            parent,
            sources: Vec::new(),
            excluded_sources: BTreeSet::new(),
            agent_sets: Vec::new(),
            placements: Vec::new(),
            default_agent_set_ref: None,
        }
    }

    /// Reject a world record whose declared default formation ref is not
    /// well formed. The check is format-only on purpose: whether the named
    /// set exists is a resolution-time question, and a world may legitimately
    /// declare a default ahead of the set's own record in the same store.
    /// Colon-form is the pasu grammar, not a set id, so it is refused here —
    /// the same drift rule the agent-set record applies to agent refs.
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        if let Some(default) = self.default_agent_set_ref.as_ref() {
            let default = default.0.as_str();
            if default.is_empty()
                || default.trim() != default
                || default.contains('\0')
                || default.contains(':')
                || default.contains(char::is_whitespace)
            {
                return Err(WorldError::InvalidRef(format!(
                    "default_agent_set_ref must be a plain agent-set id, got {default:?}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorldGraph {
    worlds: BTreeMap<WorldRef, WorldRecord>,
}

impl WorldGraph {
    pub fn insert(&mut self, world: WorldRecord) -> Result<(), WorldError> {
        world.validate()?;
        if world.parent.as_ref() == Some(&world.world_ref) {
            return Err(WorldError::Cycle(vec![world.world_ref]));
        }
        if world.schema != WORLD_RELATION_SCHEMA {
            return Err(WorldError::Schema(world.schema));
        }
        self.worlds.insert(world.world_ref.clone(), world);
        Ok(())
    }

    pub fn get(&self, world: &WorldRef) -> Option<&WorldRecord> {
        self.worlds.get(world)
    }

    pub fn ancestry(&self, world: &WorldRef) -> Result<Vec<WorldRef>, WorldError> {
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cursor = Some(world.clone());
        while let Some(current) = cursor {
            if !seen.insert(current.clone()) {
                path.push(current);
                return Err(WorldError::Cycle(path));
            }
            let record = self
                .worlds
                .get(&current)
                .ok_or_else(|| WorldError::MissingWorld(current.clone()))?;
            path.push(current);
            cursor = record.parent.clone();
        }
        path.reverse();
        Ok(path)
    }

    pub fn effective_sources(
        &self,
        world: &WorldRef,
    ) -> Result<Vec<EffectiveWorldSource>, WorldError> {
        let ancestry = self.ancestry(world)?;
        let mut effective: BTreeMap<String, EffectiveWorldSource> = BTreeMap::new();

        for (index, current) in ancestry.iter().enumerate() {
            let record = self
                .worlds
                .get(current)
                .ok_or_else(|| WorldError::MissingWorld(current.clone()))?;

            for entry in effective.values_mut() {
                if entry.propagation_path.last() != Some(current) {
                    entry.propagation_path.push(current.clone());
                }
            }

            for excluded in &record.excluded_sources {
                if let Some(entry) = effective.get_mut(excluded) {
                    entry.state = EffectiveSourceState::Excluded;
                }
            }

            for relation in &record.sources {
                if relation.source_ref.trim().is_empty() || relation.revision.trim().is_empty() {
                    return Err(WorldError::InvalidSource(relation.source_ref.clone()));
                }
                let path = ancestry[..=index].to_vec();
                let hop = SourceProvenanceHop {
                    world: current.clone(),
                    revision: relation.revision.clone(),
                    authority: relation.authority.clone(),
                    treatment: relation.treatment,
                };
                match effective.get_mut(&relation.source_ref) {
                    Some(entry) => {
                        entry.effective_source_world = current.clone();
                        entry.effective_revision = relation.revision.clone();
                        entry.authority = relation.authority.clone();
                        entry.effective_treatment = relation.treatment;
                        entry.state = EffectiveSourceState::Available;
                        entry.propagation_path = path;
                        entry.provenance.push(hop);
                    }
                    None => {
                        effective.insert(
                            relation.source_ref.clone(),
                            EffectiveWorldSource {
                                source_ref: relation.source_ref.clone(),
                                effective_source_world: current.clone(),
                                effective_revision: relation.revision.clone(),
                                authority: relation.authority.clone(),
                                source_treatment: relation.treatment,
                                effective_treatment: relation.treatment,
                                state: EffectiveSourceState::Available,
                                propagation_path: path,
                                provenance: vec![hop],
                            },
                        );
                    }
                }
            }
        }

        Ok(effective.into_values().collect())
    }

    pub fn propose_return(
        &self,
        from: &WorldRef,
        toward: &WorldRef,
        proposal_ref: impl Into<String>,
        subject_ref: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<WorldReturnProposal, WorldError> {
        let ancestry = self.ancestry(from)?;
        if !ancestry.contains(toward) || from == toward {
            return Err(WorldError::InvalidReturn {
                from: from.clone(),
                toward: toward.clone(),
            });
        }
        Ok(WorldReturnProposal {
            proposal_ref: proposal_ref.into(),
            from_world: from.clone(),
            toward_world: toward.clone(),
            subject_ref: subject_ref.into(),
            summary: summary.into(),
            recognition_required: true,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldReturnProposal {
    #[serde(rename = "ref")]
    pub proposal_ref: String,
    pub from_world: WorldRef,
    pub toward_world: WorldRef,
    pub subject_ref: String,
    pub summary: String,
    pub recognition_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AgentSetMember {
    Agent { agent_ref: String },
    AgentSet { agent_set_ref: AgentSetRef },
}

/// Authorship standing of a correction to an authored agent set. A generated
/// Action can only ever record an agent-made correction; the human owner's
/// acceptance is a separate act that no Action performs and no payload can
/// claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSetCorrectionAuthorship {
    AgentCorrection,
}

/// Provenance for a revision that corrects an earlier authored agent set.
/// Absent on original authorship, which has no correction to name.
///
/// The superseded membership is retained rather than dropped with the
/// revision: a corrected record must be able to say what it removed, or the
/// correction is exactly the unattributed rewrite of authored composition
/// that this block exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSetCorrection {
    pub schema: String,
    /// The revision this correction supersedes.
    pub supersedes_revision: String,
    /// Canonical Central Action that made the correction.
    pub origin_action: String,
    pub authorship: AgentSetCorrectionAuthorship,
    /// Why the correction was made, in the corrector's own words.
    pub reason: String,
    /// Member refs the correction removed, retained so the removal stays
    /// recoverable instead of silent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_members: Vec<String>,
}

impl AgentSetCorrection {
    /// Mint an agent-made correction. Every field is required and is rejected
    /// when blank or padded, so a correction can never be authored without
    /// saying what it supersedes and why.
    pub fn agent_correction(
        supersedes_revision: impl Into<String>,
        origin_action: impl Into<String>,
        reason: impl Into<String>,
        removed_members: Vec<String>,
    ) -> Result<Self, WorldError> {
        let correction = Self {
            schema: AGENT_SET_CORRECTION_SCHEMA.into(),
            supersedes_revision: supersedes_revision.into(),
            origin_action: origin_action.into(),
            authorship: AgentSetCorrectionAuthorship::AgentCorrection,
            reason: reason.into(),
            removed_members,
        };
        correction.validate()?;
        Ok(correction)
    }

    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        if self.schema != AGENT_SET_CORRECTION_SCHEMA {
            return Err(WorldError::Schema(self.schema.clone()));
        }
        validate_correction_text("superseded revision", &self.supersedes_revision)?;
        validate_correction_text("origin action", &self.origin_action)?;
        validate_correction_text("reason", &self.reason)?;
        Ok(())
    }
}

fn validate_correction_text(field: &str, value: &str) -> Result<(), WorldError> {
    if value.trim().is_empty() || value != value.trim() || value.contains('\0') {
        return Err(WorldError::InvalidCorrection(field.to_owned()));
    }
    Ok(())
}

/// Authorship standing of an agent-proposed agent set. A generated Action can
/// only ever record a generated proposal; the human owner's adoption is a
/// separate act that no Action performs and no payload can claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSetProposalAuthorship {
    GeneratedProposal,
}

/// Provenance for an agent-proposed agent set on original authorship. Absent
/// when the human owner authored the set directly, which needs no proposal
/// stamp. The block keeps the record's standing readable at the store: a
/// proposed set is declared source, not adopted law, and readers separate the
/// two instead of guessing from presence alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSetProposal {
    pub schema: String,
    /// Canonical Central Action that authored the proposal.
    pub origin_action: String,
    pub authorship: AgentSetProposalAuthorship,
    /// Standing of the proposal at write time. A generated Action always
    /// records `unrecognised`; recognition is the owner's separate act.
    pub recognition: AgentSetProposalRecognition,
    /// Why the composition was proposed, in the proposer's own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Recognition standing of a generated agent-set proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSetProposalRecognition {
    Unrecognised,
}

impl AgentSetProposal {
    /// Mint a generated proposal. The reason is optional but, when present,
    /// must be clean text: a proposal that says why it exists must say it
    /// plainly, not with padding or control characters.
    pub fn generated(
        origin_action: impl Into<String>,
        reason: Option<String>,
    ) -> Result<Self, WorldError> {
        let origin_action = origin_action.into();
        validate_proposal_text("origin action", &origin_action)?;
        if let Some(reason) = reason.as_deref() {
            validate_proposal_text("reason", reason)?;
        }
        Ok(Self {
            schema: AGENT_SET_PROPOSAL_SCHEMA.into(),
            origin_action,
            authorship: AgentSetProposalAuthorship::GeneratedProposal,
            recognition: AgentSetProposalRecognition::Unrecognised,
            reason,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        if self.schema != AGENT_SET_PROPOSAL_SCHEMA {
            return Err(WorldError::Schema(self.schema.clone()));
        }
        if self.authorship != AgentSetProposalAuthorship::GeneratedProposal {
            return Err(WorldError::InvalidProposal(
                "proposal authorship".to_owned(),
            ));
        }
        if self.recognition != AgentSetProposalRecognition::Unrecognised {
            return Err(WorldError::InvalidProposal(
                "proposal recognition".to_owned(),
            ));
        }
        validate_proposal_text("origin action", &self.origin_action)?;
        if let Some(reason) = self.reason.as_deref() {
            validate_proposal_text("reason", reason)?;
        }
        Ok(())
    }
}

fn validate_proposal_text(field: &str, value: &str) -> Result<(), WorldError> {
    if value.trim().is_empty() || value != value.trim() || value.contains('\0') {
        return Err(WorldError::InvalidProposal(field.to_owned()));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSetRecord {
    pub schema: String,
    #[serde(rename = "ref")]
    pub agent_set_ref: AgentSetRef,
    pub revision: String,
    #[serde(default)]
    pub members: Vec<AgentSetMember>,
    /// The agency that orchestrates this set. Absent means no orchestrator is
    /// authored, which is the state of every record written before this field
    /// existed.
    ///
    /// This is a relation, not a grant. Naming an agency here says who holds
    /// the set's orchestration among its authored relations; it confers no
    /// authority, no capability and no selection on the named agency. The
    /// orchestrator is not required to be a member: the field guardian
    /// orchestrates the product guardians without being one of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator_agent_ref: Option<String>,
    /// Provenance for a corrective revision. Absent on original authorship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<AgentSetCorrection>,
    /// Provenance for an agent-proposed set on original authorship. Absent
    /// when the human owner authored the set directly. A record carrying this
    /// block is declared generated source, not adopted law.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<AgentSetProposal>,
}

impl AgentSetRecord {
    pub fn new(agent_set_ref: AgentSetRef, revision: impl Into<String>) -> Self {
        Self {
            schema: AGENT_SET_SCHEMA.into(),
            agent_set_ref,
            revision: revision.into(),
            members: Vec::new(),
            orchestrator_agent_ref: None,
            correction: None,
            proposal: None,
        }
    }

    /// Mint a generated proposal: an ordinary record plus the provenance
    /// block that keeps its standing readable. The proposal is never
    /// recognised here — recognition is the owner's separate act.
    pub fn proposed(
        agent_set_ref: AgentSetRef,
        revision: impl Into<String>,
        members: Vec<AgentSetMember>,
        orchestrator_agent_ref: Option<String>,
        origin_action: impl Into<String>,
        reason: Option<String>,
    ) -> Result<Self, WorldError> {
        let mut set = Self::new(agent_set_ref, revision);
        set.members = members;
        set.orchestrator_agent_ref = orchestrator_agent_ref;
        set.proposal = Some(AgentSetProposal::generated(origin_action, reason)?);
        set.validate()?;
        Ok(set)
    }

    /// Reject a record whose declared composition is not well formed. The
    /// orchestrator is checked here rather than inherited from the store's
    /// `validate_ref`, which accepts anything non-blank: a new field must not
    /// enter through the hole in an existing one. Member agent refs are held
    /// to the same slash-form rule, so the harness-as-agency category error
    /// cannot re-enter through membership after being driven out of the
    /// orchestrator field.
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        if self.schema != AGENT_SET_SCHEMA {
            return Err(WorldError::Schema(self.schema.clone()));
        }
        for member in &self.members {
            if let AgentSetMember::Agent { agent_ref } = member {
                validate_relation_agent_ref(agent_ref)?;
            }
        }
        if let Some(orchestrator) = self.orchestrator_agent_ref.as_deref() {
            validate_relation_agent_ref(orchestrator)?;
        }
        if let Some(correction) = self.correction.as_ref() {
            correction.validate()?;
        }
        if let Some(proposal) = self.proposal.as_ref() {
            proposal.validate()?;
        }
        Ok(())
    }
}

/// Agent refs inside Central relation records are slash-form (`agent/<id>`),
/// matching every authored profile. The colon form is the suite's pasu grammar
/// (`central:pasu:agent:<id>`), which is a different identity: accepting it
/// here is how a harness ref like `agent:hermes` was able to pass as an
/// agency. Both are rejected, so the drift cannot re-accumulate through the
/// new field.
fn validate_relation_agent_ref(value: &str) -> Result<(), WorldError> {
    let Some(id) = value.strip_prefix("agent/") else {
        return Err(WorldError::InvalidRef(format!(
            "agent ref must be slash-form `agent/<id>`, got {value:?}"
        )));
    };
    if id.is_empty()
        || id.trim() != id
        || id.contains('\0')
        || id.contains(':')
        || id.contains('/')
        || id.contains(char::is_whitespace)
    {
        return Err(WorldError::InvalidRef(format!(
            "agent ref must be slash-form `agent/<id>`, got {value:?}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAgentSet {
    #[serde(rename = "ref")]
    pub agent_set_ref: AgentSetRef,
    pub revision: String,
    pub authored_agents: Vec<String>,
    pub resolved_agents: Vec<String>,
    pub unavailable_agents: Vec<String>,
    pub nested_sets: Vec<AgentSetRef>,
    /// The set's authored orchestrator, carried through so a consumer can
    /// explain the composition's orchestration relation without re-reading
    /// the record. Absent when the set names no orchestrator. This is a
    /// relation, not a grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator_agent_ref: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentSetRegistry {
    sets: BTreeMap<AgentSetRef, AgentSetRecord>,
}

impl AgentSetRegistry {
    pub fn insert(&mut self, set: AgentSetRecord) -> Result<(), WorldError> {
        set.validate()?;
        self.sets.insert(set.agent_set_ref.clone(), set);
        Ok(())
    }

    pub fn resolve(
        &self,
        set_ref: &AgentSetRef,
        available_agents: Option<&BTreeSet<String>>,
    ) -> Result<ResolvedAgentSet, WorldError> {
        let root = self
            .sets
            .get(set_ref)
            .ok_or_else(|| WorldError::MissingAgentSet(set_ref.clone()))?;
        let mut authored_agents = BTreeSet::new();
        let mut nested_sets = BTreeSet::new();
        let mut stack = Vec::new();
        self.collect(set_ref, &mut stack, &mut authored_agents, &mut nested_sets)?;

        let (resolved_agents, unavailable_agents): (Vec<_>, Vec<_>) =
            authored_agents.iter().cloned().partition(|agent| {
                available_agents
                    .map(|set| set.contains(agent))
                    .unwrap_or(true)
            });

        Ok(ResolvedAgentSet {
            agent_set_ref: set_ref.clone(),
            revision: root.revision.clone(),
            authored_agents: authored_agents.into_iter().collect(),
            resolved_agents,
            unavailable_agents,
            nested_sets: nested_sets.into_iter().collect(),
            orchestrator_agent_ref: root.orchestrator_agent_ref.clone(),
        })
    }

    fn collect(
        &self,
        set_ref: &AgentSetRef,
        stack: &mut Vec<AgentSetRef>,
        agents: &mut BTreeSet<String>,
        nested: &mut BTreeSet<AgentSetRef>,
    ) -> Result<(), WorldError> {
        if let Some(position) = stack.iter().position(|item| item == set_ref) {
            let mut cycle = stack[position..].to_vec();
            cycle.push(set_ref.clone());
            return Err(WorldError::AgentSetCycle(cycle));
        }
        let record = self
            .sets
            .get(set_ref)
            .ok_or_else(|| WorldError::MissingAgentSet(set_ref.clone()))?;
        stack.push(set_ref.clone());
        for member in &record.members {
            match member {
                AgentSetMember::Agent { agent_ref } => {
                    agents.insert(agent_ref.clone());
                }
                AgentSetMember::AgentSet { agent_set_ref } => {
                    nested.insert(agent_set_ref.clone());
                    self.collect(agent_set_ref, stack, agents, nested)?;
                }
            }
        }
        stack.pop();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlacementSubject {
    Agent { agent_ref: String },
    AgentSet { agent_set_ref: AgentSetRef },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlacementPreference {
    Local,
    MachineRole { role_ref: String },
    WorkcellClass { class_ref: String },
    Capability { requirement_ref: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlacementStrength {
    Required,
    Preferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementIntent {
    #[serde(rename = "ref")]
    pub intent_ref: String,
    pub subject: PlacementSubject,
    pub preference: PlacementPreference,
    pub strength: PlacementStrength,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    InvalidRef(String),
    Schema(String),
    MissingWorld(WorldRef),
    Cycle(Vec<WorldRef>),
    InvalidSource(String),
    MissingAgentSet(AgentSetRef),
    AgentSetCycle(Vec<AgentSetRef>),
    InvalidReturn { from: WorldRef, toward: WorldRef },
    InvalidCorrection(String),
    InvalidProposal(String),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRef(message) => f.write_str(message),
            Self::Schema(schema) => write!(f, "unsupported schema {schema}"),
            Self::MissingWorld(world) => write!(f, "missing World {world}"),
            Self::Cycle(path) => write!(f, "World cycle: {path:?}"),
            Self::InvalidSource(source) => write!(f, "invalid World source {source:?}"),
            Self::MissingAgentSet(set) => write!(f, "missing AgentSet {set}"),
            Self::AgentSetCycle(path) => write!(f, "AgentSet cycle: {path:?}"),
            Self::InvalidReturn { from, toward } => {
                write!(f, "{toward} is not an ancestor of {from}")
            }
            Self::InvalidCorrection(field) => write!(f, "invalid correction {field}"),
            Self::InvalidProposal(field) => write!(f, "invalid proposal {field}"),
        }
    }
}

impl Error for WorldError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(value: &str) -> WorldRef {
        WorldRef::new(value).unwrap()
    }

    fn set_ref(value: &str) -> AgentSetRef {
        AgentSetRef::new(value).unwrap()
    }

    #[test]
    fn nested_world_propagation_keeps_source_provenance_through_override_and_exclusion() {
        let root = world("world:personal");
        let project = world("world:project:oi");
        let development = world("world:project:oi:development");
        let mut graph = WorldGraph::default();

        let mut root_record = WorldRecord::new(root.clone(), "r1", None);
        root_record.sources.push(WorldSourceRelation {
            source_ref: "source:governance".into(),
            revision: "g1".into(),
            authority: "human-authored".into(),
            treatment: SourceTreatment::Canonical,
        });
        root_record.sources.push(WorldSourceRelation {
            source_ref: "source:temporary".into(),
            revision: "t1".into(),
            authority: "human-authored".into(),
            treatment: SourceTreatment::RetainNative,
        });
        graph.insert(root_record).unwrap();

        let mut project_record = WorldRecord::new(project.clone(), "p1", Some(root.clone()));
        project_record.sources.push(WorldSourceRelation {
            source_ref: "source:governance".into(),
            revision: "g2".into(),
            authority: "human-authored".into(),
            treatment: SourceTreatment::Canonical,
        });
        graph.insert(project_record).unwrap();

        let mut development_record =
            WorldRecord::new(development.clone(), "d1", Some(project.clone()));
        development_record
            .excluded_sources
            .insert("source:temporary".into());
        graph.insert(development_record).unwrap();

        let sources = graph.effective_sources(&development).unwrap();
        let governance = sources
            .iter()
            .find(|source| source.source_ref == "source:governance")
            .unwrap();
        assert_eq!(governance.effective_source_world, project);
        assert_eq!(governance.effective_revision, "g2");
        assert_eq!(governance.provenance.len(), 2);
        assert_eq!(governance.provenance[0].world, root);
        assert_eq!(
            governance.propagation_path,
            vec![root.clone(), project.clone(), development.clone()]
        );

        let temporary = sources
            .iter()
            .find(|source| source.source_ref == "source:temporary")
            .unwrap();
        assert_eq!(temporary.state, EffectiveSourceState::Excluded);
        assert_eq!(temporary.provenance[0].world, root);
    }

    #[test]
    fn nested_agent_set_keeps_authored_membership_separate_from_availability_and_rejects_cycles() {
        let developers = set_ref("agent-set:developers");
        let reviewers = set_ref("agent-set:reviewers");
        let mut registry = AgentSetRegistry::default();

        let mut review_set = AgentSetRecord::new(reviewers.clone(), "r1");
        review_set.members.push(AgentSetMember::Agent {
            agent_ref: "agent/reviewer".into(),
        });
        registry.insert(review_set).unwrap();

        let mut dev_set = AgentSetRecord::new(developers.clone(), "d1");
        dev_set.members.push(AgentSetMember::Agent {
            agent_ref: "agent/builder".into(),
        });
        dev_set.members.push(AgentSetMember::AgentSet {
            agent_set_ref: reviewers.clone(),
        });
        registry.insert(dev_set).unwrap();

        let available = BTreeSet::from(["agent/builder".to_string()]);
        let resolved = registry.resolve(&developers, Some(&available)).unwrap();
        assert_eq!(
            resolved.authored_agents,
            vec!["agent/builder", "agent/reviewer"]
        );
        assert_eq!(resolved.resolved_agents, vec!["agent/builder"]);
        assert_eq!(resolved.unavailable_agents, vec!["agent/reviewer"]);
        assert_eq!(resolved.revision, "d1");

        let mut cyclic_review = AgentSetRecord::new(reviewers.clone(), "r2");
        cyclic_review.members.push(AgentSetMember::AgentSet {
            agent_set_ref: developers.clone(),
        });
        registry.insert(cyclic_review).unwrap();
        assert!(matches!(
            registry.resolve(&developers, None),
            Err(WorldError::AgentSetCycle(_))
        ));
    }

    #[test]
    fn placement_intent_and_return_preserve_semantic_identity() {
        let root = world("world:personal");
        let development = world("world:development");
        let agent_set = set_ref("agent-set:development");
        let mut graph = WorldGraph::default();
        graph
            .insert(WorldRecord::new(root.clone(), "r1", None))
            .unwrap();
        let mut child = WorldRecord::new(development.clone(), "d1", Some(root.clone()));
        child.placements.push(PlacementIntent {
            intent_ref: "placement:development".into(),
            subject: PlacementSubject::AgentSet {
                agent_set_ref: agent_set.clone(),
            },
            preference: PlacementPreference::MachineRole {
                role_ref: "machine-role:remote-development".into(),
            },
            strength: PlacementStrength::Preferred,
            revision: "p1".into(),
        });
        graph.insert(child).unwrap();

        let record = graph.get(&development).unwrap();
        assert_eq!(
            record.placements[0].subject,
            PlacementSubject::AgentSet {
                agent_set_ref: agent_set
            }
        );
        let proposal = graph
            .propose_return(
                &development,
                &root,
                "proposal:generalise-1",
                "source:project-learning",
                "consider this as general source",
            )
            .unwrap();
        assert!(proposal.recognition_required);
        assert_eq!(proposal.from_world, development);
        assert_eq!(proposal.toward_world, root);
    }

    #[test]
    fn orchestration_and_default_are_additive_so_records_written_before_them_still_parse() {
        let legacy_set = serde_json::json!({
            "schema": AGENT_SET_SCHEMA,
            "ref": "agent-set:legacy",
            "revision": "r1",
            "members": [],
        });
        let set: AgentSetRecord = serde_json::from_value(legacy_set).unwrap();
        assert!(set.orchestrator_agent_ref.is_none());
        assert!(set.correction.is_none());

        let legacy_world = serde_json::json!({
            "schema": WORLD_RELATION_SCHEMA,
            "ref": "world:legacy",
            "revision": "w1",
            "parent": null,
        });
        let record: WorldRecord = serde_json::from_value(legacy_world).unwrap();
        assert!(record.default_agent_set_ref.is_none());

        // An unauthored default stays absent on the way out: it must never be
        // emitted as an authored null that a reader could mistake for a choice.
        let emitted = serde_json::to_value(&record).unwrap();
        assert!(emitted.get("default_agent_set_ref").is_none());
    }

    #[test]
    fn a_correction_names_what_it_supersedes_and_retains_the_membership_it_removed() {
        let operators = set_ref("agent-set:world-operators");
        let mut corrected = AgentSetRecord::new(operators.clone(), "r2");
        corrected.members.push(AgentSetMember::AgentSet {
            agent_set_ref: set_ref("agent-set:factory-bounded-acceptance-commission"),
        });
        corrected.correction = Some(
            AgentSetCorrection::agent_correction(
                "r1",
                "central.agent-set.save",
                "a harness was authored as an agency in slot 0",
                vec!["agent:hermes".to_owned()],
            )
            .unwrap(),
        );

        let mut registry = AgentSetRegistry::default();
        registry.insert(corrected).unwrap();
        let stored = registry.sets.get(&operators).unwrap();
        let correction = stored.correction.as_ref().unwrap();
        assert_eq!(correction.supersedes_revision, "r1");
        assert_eq!(correction.removed_members, vec!["agent:hermes".to_owned()]);
        assert_eq!(
            correction.authorship,
            AgentSetCorrectionAuthorship::AgentCorrection
        );
        // Removed from the live membership, retained in the correction: the
        // removal is recorded rather than silent.
        assert!(!stored.members.iter().any(|member| matches!(
            member,
            AgentSetMember::Agent { agent_ref } if agent_ref == "agent:hermes"
        )));
    }

    #[test]
    fn a_correction_cannot_be_authored_without_saying_what_it_supersedes_or_why() {
        assert!(matches!(
            AgentSetCorrection::agent_correction("", "central.agent-set.save", "reason", vec![]),
            Err(WorldError::InvalidCorrection(_))
        ));
        assert!(matches!(
            AgentSetCorrection::agent_correction(
                "r1",
                "central.agent-set.save",
                "  padded  ",
                vec![]
            ),
            Err(WorldError::InvalidCorrection(_))
        ));

        // A correction carrying a foreign schema is refused at insert, so a
        // record cannot adopt an unversioned provenance block.
        let mut set = AgentSetRecord::new(set_ref("agent-set:unversioned"), "r2");
        set.correction = Some(AgentSetCorrection {
            schema: "central.agent-set-correction/v9".into(),
            supersedes_revision: "r1".into(),
            origin_action: "central.agent-set.save".into(),
            authorship: AgentSetCorrectionAuthorship::AgentCorrection,
            reason: "reason".into(),
            removed_members: vec![],
        });
        let mut registry = AgentSetRegistry::default();
        assert!(matches!(registry.insert(set), Err(WorldError::Schema(_))));
    }

    #[test]
    fn an_orchestrator_must_be_a_slash_form_agency_ref_not_a_harness_shaped_colon_ref() {
        let mut accepted = AgentSetRecord::new(set_ref("agent-set:guardians"), "r2");
        accepted.orchestrator_agent_ref = Some("agent/oi-guardian-field".into());
        let mut registry = AgentSetRegistry::default();
        registry.insert(accepted).unwrap();

        // The colon form is the pasu grammar, not a relation-record agent ref.
        // It is exactly how a harness slipped in as an agency, so it is refused.
        let mut rejected = AgentSetRecord::new(set_ref("agent-set:operators"), "r2");
        rejected.orchestrator_agent_ref = Some("agent:hermes".into());
        let mut registry = AgentSetRegistry::default();
        assert!(matches!(
            registry.insert(rejected),
            Err(WorldError::InvalidRef(_))
        ));

        for malformed in [
            "agent/",
            "agent/ spaced",
            "agent/a:b",
            "agent/a/b",
            "oi-guardian-field",
        ] {
            let mut set = AgentSetRecord::new(set_ref("agent-set:malformed"), "r2");
            set.orchestrator_agent_ref = Some(malformed.into());
            let mut registry = AgentSetRegistry::default();
            assert!(
                matches!(registry.insert(set), Err(WorldError::InvalidRef(_))),
                "{malformed:?} should be refused"
            );
        }
    }

    #[test]
    fn member_agent_refs_are_held_to_the_same_slash_form_rule_as_orchestrators() {
        // The orchestrator rule closed the front door; members were the back
        // one. `agent:hermes` must not re-enter as an ordinary member after
        // having been driven out of the orchestrator field.
        let mut colon = AgentSetRecord::new(set_ref("agent-set:members"), "r1");
        colon.members.push(AgentSetMember::Agent {
            agent_ref: "agent:hermes".into(),
        });
        let mut registry = AgentSetRegistry::default();
        assert!(matches!(
            registry.insert(colon),
            Err(WorldError::InvalidRef(_))
        ));

        let mut slash = AgentSetRecord::new(set_ref("agent-set:members"), "r1");
        slash.members.push(AgentSetMember::Agent {
            agent_ref: "agent/hermes".into(),
        });
        slash.members.push(AgentSetMember::Agent {
            agent_ref: "agent/oi-guardian-field".into(),
        });
        let mut registry = AgentSetRegistry::default();
        registry.insert(slash).unwrap();
    }

    #[test]
    fn world_default_agent_set_ref_refuses_pasu_form_and_blank_ids() {
        // Format-only: existence is a resolution-time question, so a world may
        // declare a default ahead of the set record landing in its store.
        let mut declared = WorldRecord::new(world("control:root"), "w2", None);
        declared.default_agent_set_ref = Some(AgentSetRef::new("oi-product-guardians").unwrap());
        let mut graph = WorldGraph::default();
        graph.insert(declared).unwrap();

        for malformed in ["central:pasu:agent-set:x", " spaced ", "", "with space"] {
            let mut bad = WorldRecord::new(world("control:root"), "w2", None);
            bad.default_agent_set_ref = Some(AgentSetRef(malformed.to_owned()));
            let mut graph = WorldGraph::default();
            assert!(
                matches!(graph.insert(bad), Err(WorldError::InvalidRef(_))),
                "{malformed:?} should be refused as a default formation ref"
            );
        }
    }

    #[test]
    fn a_proposed_set_carries_generated_standing_and_the_resolution_surfaces_the_orchestrator() {
        let field = AgentSetRef::new("oi-product-guardians").unwrap();
        let proposed = AgentSetRecord::proposed(
            field.clone(),
            "r1",
            vec![
                AgentSetMember::Agent {
                    agent_ref: "agent/central-guardian".into(),
                },
                AgentSetMember::Agent {
                    agent_ref: "agent/factory-guardian".into(),
                },
            ],
            Some("agent/oi-guardian-field".into()),
            "central.agent-set.propose",
            Some("guardian formation proposal".into()),
        )
        .unwrap();
        assert!(proposed.proposal.is_some());

        // The proposal block survives a serialisation round trip and says its
        // standing on its face: generated, unrecognised, never adopted.
        let emitted = serde_json::to_value(&proposed).unwrap();
        assert_eq!(
            emitted["proposal"]["schema"],
            "central.agent-set-proposal/v1"
        );
        assert_eq!(emitted["proposal"]["authorship"], "generated-proposal");
        assert_eq!(emitted["proposal"]["recognition"], "unrecognised");
        let reparsed: AgentSetRecord = serde_json::from_value(emitted).unwrap();
        reparsed.validate().unwrap();

        // A proposal block that claims recognition is refused: recognition is
        // the owner's act, and no payload can mint it.
        let mut promoted = proposed.clone();
        if let Some(proposal) = promoted.proposal.as_mut() {
            proposal.recognition = AgentSetProposalRecognition::Unrecognised;
            proposal.origin_action = " padded ".into();
        }
        assert!(matches!(
            promoted.validate(),
            Err(WorldError::InvalidProposal(_))
        ));

        // The orchestration relation is part of what a resolution returns, so
        // a consumer can explain the composition without re-reading the record.
        let mut registry = AgentSetRegistry::default();
        registry.insert(proposed).unwrap();
        let resolved = registry.resolve(&field, None).unwrap();
        assert_eq!(
            resolved.orchestrator_agent_ref.as_deref(),
            Some("agent/oi-guardian-field")
        );

        let plain = AgentSetRecord::new(field.clone(), "r1");
        let mut registry = AgentSetRegistry::default();
        registry.insert(plain).unwrap();
        assert!(registry.resolve(&field, None).unwrap().orchestrator_agent_ref.is_none());
    }
}
