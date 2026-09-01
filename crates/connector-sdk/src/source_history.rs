use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{PortContract, PortError, PortOperationContract};

pub const SOURCE_HISTORY_OPERATIONS: [PortOperationContract; 3] = [
    PortOperationContract {
        name: "history",
        input_type: "SourceHistoryRequest",
        output_type: "SourceHistoryOutput",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "compare",
        input_type: "SourceCompareRequest",
        output_type: "SourceCompareOutput",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
    PortOperationContract {
        name: "read_revision",
        input_type: "SourceRevisionReadRequest",
        output_type: "SourceRevisionReadOutput",
        mutation_class: "read-only",
        preview_required: false,
        idempotent: true,
    },
];

pub const SOURCE_HISTORY_PORT: PortContract = PortContract {
    id: "SourceHistory",
    version: "1.0.0",
    purpose: "Read bounded native source lineage and differences without transferring semantic source identity or mutation authority to the history provider.",
    operations: &SOURCE_HISTORY_OPERATIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHistoryRequest {
    pub world_root: PathBuf,
    pub source_path: PathBuf,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHistoryEntry {
    pub revision: String,
    #[serde(default)]
    pub parents: Vec<String>,
    pub subject: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub authored_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHistoryOutput {
    pub provider: String,
    pub source_path: PathBuf,
    pub entries: Vec<SourceHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCompareRequest {
    pub world_root: PathBuf,
    pub source_path: PathBuf,
    pub from_revision: String,
    pub to_revision: String,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCompareOutput {
    pub provider: String,
    pub source_path: PathBuf,
    pub from_revision: String,
    pub to_revision: String,
    pub patch: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevisionReadRequest {
    pub world_root: PathBuf,
    pub source_path: PathBuf,
    pub revision: String,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevisionReadOutput {
    pub provider: String,
    pub source_path: PathBuf,
    pub revision: String,
    pub content: Vec<u8>,
    pub truncated: bool,
}

pub trait SourceHistory: Send + Sync {
    fn history(&self, input: &SourceHistoryRequest) -> Result<SourceHistoryOutput, PortError>;
    fn compare(&self, input: &SourceCompareRequest) -> Result<SourceCompareOutput, PortError>;
    fn read_revision(
        &self,
        input: &SourceRevisionReadRequest,
    ) -> Result<SourceRevisionReadOutput, PortError>;
}
