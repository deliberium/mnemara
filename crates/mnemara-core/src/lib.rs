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
    RecallPlanningProfile, RecallPolicyProfile, RecallScorerKind, RecallScoringProfile,
    RetentionPolicy,
};
pub use embedding::{
    ConfiguredSemanticEmbedder, DeterministicLocalEmbedder, DisabledEmbedder, EmbeddingVector,
    SemanticEmbedder, SharedSemanticEmbedder,
};
pub use error::{Error, Result};
pub use evaluation::{JudgedRecallCase, RankingMetrics, evaluate_rankings_at_k};
pub use model::{
    AffectiveAnnotation, AffectiveAnnotationProvenance, ArtifactPointer, EPISODE_SCHEMA_VERSION,
    EpisodeContext, EpisodeContinuityState, EpisodeSalience, LineageLink, LineageRelationKind,
    MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryTrustLevel,
};
pub use query::{
    CompactionReport, CompactionRequest, ExportRequest, ImportFailure, ImportMode, ImportReport,
    ImportRequest, IntegrityCheckReport, IntegrityCheckRequest, MaintenanceStats, NamespaceStats,
    OperationTrace, OperationTraceSummary, PortableRecord, PortableStorePackage,
    RecallCandidateSource, RecallExplanation, RecallFilters, RecallHistoricalMode, RecallHit,
    RecallPlannerStage, RecallPlanningTrace, RecallQuery, RecallResult, RecallScoreBreakdown,
    RecallTemporalOrder, RecallTraceCandidate, RepairReport, RepairRequest, SnapshotManifest,
    StoreStatsReport, StoreStatsRequest, TraceListRequest, TraceOperationKind, TraceStatus,
};
pub use scorer::{
    ConfiguredRecallScorer, CuratedRecallScorer, PlannedRecallCandidate, ProfileRecallScorer,
    RecallPlanner, RecallPlannerMetrics, RecallScorer, ScoredRecallCandidate,
};
pub use store::{
    ArchiveReceipt, ArchiveRequest, BatchUpsertRequest, DeleteReceipt, DeleteRequest, MemoryStore,
    RecoverReceipt, RecoverRequest, SuppressReceipt, SuppressRequest, UpsertReceipt, UpsertRequest,
};

pub const CRATE_NAME: &str = "mnemara-core";
