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
pub use evaluation::{
    JudgedRecallCase, RankingMetrics, RecallEvaluationAssertions, RecallEvaluationCase,
    RecallEvaluationCaseReport, RecallEvaluationReport, evaluate_rankings_at_k,
    evaluate_recall_results, run_recall_evaluation,
};
pub use model::{
    AffectiveAnnotation, AffectiveAnnotationProvenance, ArtifactPointer, ConflictAnnotation,
    ConflictResolutionKind, ConflictReviewState, EPISODE_SCHEMA_VERSION, EpisodeContext,
    EpisodeContinuityState, EpisodeSalience, LineageLink, LineageRelationKind,
    MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryTrustLevel,
};
pub use query::{
    ChangefeedEvent, ChangefeedEventKind, ChangefeedReport, ChangefeedRequest, CompactionReport,
    CompactionRequest, ExportRequest, GraphInspectionEdge, GraphInspectionEdgeKind,
    GraphInspectionNode, GraphInspectionReport, GraphInspectionRequest, ImportFailure, ImportMode,
    ImportReport, ImportRequest, IntegrityCheckReport, IntegrityCheckRequest, MaintenanceRunReport,
    MaintenanceRunRequest, MaintenanceStats, NamespaceStats, OperationTrace, OperationTraceSummary,
    PortableRecord, PortableStorePackage, RecallCandidateSource, RecallExplanation, RecallFilters,
    RecallHistoricalMode, RecallHit, RecallPlannerStage, RecallPlanningTrace, RecallQuery,
    RecallResult, RecallScoreBreakdown, RecallTemporalOrder, RecallTraceCandidate, RepairReport,
    RepairRequest, SnapshotManifest, SnapshotShipReport, SnapshotShipRequest, StoreStatsReport,
    StoreStatsRequest, SynthesisProposal, SynthesisReport, SynthesisRequest,
    TimeTravelRecallRequest, TraceListRequest, TraceOperationKind, TraceStatus,
    build_graph_inspection_report,
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
