#![forbid(unsafe_code)]

mod config;
mod embedding;
mod error;
mod evaluation;
mod model;
mod query;
mod scorer;
mod store;

pub use config::{
    CompactionPolicy, EmbeddingProviderKind, EngineConfig, EngineTuningInfo, IngestionPolicy,
    RecallScorerKind, RecallScoringProfile, RetentionPolicy,
};
pub use embedding::{
    ConfiguredSemanticEmbedder, DeterministicLocalEmbedder, DisabledEmbedder, EmbeddingVector,
    SemanticEmbedder,
};
pub use error::{Error, Result};
pub use evaluation::{JudgedRecallCase, RankingMetrics, evaluate_rankings_at_k};
pub use model::{
    ArtifactPointer, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryTrustLevel,
};
pub use query::{
    CompactionReport, CompactionRequest, ExportRequest, ImportFailure, ImportMode, ImportReport,
    ImportRequest, IntegrityCheckReport, IntegrityCheckRequest, MaintenanceStats, NamespaceStats,
    OperationTrace, OperationTraceSummary, PortableRecord, PortableStorePackage, RecallExplanation,
    RecallFilters, RecallHit, RecallPlanningTrace, RecallQuery, RecallResult, RecallScoreBreakdown,
    RecallTraceCandidate, RepairReport, RepairRequest, SnapshotManifest, StoreStatsReport,
    StoreStatsRequest, TraceListRequest, TraceOperationKind, TraceStatus,
};
pub use scorer::{
    ConfiguredRecallScorer, CuratedRecallScorer, ProfileRecallScorer, RecallScorer,
    ScoredRecallCandidate,
};
pub use store::{
    BatchUpsertRequest, DeleteReceipt, DeleteRequest, MemoryStore, UpsertReceipt, UpsertRequest,
};

pub const CRATE_NAME: &str = "mnemara-core";
