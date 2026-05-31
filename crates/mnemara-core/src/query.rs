use crate::config::{
    EngineTuningInfo, RecallPlanningProfile, RecallPolicyProfile, RecallScorerKind,
    RecallScoringProfile,
};
use crate::model::{
    ConflictResolutionKind, ConflictReviewState, EpisodeContinuityState, MemoryHistoricalState,
    MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RecallTemporalOrder {
    #[default]
    Relevance,
    ChronologicalAsc,
    ChronologicalDesc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RecallHistoricalMode {
    #[default]
    CurrentOnly,
    IncludeHistorical,
    HistoricalOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
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
    pub episode_id: Option<String>,
    pub continuity_states: Vec<EpisodeContinuityState>,
    pub unresolved_only: bool,
    pub temporal_order: RecallTemporalOrder,
    pub historical_mode: RecallHistoricalMode,
    pub lineage_record_id: Option<String>,
    pub before_record_id: Option<String>,
    pub after_record_id: Option<String>,
    pub boundary_labels: Vec<String>,
    pub recurrence_key: Option<String>,
    pub conflict_states: Vec<ConflictReviewState>,
    pub resolution_kinds: Vec<ConflictResolutionKind>,
    pub unresolved_conflicts_only: bool,
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
pub struct TimeTravelRecallRequest {
    pub query: RecallQuery,
    pub as_of_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallScoreBreakdown {
    pub lexical: f32,
    pub semantic: f32,
    pub graph: f32,
    pub temporal: f32,
    pub metadata: f32,
    pub episodic: f32,
    pub salience: f32,
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
    pub planning_profile: Option<RecallPlanningProfile>,
    pub policy_profile: Option<RecallPolicyProfile>,
    pub scorer_kind: Option<RecallScorerKind>,
    pub scoring_profile: Option<RecallScoringProfile>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RecallPlannerStage {
    #[default]
    CandidateGeneration,
    GraphExpansion,
    Selection,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecallCandidateSource {
    Lexical,
    Semantic,
    Metadata,
    Episode,
    Graph,
    Temporal,
    Provenance,
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
    pub planner_stage: RecallPlannerStage,
    pub candidate_sources: Vec<RecallCandidateSource>,
    pub relation_reasons: Vec<String>,
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
    pub superseded_records: u64,
    pub lineage_links_created: u64,
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
    pub historical_records: u64,
    pub superseded_records: u64,
    pub lineage_links: u64,
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
#[serde(default)]
pub struct GraphInspectionRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub actor_id: Option<String>,
    pub conversation_id: Option<String>,
    pub session_id: Option<String>,
    pub include_archived: bool,
    pub include_suppressed: bool,
    pub include_deleted: bool,
    pub max_nodes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphInspectionNode {
    pub record_id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub actor_id: String,
    pub kind: MemoryRecordKind,
    pub summary: Option<String>,
    pub quality_state: MemoryQualityState,
    pub historical_state: MemoryHistoricalState,
    pub episode_id: Option<String>,
    pub continuity_state: Option<EpisodeContinuityState>,
    pub conflict_state: Option<ConflictReviewState>,
    pub importance_per_mille: u16,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraphInspectionEdgeKind {
    EpisodeMembership,
    ChronologyPrevious,
    ChronologyNext,
    Causal,
    Related,
    Lineage,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphInspectionEdge {
    pub source_id: String,
    pub target_id: String,
    pub kind: GraphInspectionEdgeKind,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphInspectionReport {
    pub generated_at_unix_ms: u64,
    pub total_records_scanned: u64,
    pub nodes: Vec<GraphInspectionNode>,
    pub edges: Vec<GraphInspectionEdge>,
    pub truncated: bool,
}

pub fn build_graph_inspection_report(
    records: &[MemoryRecord],
    request: &GraphInspectionRequest,
    generated_at_unix_ms: u64,
) -> GraphInspectionReport {
    let mut nodes = Vec::new();
    let mut included_ids = BTreeSet::new();
    let mut truncated = false;

    for record in records {
        if !graph_inspection_includes_record(record, request) {
            continue;
        }
        if request.max_nodes.is_some_and(|limit| nodes.len() >= limit) {
            truncated = true;
            break;
        }
        included_ids.insert(record.id.clone());
        nodes.push(graph_inspection_node(record));
    }

    let mut edge_keys = BTreeSet::new();
    let mut edges = Vec::new();
    for record in records {
        if !included_ids.contains(&record.id) {
            continue;
        }
        graph_inspection_edges(record, &included_ids, &mut edge_keys, &mut edges);
    }

    GraphInspectionReport {
        generated_at_unix_ms,
        total_records_scanned: records.len() as u64,
        nodes,
        edges,
        truncated,
    }
}

fn graph_inspection_includes_record(
    record: &MemoryRecord,
    request: &GraphInspectionRequest,
) -> bool {
    if request
        .tenant_id
        .as_ref()
        .is_some_and(|expected| &record.scope.tenant_id != expected)
        || request
            .namespace
            .as_ref()
            .is_some_and(|expected| &record.scope.namespace != expected)
        || request
            .actor_id
            .as_ref()
            .is_some_and(|expected| &record.scope.actor_id != expected)
        || request
            .conversation_id
            .as_ref()
            .is_some_and(|expected| record.scope.conversation_id.as_ref() != Some(expected))
        || request
            .session_id
            .as_ref()
            .is_some_and(|expected| record.scope.session_id.as_ref() != Some(expected))
    {
        return false;
    }

    match record.quality_state {
        MemoryQualityState::Archived => request.include_archived,
        MemoryQualityState::Suppressed => request.include_suppressed,
        MemoryQualityState::Deleted => request.include_deleted,
        _ => true,
    }
}

fn graph_inspection_node(record: &MemoryRecord) -> GraphInspectionNode {
    GraphInspectionNode {
        record_id: record.id.clone(),
        tenant_id: record.scope.tenant_id.clone(),
        namespace: record.scope.namespace.clone(),
        actor_id: record.scope.actor_id.clone(),
        kind: record.kind,
        summary: record.summary.clone(),
        quality_state: record.quality_state,
        historical_state: record.historical_state,
        episode_id: record
            .episode
            .as_ref()
            .map(|episode| episode.episode_id.clone()),
        continuity_state: record
            .episode
            .as_ref()
            .map(|episode| episode.continuity_state),
        conflict_state: record.conflict.as_ref().map(|conflict| conflict.state),
        importance_per_mille: (record.importance_score.clamp(0.0, 1.0) * 1000.0).round() as u16,
        updated_at_unix_ms: record.updated_at_unix_ms,
    }
}

fn push_graph_inspection_edge(
    edge_keys: &mut BTreeSet<(String, String, GraphInspectionEdgeKind)>,
    edges: &mut Vec<GraphInspectionEdge>,
    source_record_id: &str,
    target_record_id: &str,
    kind: GraphInspectionEdgeKind,
    details: Vec<String>,
) {
    if source_record_id == target_record_id {
        return;
    }
    let key = (
        source_record_id.to_string(),
        target_record_id.to_string(),
        kind,
    );
    if edge_keys.insert(key) {
        edges.push(GraphInspectionEdge {
            source_id: source_record_id.to_string(),
            target_id: target_record_id.to_string(),
            kind,
            details,
        });
    }
}

fn graph_inspection_edges(
    record: &MemoryRecord,
    included_ids: &BTreeSet<String>,
    edge_keys: &mut BTreeSet<(String, String, GraphInspectionEdgeKind)>,
    edges: &mut Vec<GraphInspectionEdge>,
) {
    if let Some(episode) = &record.episode {
        if !episode.episode_id.is_empty() {
            push_graph_inspection_edge(
                edge_keys,
                edges,
                &record.id,
                &format!("episode:{}", episode.episode_id),
                GraphInspectionEdgeKind::EpisodeMembership,
                vec![format!("continuity_state={:?}", episode.continuity_state)],
            );
        }
        if let Some(previous) = &episode.previous_record_id
            && included_ids.contains(previous)
        {
            push_graph_inspection_edge(
                edge_keys,
                edges,
                &record.id,
                previous,
                GraphInspectionEdgeKind::ChronologyPrevious,
                Vec::new(),
            );
        }
        if let Some(next) = &episode.next_record_id
            && included_ids.contains(next)
        {
            push_graph_inspection_edge(
                edge_keys,
                edges,
                &record.id,
                next,
                GraphInspectionEdgeKind::ChronologyNext,
                Vec::new(),
            );
        }
        for causal_id in &episode.causal_record_ids {
            if included_ids.contains(causal_id) {
                push_graph_inspection_edge(
                    edge_keys,
                    edges,
                    &record.id,
                    causal_id,
                    GraphInspectionEdgeKind::Causal,
                    Vec::new(),
                );
            }
        }
        for related_id in &episode.related_record_ids {
            if included_ids.contains(related_id) {
                push_graph_inspection_edge(
                    edge_keys,
                    edges,
                    &record.id,
                    related_id,
                    GraphInspectionEdgeKind::Related,
                    Vec::new(),
                );
            }
        }
    }

    for link in &record.lineage {
        if included_ids.contains(&link.record_id) {
            push_graph_inspection_edge(
                edge_keys,
                edges,
                &record.id,
                &link.record_id,
                GraphInspectionEdgeKind::Lineage,
                vec![
                    format!("relation={:?}", link.relation),
                    format!(
                        "confidence_per_mille={}",
                        (link.confidence.clamp(0.0, 1.0) * 1000.0).round() as u16
                    ),
                ],
            );
        }
    }

    if let Some(conflict) = &record.conflict {
        for conflicting_id in &conflict.conflicting_record_ids {
            if included_ids.contains(conflicting_id) {
                push_graph_inspection_edge(
                    edge_keys,
                    edges,
                    &record.id,
                    conflicting_id,
                    GraphInspectionEdgeKind::Conflict,
                    vec![
                        format!("state={:?}", conflict.state),
                        format!("resolution={:?}", conflict.resolution),
                    ],
                );
            }
        }
    }
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
#[serde(default)]
pub struct MaintenanceRunRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub dry_run: bool,
    pub reason: String,
    pub run_integrity_check: bool,
    pub run_repair: bool,
    pub run_compaction: bool,
    pub remove_stale_idempotency_keys: bool,
    pub rebuild_missing_idempotency_keys: bool,
}

impl Default for MaintenanceRunRequest {
    fn default() -> Self {
        Self {
            tenant_id: None,
            namespace: None,
            dry_run: true,
            reason: "manual maintenance run".to_string(),
            run_integrity_check: true,
            run_repair: true,
            run_compaction: true,
            remove_stale_idempotency_keys: true,
            rebuild_missing_idempotency_keys: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MaintenanceRunReport {
    pub dry_run: bool,
    pub integrity_before: Option<IntegrityCheckReport>,
    pub repair: Option<RepairReport>,
    pub compaction: Option<CompactionReport>,
    pub integrity_after: Option<IntegrityCheckReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SnapshotShipRequest {
    pub target_url: String,
    pub bearer_token: Option<String>,
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub include_archived: bool,
    pub mode: ImportMode,
    pub dry_run: bool,
}

impl Default for SnapshotShipRequest {
    fn default() -> Self {
        Self {
            target_url: String::new(),
            bearer_token: None,
            tenant_id: None,
            namespace: None,
            include_archived: false,
            mode: ImportMode::Validate,
            dry_run: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotShipReport {
    pub target_url: String,
    pub exported_records: u64,
    pub imported_records: u64,
    pub skipped_records: u64,
    pub dry_run: bool,
    pub compatible_package: bool,
    pub remote_status: u16,
    pub remote_snapshot_id: Option<String>,
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
    Archive,
    Suppress,
    Recover,
    Export,
    Import,
    MaintenanceRun,
    SnapshotShip,
    Changefeed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangefeedEventKind {
    Upserted,
    Deleted,
    Archived,
    Suppressed,
    Recovered,
    Compacted,
    Imported,
    RetentionArchived,
    RetentionDeleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ChangefeedRequest {
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub after_sequence: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChangefeedEvent {
    pub sequence: u64,
    pub event_id: String,
    pub kind: ChangefeedEventKind,
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: Option<String>,
    pub occurred_at_unix_ms: u64,
    pub summary: Option<String>,
    pub record: Option<MemoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChangefeedReport {
    pub events: Vec<ChangefeedEvent>,
    pub last_sequence: Option<u64>,
    pub truncated: bool,
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

#[cfg(test)]
mod tests {
    use super::{GraphInspectionEdgeKind, GraphInspectionRequest, build_graph_inspection_report};
    use crate::model::{
        ConflictAnnotation, ConflictReviewState, EpisodeContext, EpisodeContinuityState,
        LineageLink, LineageRelationKind, MemoryHistoricalState, MemoryQualityState, MemoryRecord,
        MemoryRecordKind, MemoryScope, MemoryTrustLevel,
    };
    use std::collections::BTreeMap;

    fn scope() -> MemoryScope {
        MemoryScope {
            tenant_id: "tenant-a".to_string(),
            namespace: "ops".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-1".to_string()),
            session_id: Some("session-1".to_string()),
            source: "test".to_string(),
            labels: Vec::new(),
            trust_level: MemoryTrustLevel::Verified,
        }
    }

    fn record(id: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            scope: scope(),
            kind: MemoryRecordKind::Episodic,
            content: format!("content {id}"),
            summary: Some(format!("summary {id}")),
            source_id: None,
            metadata: BTreeMap::new(),
            quality_state: MemoryQualityState::Active,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
            expires_at_unix_ms: None,
            importance_score: 0.5,
            artifact: None,
            episode: None,
            historical_state: MemoryHistoricalState::Current,
            lineage: Vec::new(),
            conflict: None,
        }
    }

    #[test]
    fn graph_inspection_builds_typed_edges_and_filters_hidden_records() {
        let mut seed = record("seed");
        seed.episode = Some(EpisodeContext {
            episode_id: "incident".to_string(),
            continuity_state: EpisodeContinuityState::Open,
            next_record_id: Some("next".to_string()),
            related_record_ids: vec!["next".to_string()],
            ..Default::default()
        });

        let mut next = record("next");
        next.episode = Some(EpisodeContext {
            episode_id: "incident".to_string(),
            continuity_state: EpisodeContinuityState::Open,
            previous_record_id: Some("seed".to_string()),
            causal_record_ids: vec!["seed".to_string()],
            ..Default::default()
        });
        next.lineage = vec![LineageLink {
            record_id: "seed".to_string(),
            relation: LineageRelationKind::DerivedFrom,
            confidence: 0.8,
        }];
        next.conflict = Some(ConflictAnnotation {
            state: ConflictReviewState::PotentialConflict,
            conflicting_record_ids: vec!["seed".to_string()],
            ..Default::default()
        });

        let mut hidden = record("hidden");
        hidden.quality_state = MemoryQualityState::Suppressed;

        let report = build_graph_inspection_report(
            &[seed, next, hidden],
            &GraphInspectionRequest {
                tenant_id: Some("tenant-a".to_string()),
                namespace: Some("ops".to_string()),
                ..Default::default()
            },
            42,
        );

        assert_eq!(report.generated_at_unix_ms, 42);
        assert_eq!(report.nodes.len(), 2);
        assert!(!report.nodes.iter().any(|node| node.record_id == "hidden"));
        assert!(report.edges.iter().any(|edge| matches!(
            edge.kind,
            GraphInspectionEdgeKind::ChronologyNext | GraphInspectionEdgeKind::ChronologyPrevious
        )));
        assert!(
            report
                .edges
                .iter()
                .any(|edge| edge.kind == GraphInspectionEdgeKind::Causal)
        );
        assert!(
            report
                .edges
                .iter()
                .any(|edge| edge.kind == GraphInspectionEdgeKind::Related)
        );
        assert!(
            report
                .edges
                .iter()
                .any(|edge| edge.kind == GraphInspectionEdgeKind::Lineage)
        );
        assert!(
            report
                .edges
                .iter()
                .any(|edge| edge.kind == GraphInspectionEdgeKind::Conflict)
        );
    }
}
