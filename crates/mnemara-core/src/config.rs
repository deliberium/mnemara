use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum EmbeddingProviderKind {
    #[default]
    Disabled,
    DeterministicLocal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RecallScoringProfile {
    #[default]
    Balanced,
    LexicalFirst,
    ImportanceFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RecallScorerKind {
    #[default]
    Profile,
    Curated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineTuningInfo {
    pub recall_scorer_kind: RecallScorerKind,
    pub recall_scoring_profile: RecallScoringProfile,
    pub embedding_provider_kind: EmbeddingProviderKind,
    pub embedding_dimensions: usize,
    pub compaction_summarize_after_record_count: usize,
    pub compaction_cold_archive_after_days: u32,
    pub compaction_cold_archive_importance_threshold_per_mille: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineConfig {
    pub node_id: String,
    pub default_tenant: String,
    pub max_batch_size: usize,
    pub retention: RetentionPolicy,
    pub compaction: CompactionPolicy,
    pub ingestion: IngestionPolicy,
    pub recall_scorer_kind: RecallScorerKind,
    pub recall_scoring_profile: RecallScoringProfile,
    pub embedding_provider_kind: EmbeddingProviderKind,
    pub embedding_dimensions: usize,
    pub explain_recall: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            node_id: "local-node".to_string(),
            default_tenant: "default".to_string(),
            max_batch_size: 256,
            retention: RetentionPolicy::default(),
            compaction: CompactionPolicy::default(),
            ingestion: IngestionPolicy::default(),
            recall_scorer_kind: RecallScorerKind::default(),
            recall_scoring_profile: RecallScoringProfile::default(),
            embedding_provider_kind: EmbeddingProviderKind::default(),
            embedding_dimensions: 64,
            explain_recall: true,
        }
    }
}

impl EngineConfig {
    pub fn tuning_info(&self) -> EngineTuningInfo {
        EngineTuningInfo {
            recall_scorer_kind: self.recall_scorer_kind,
            recall_scoring_profile: self.recall_scoring_profile,
            embedding_provider_kind: self.embedding_provider_kind,
            embedding_dimensions: self.embedding_dimensions,
            compaction_summarize_after_record_count: self.compaction.summarize_after_record_count,
            compaction_cold_archive_after_days: self.compaction.cold_archive_after_days,
            compaction_cold_archive_importance_threshold_per_mille: self
                .compaction
                .cold_archive_importance_threshold_per_mille,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub ttl_days: u32,
    pub archive_after_days: u32,
    pub max_records_per_namespace: usize,
    pub pinned_records_exempt: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            ttl_days: 0,
            archive_after_days: 0,
            max_records_per_namespace: 5_000,
            pinned_records_exempt: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionPolicy {
    pub deduplicate_similar_records: bool,
    pub summarize_after_record_count: usize,
    pub prune_stale_graph_edges: bool,
    pub cold_archive_after_days: u32,
    pub cold_archive_importance_threshold_per_mille: u16,
    pub maintenance_interval_operations: u32,
    pub dry_run_supported: bool,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self {
            deduplicate_similar_records: true,
            summarize_after_record_count: 50,
            prune_stale_graph_edges: true,
            cold_archive_after_days: 0,
            cold_archive_importance_threshold_per_mille: 250,
            maintenance_interval_operations: 32,
            dry_run_supported: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestionPolicy {
    pub idempotent_writes_required: bool,
    pub deduplication_window_hours: u32,
    pub allow_model_derived_memories: bool,
    pub require_source_labels: bool,
}

impl Default for IngestionPolicy {
    fn default() -> Self {
        Self {
            idempotent_writes_required: true,
            deduplication_window_hours: 24,
            allow_model_derived_memories: true,
            require_source_labels: false,
        }
    }
}
