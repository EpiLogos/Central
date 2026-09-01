use serde::{Deserialize, Serialize};
use std::fmt;

pub const SOURCE_HISTORY_SCHEMA: &str = "central.source-history/v1";
pub const SOURCE_DIFFERENCE_SCHEMA: &str = "central.source-difference/v1";

/// Provider-local address for an historical state.
///
/// This is deliberately distinct from Central's current `SourceRevision`:
/// a Git commit, snapshot id, or other provider revision describes lineage,
/// while Central remains authoritative for SourceRef identity and current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHistoryRevision {
    pub provider: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHistoryEntry {
    pub schema: String,
    pub source_ref: String,
    pub source_path: String,
    pub revision: SourceHistoryRevision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision: Option<SourceHistoryRevision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authored_at_unix_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDifference {
    pub schema: String,
    pub source_ref: String,
    pub source_path: String,
    pub from_revision: SourceHistoryRevision,
    pub to_revision: SourceHistoryRevision,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounded_diff: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceHistoryError {
    Unsupported(String),
    NotFound(String),
    Provider(String),
}

impl fmt::Display for SourceHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(message) => write!(formatter, "unsupported source history operation: {message}"),
            Self::NotFound(message) => write!(formatter, "source history state not found: {message}"),
            Self::Provider(message) => write!(formatter, "source history provider error: {message}"),
        }
    }
}

impl std::error::Error for SourceHistoryError {}

/// Optional deeper lineage for a Central-owned source.
///
/// Providers may expose history and comparison, but they do not acquire source
/// identity, standing, authority, or mutation rights. Recovery is therefore
/// intentionally absent from this trait: restoring historical material must
/// return through Central's normal source mutation/proposal path.
pub trait SourceHistoryProvider {
    fn provider_id(&self) -> &str;

    fn history(
        &self,
        source_ref: &str,
        source_path: &str,
        limit: usize,
    ) -> Result<Vec<SourceHistoryEntry>, SourceHistoryError>;

    fn compare(
        &self,
        source_ref: &str,
        source_path: &str,
        from_revision: &SourceHistoryRevision,
        to_revision: &SourceHistoryRevision,
    ) -> Result<SourceDifference, SourceHistoryError>;

    fn read_historical(
        &self,
        source_ref: &str,
        source_path: &str,
        revision: &SourceHistoryRevision,
    ) -> Result<Vec<u8>, SourceHistoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixtureProvider;

    impl SourceHistoryProvider for FixtureProvider {
        fn provider_id(&self) -> &str {
            "fixture/v1"
        }

        fn history(
            &self,
            source_ref: &str,
            source_path: &str,
            limit: usize,
        ) -> Result<Vec<SourceHistoryEntry>, SourceHistoryError> {
            Ok((limit > 0)
                .then(|| SourceHistoryEntry {
                    schema: SOURCE_HISTORY_SCHEMA.to_string(),
                    source_ref: source_ref.to_string(),
                    source_path: source_path.to_string(),
                    revision: SourceHistoryRevision {
                        provider: self.provider_id().to_string(),
                        revision: "r2".to_string(),
                    },
                    parent_revision: Some(SourceHistoryRevision {
                        provider: self.provider_id().to_string(),
                        revision: "r1".to_string(),
                    }),
                    authored_at_unix_seconds: None,
                    summary: None,
                })
                .into_iter()
                .collect())
        }

        fn compare(
            &self,
            source_ref: &str,
            source_path: &str,
            from_revision: &SourceHistoryRevision,
            to_revision: &SourceHistoryRevision,
        ) -> Result<SourceDifference, SourceHistoryError> {
            Ok(SourceDifference {
                schema: SOURCE_DIFFERENCE_SCHEMA.to_string(),
                source_ref: source_ref.to_string(),
                source_path: source_path.to_string(),
                from_revision: from_revision.clone(),
                to_revision: to_revision.clone(),
                changed: from_revision != to_revision,
                bounded_diff: None,
            })
        }

        fn read_historical(
            &self,
            _source_ref: &str,
            _source_path: &str,
            revision: &SourceHistoryRevision,
        ) -> Result<Vec<u8>, SourceHistoryError> {
            Ok(revision.revision.as_bytes().to_vec())
        }
    }

    #[test]
    fn history_provider_keeps_lineage_revision_distinct_from_central_source_revision() {
        let provider = FixtureProvider;
        let entries = provider.history("source:ground", "Ground.md", 1).unwrap();
        assert_eq!(entries[0].revision.provider, "fixture/v1");
        assert_eq!(entries[0].revision.revision, "r2");
    }

    #[test]
    fn compare_is_source_scoped_and_does_not_offer_mutation() {
        let provider = FixtureProvider;
        let before = SourceHistoryRevision {
            provider: "fixture/v1".to_string(),
            revision: "r1".to_string(),
        };
        let after = SourceHistoryRevision {
            provider: "fixture/v1".to_string(),
            revision: "r2".to_string(),
        };
        let difference = provider
            .compare("source:ground", "Ground.md", &before, &after)
            .unwrap();
        assert!(difference.changed);
        assert_eq!(difference.source_ref, "source:ground");
    }
}
