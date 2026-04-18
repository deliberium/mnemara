use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryScope {
    pub tenant_id: String,
    pub namespace: String,
    pub actor_id: String,
    pub conversation_id: Option<String>,
    pub session_id: Option<String>,
    pub source: String,
    pub labels: Vec<String>,
    pub trust_level: MemoryTrustLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MemoryTrustLevel {
    Untrusted,
    Observed,
    #[default]
    Derived,
    Verified,
    Pinned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryRecordKind {
    Episodic,
    Summary,
    Fact,
    Preference,
    Task,
    Artifact,
    Hypothesis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryQualityState {
    Draft,
    Active,
    Verified,
    Archived,
    Suppressed,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactPointer {
    pub uri: String,
    pub media_type: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub kind: MemoryRecordKind,
    pub content: String,
    pub summary: Option<String>,
    pub source_id: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub quality_state: MemoryQualityState,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub expires_at_unix_ms: Option<u64>,
    pub importance_score: f32,
    pub artifact: Option<ArtifactPointer>,
}
