use crate::config::{EngineTuningInfo, RecallScorerKind, RecallScoringProfile};
use crate::model::{
    MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecallFilters {
    pub kinds: Vec<MemoryRecordKind>,
    pub required_labels: Vec<String>,
    pub source: Option<String>,
    pub from_unix_ms: Option<u64>,
    pub to_unix_ms: Option<u64>,
    pub min_importance_score: Option<f32>,
    pub trust_levels: Vec<MemoryTrustLevel>,
    pub states: Vec<MemoryQualityState>,
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallQuery {
    pub scope: MemoryScope,
    pub query_text: String,
    pub max_items: usize,
    pub token_budget: Option<usize>,
    pub filters: RecallFilters,
    pub include_explanation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallScoreBreakdown {
    pub lexical: f32,
    pub semantic: f32,
    pub graph: f32,
    pub temporal: f32,
    pub metadata: f32,
    pub curation: f32,
    pub policy: f32,
    pub total: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallExplanation {
    pub selected_channels: Vec<String>,
    pub policy_notes: Vec<String>,
    pub trace_id: Option<String>,
    pub planning_trace: Option<RecallPlanningTrace>,
    pub scorer_kind: Option<RecallScorerKind>,
    pub scoring_profile: Option<RecallScoringProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallPlanningTrace {
    pub trace_id: String,
    pub token_budget_applied: bool,
    pub candidates: Vec<RecallTraceCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallTraceCandidate {
    pub record_id: String,
    pub kind: MemoryRecordKind,
    pub selected: bool,
    pub selection_rank: Option<u32>,
    pub matched_terms: Vec<String>,
    pub selected_channels: Vec<String>,
    pub filter_reasons: Vec<String>,
    pub decision_reason: String,
    pub breakdown: RecallScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallHit {
    pub record: MemoryRecord,
    pub breakdown: RecallScoreBreakdown,
    pub explanation: Option<RecallExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallResult {
    pub hits: Vec<RecallHit>,
    pub total_candidates_examined: usize,
    pub explanation: Option<RecallExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionRequest {
    pub tenant_id: String,
    pub namespace: Option<String>,
    pub dry_run: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionReport {
    pub deduplicated_records: u64,
    pub archived_records: u64,
    pub summarized_clusters: u64,
    pub pruned_graph_edges: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub snapshot_id: String,
    pub created_at_unix_ms: u64,
    pub namespaces: Vec<String>,
    pub record_count: u64,
    pub storage_bytes: u64,
    pub engine: EngineTuningInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StoreStatsRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamespaceStats {
    pub tenant_id: String,
    pub namespace: String,
    pub active_records: u64,
    pub archived_records: u64,
    pub deleted_records: u64,
    pub suppressed_records: u64,
    pub pinned_records: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MaintenanceStats {
    pub duplicate_candidate_groups: u64,
    pub duplicate_candidate_records: u64,
    pub tombstoned_records: u64,
    pub expired_records: u64,
    pub stale_idempotency_keys: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreStatsReport {
    pub generated_at_unix_ms: u64,
    pub total_records: u64,
    pub storage_bytes: u64,
    pub namespaces: Vec<NamespaceStats>,
    pub maintenance: MaintenanceStats,
    pub engine: EngineTuningInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IntegrityCheckRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityCheckReport {
    pub generated_at_unix_ms: u64,
    pub healthy: bool,
    pub scanned_records: u64,
    pub scanned_idempotency_keys: u64,
    pub stale_idempotency_keys: u64,
    pub missing_idempotency_keys: u64,
    pub duplicate_active_records: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepairRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub dry_run: bool,
    pub reason: String,
    pub remove_stale_idempotency_keys: bool,
    pub rebuild_missing_idempotency_keys: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairReport {
    pub dry_run: bool,
    pub scanned_records: u64,
    pub scanned_idempotency_keys: u64,
    pub removed_stale_idempotency_keys: u64,
    pub rebuilt_missing_idempotency_keys: u64,
    pub healthy_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TraceOperationKind {
    Upsert,
    BatchUpsert,
    Recall,
    Snapshot,
    Stats,
    IntegrityCheck,
    Repair,
    Compact,
    Delete,
    Export,
    Import,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TraceStatus {
    Ok,
    Rejected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OperationTraceSummary {
    pub record_id: Option<String>,
    pub request_count: Option<u32>,
    pub query_text: Option<String>,
    pub max_items: Option<u32>,
    pub token_budget: Option<u32>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationTrace {
    pub trace_id: String,
    pub correlation_id: String,
    pub operation: TraceOperationKind,
    pub transport: String,
    pub backend: Option<String>,
    pub admission_class: Option<String>,
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub principal: Option<String>,
    pub store_span_id: Option<String>,
    pub planning_trace_id: Option<String>,
    pub started_at_unix_ms: u64,
    pub completed_at_unix_ms: u64,
    pub latency_ms: u64,
    pub status: TraceStatus,
    pub status_message: Option<String>,
    pub summary: OperationTraceSummary,
    pub recall_explanation: Option<RecallExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TraceListRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub operation: Option<TraceOperationKind>,
    pub status: Option<TraceStatus>,
    pub before_started_at_unix_ms: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortableRecord {
    pub record: MemoryRecord,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExportRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortableStorePackage {
    pub package_version: u32,
    pub exported_at_unix_ms: u64,
    pub manifest: SnapshotManifest,
    pub records: Vec<PortableRecord>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImportMode {
    Validate,
    Merge,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportRequest {
    pub package: PortableStorePackage,
    pub mode: ImportMode,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportFailure {
    pub record_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportReport {
    pub mode: ImportMode,
    pub dry_run: bool,
    pub applied: bool,
    pub compatible_package: bool,
    pub package_version: u32,
    pub validated_records: u64,
    pub imported_records: u64,
    pub skipped_records: u64,
    pub replaced_existing: bool,
    pub snapshot_id: String,
    pub failed_records: Vec<ImportFailure>,
}
