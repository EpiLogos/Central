//! Remembered-note record (wayfinder U3.3, wave 4 cell W4-A): a selection
//! remembered into durable owner ground as a generated proposal until the human
//! owner recognises it. Recognition promotes the note; no Central Action
//! performs or claims recognition.
//!
//! Provenance law follows the W3-E AgentProfile pattern: single-variant enums
//! make human-accepted/recognised states unparseable, so no payload can claim
//! standing the typed record cannot hold. The provenance block shape is
//! reused with the note-specific divergence that the mint carries the
//! selection verbatim plus its source ref and a wall-clock timestamp (W3-E
//! profiles carry intent + revision instead).
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub const REMEMBERED_NOTE_SCHEMA: &str = "central.remembered-note/v1";
pub const REMEMBERED_NOTE_PROVENANCE_SCHEMA: &str = "central.remembered-note-provenance/v1";

/// The only durable destination Remember-this currently authors. Closed set so
/// an unknown destination is an explicit refusal, never a silent fallback.
pub const REMEMBERED_DESTINATION: &str = "remembered";

/// Authorship standing of a remembered note. A generated Action can only ever
/// author `GeneratedProposal`; the human owner's recognition is a separate act
/// that no Action performs and no payload can shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RememberedNoteAuthorship {
    GeneratedProposal,
}

/// Recognition standing of a remembered note. Until the human owner recognises
/// the proposal the only representable state is `Unrecognised`; the recognition
/// act itself is owned by the human, not by any Central Action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RememberedNoteRecognition {
    Unrecognised,
}

/// Generated-proposal provenance stamped when a selection is remembered. The
/// selection is retained verbatim; the source ref stays a world-relative ref,
/// never copied payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedNoteProvenance {
    pub schema: String,
    /// The remembered selection exactly as selected: verbatim, never rewritten,
    /// never summarised by the authoring Action.
    pub selection: String,
    /// Source ref the selection was read from (path, source ref or flow ref);
    /// the source content itself is never copied into the note.
    pub source_ref: String,
    /// Canonical Central Action that authored this proposal.
    pub origin_action: String,
    /// Wall-clock seconds when the note was authored.
    pub recorded_at_unix_seconds: u64,
    pub authorship: RememberedNoteAuthorship,
    pub recognition: RememberedNoteRecognition,
}

impl RememberedNoteProvenance {
    /// Mint the only provenance an Action can author: a generated proposal whose
    /// recognition is explicitly the human owner's separate act.
    pub fn generated_proposal(
        selection: impl Into<String>,
        source_ref: impl Into<String>,
        origin_action: impl Into<String>,
        recorded_at_unix_seconds: u64,
    ) -> Result<Self, RememberError> {
        let selection = selection.into();
        if selection.trim().is_empty() {
            return Err(RememberError::EmptySelection);
        }
        if selection != selection.trim() || selection.contains('\0') {
            return Err(RememberError::InvalidSelection);
        }
        let source_ref = source_ref.into();
        if source_ref.trim().is_empty() || source_ref != source_ref.trim() {
            return Err(RememberError::InvalidSourceRef);
        }
        let origin_action = origin_action.into();
        if origin_action.trim().is_empty() || origin_action != origin_action.trim() {
            return Err(RememberError::InvalidText("origin action".into()));
        }
        Ok(Self {
            schema: REMEMBERED_NOTE_PROVENANCE_SCHEMA.into(),
            selection,
            source_ref,
            origin_action,
            recorded_at_unix_seconds,
            authorship: RememberedNoteAuthorship::GeneratedProposal,
            recognition: RememberedNoteRecognition::Unrecognised,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), RememberError> {
        if self.schema != REMEMBERED_NOTE_PROVENANCE_SCHEMA {
            return Err(RememberError::Schema(self.schema.clone()));
        }
        if self.selection.trim().is_empty() {
            return Err(RememberError::EmptySelection);
        }
        if self.selection != self.selection.trim() || self.selection.contains('\0') {
            return Err(RememberError::InvalidSelection);
        }
        if self.source_ref.trim().is_empty() || self.source_ref != self.source_ref.trim() {
            return Err(RememberError::InvalidSourceRef);
        }
        if self.origin_action.trim().is_empty() {
            return Err(RememberError::InvalidText("origin action".into()));
        }
        // Single-variant enums make the unrepresentable states unparseable: no
        // payload can claim human recognition that the typed record cannot hold.
        match (self.authorship, self.recognition) {
            (
                RememberedNoteAuthorship::GeneratedProposal,
                RememberedNoteRecognition::Unrecognised,
            ) => Ok(()),
        }
    }
}

/// Durable remembered note: one selection carried as generated-proposal ground
/// until the human owner recognises it. Unlike AgentProfile, a remembered note
/// has no owner-authored creation path: the only way one exists is through the
/// Remember-this Action, so the provenance block is mandatory, not optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedNote {
    pub schema: String,
    #[serde(rename = "ref")]
    pub note_ref: String,
    pub destination: String,
    pub provenance: RememberedNoteProvenance,
}

impl RememberedNote {
    /// Author a generated proposal from a remembered selection. No parameter of
    /// this constructor can claim recognition; the record parses no such state.
    pub fn generated_proposal(
        selection: impl Into<String>,
        source_ref: impl Into<String>,
        origin_action: impl Into<String>,
        recorded_at_unix_seconds: u64,
    ) -> Result<Self, RememberError> {
        let selection = selection.into();
        let source_ref = source_ref.into();
        let origin_action = origin_action.into();
        let note_ref = note_ref_for(&selection, &source_ref, &origin_action);
        let value = Self {
            schema: REMEMBERED_NOTE_SCHEMA.into(),
            note_ref,
            destination: REMEMBERED_DESTINATION.into(),
            provenance: RememberedNoteProvenance::generated_proposal(
                selection,
                source_ref,
                origin_action,
                recorded_at_unix_seconds,
            )?,
        };
        value.validate_shape()?;
        Ok(value)
    }

    pub fn validate_shape(&self) -> Result<(), RememberError> {
        if self.schema != REMEMBERED_NOTE_SCHEMA {
            return Err(RememberError::Schema(self.schema.clone()));
        }
        validate_note_ref(&self.note_ref)?;
        if self.destination != REMEMBERED_DESTINATION {
            return Err(RememberError::InvalidDestination(self.destination.clone()));
        }
        self.provenance.validate()?;
        Ok(())
    }
}

fn validate_note_ref(note_ref: &str) -> Result<(), RememberError> {
    if note_ref.trim().is_empty() || note_ref != note_ref.trim() || note_ref.contains('\0') {
        Err(RememberError::InvalidNoteRef(note_ref.to_owned()))
    } else {
        Ok(())
    }
}

/// Deterministic note identity: the same selection remembered from the same
/// source by the same Action is the same note. Divergence from W3-E, which
/// takes a caller-supplied profile_ref: content addressing makes
/// duplicate-note identity detectable without trusting caller-supplied keys.
pub fn note_ref_for(selection: &str, source_ref: &str, origin_action: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in selection
        .as_bytes()
        .iter()
        .chain([0xff].iter())
        .chain(source_ref.as_bytes().iter())
        .chain([0xfe].iter())
        .chain(origin_action.as_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("remembered-note:{hash:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RememberError {
    Schema(String),
    InvalidText(String),
    InvalidNoteRef(String),
    /// The remembered selection was empty or whitespace-only; a remembered
    /// note never rewrites what the person selected.
    EmptySelection,
    /// The remembered selection carried surrounding whitespace or unsafe
    /// characters; the selection is retained verbatim or refused.
    InvalidSelection,
    /// The source ref was empty or untrimmed.
    InvalidSourceRef,
    /// The destination is not one of the closed, known destinations.
    InvalidDestination(String),
    InvalidProvenance(String),
}

impl fmt::Display for RememberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(schema) => write!(formatter, "unsupported RememberedNote schema {schema}"),
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::InvalidNoteRef(value) => {
                write!(formatter, "invalid RememberedNote ref {value:?}")
            }
            Self::EmptySelection => formatter.write_str("remembered selection cannot be empty"),
            Self::InvalidSelection => formatter.write_str(
                "remembered selection must not carry surrounding whitespace or unsafe characters",
            ),
            Self::InvalidSourceRef => {
                formatter.write_str("source ref cannot be empty or untrimmed")
            }
            Self::InvalidDestination(destination) => {
                write!(formatter, "unknown remember destination {destination:?}")
            }
            Self::InvalidProvenance(error) => {
                write!(formatter, "invalid remembered-note provenance: {error}")
            }
        }
    }
}

impl Error for RememberError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SELECTION: &str = "The horizon stays recoverable when the field owns it.";
    const SOURCE: &str = "Control/agents/governance/field-and-now/session-work-placement.md";

    #[test]
    fn generated_proposal_stamps_verbatim_selection_source_origin_and_timestamp() {
        let note =
            RememberedNote::generated_proposal(SELECTION, SOURCE, "central.remember", 42).unwrap();
        assert_eq!(note.schema, REMEMBERED_NOTE_SCHEMA);
        assert_eq!(note.destination, REMEMBERED_DESTINATION);
        let provenance = &note.provenance;
        assert_eq!(provenance.schema, REMEMBERED_NOTE_PROVENANCE_SCHEMA);
        assert_eq!(provenance.selection, SELECTION);
        assert_eq!(provenance.source_ref, SOURCE);
        assert_eq!(provenance.origin_action, "central.remember");
        assert_eq!(provenance.recorded_at_unix_seconds, 42);
        assert_eq!(
            provenance.authorship,
            RememberedNoteAuthorship::GeneratedProposal
        );
        assert_eq!(
            provenance.recognition,
            RememberedNoteRecognition::Unrecognised
        );
        note.validate_shape().unwrap();
    }

    #[test]
    fn empty_selection_is_refused() {
        for selection in ["", "   "] {
            assert_eq!(
                RememberedNote::generated_proposal(selection, SOURCE, "central.remember", 1)
                    .unwrap_err(),
                RememberError::EmptySelection
            );
        }
    }

    #[test]
    fn padded_or_unsafe_selection_is_refused() {
        for selection in [" padded", "padded ", "unsafe\0selection"] {
            assert_eq!(
                RememberedNote::generated_proposal(selection, SOURCE, "central.remember", 1)
                    .unwrap_err(),
                RememberError::InvalidSelection
            );
        }
    }

    #[test]
    fn empty_source_ref_is_refused() {
        assert_eq!(
            RememberedNote::generated_proposal(SELECTION, "  ", "central.remember", 1).unwrap_err(),
            RememberError::InvalidSourceRef
        );
    }

    #[test]
    fn note_identity_is_content_addressed_and_stable() {
        let first = note_ref_for(SELECTION, SOURCE, "central.remember");
        let second = note_ref_for(SELECTION, SOURCE, "central.remember");
        assert_eq!(first, second);
        assert!(first.starts_with("remembered-note:"));
        assert_ne!(
            first,
            note_ref_for(SELECTION, SOURCE, "projectcentral.remember")
        );
        assert_ne!(
            first,
            note_ref_for("other selection", SOURCE, "central.remember")
        );
    }

    #[test]
    fn recognised_state_is_unparseable_tampered_payload() {
        let note =
            RememberedNote::generated_proposal(SELECTION, SOURCE, "central.remember", 7).unwrap();
        let mut tampered = serde_json::to_value(&note).unwrap();
        tampered["provenance"]["recognition"] = serde_json::json!("recognised");
        assert!(serde_json::from_value::<RememberedNote>(tampered).is_err());

        let mut tampered = serde_json::to_value(&note).unwrap();
        tampered["provenance"]["authorship"] = serde_json::json!("human-accepted");
        assert!(serde_json::from_value::<RememberedNote>(tampered).is_err());
    }
}
