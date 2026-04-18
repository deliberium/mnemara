export type MemoryTrustLevel =
  | "Untrusted"
  | "Observed"
  | "Derived"
  | "Verified"
  | "Pinned";

export type MemoryRecordKind =
  | "Episodic"
  | "Summary"
  | "Fact"
  | "Preference"
  | "Task"
  | "Artifact"
  | "Hypothesis";

export type MemoryQualityState =
  | "Draft"
  | "Active"
  | "Verified"
  | "Archived"
  | "Suppressed"
  | "Deleted";

export type RecallScorerKind = "Profile" | "Curated";

export type RecallScoringProfile =
  | "Balanced"
  | "LexicalFirst"
  | "ImportanceFirst";

export type EmbeddingProviderKind = "Disabled" | "DeterministicLocal";

export interface ArtifactPointer {
  uri: string;
  media_type?: string | null;
  checksum?: string | null;
}

export interface MemoryScope {
  tenant_id: string;
  namespace: string;
  actor_id: string;
  conversation_id?: string | null;
  session_id?: string | null;
  source: string;
  labels: string[];
  trust_level: MemoryTrustLevel;
}

export interface MemoryRecord {
  id: string;
  scope: MemoryScope;
  kind: MemoryRecordKind;
  content: string;
  summary?: string | null;
  source_id?: string | null;
  metadata: Record<string, string>;
  quality_state: MemoryQualityState;
  created_at_unix_ms: number;
  updated_at_unix_ms: number;
  expires_at_unix_ms?: number | null;
  importance_score: number;
  artifact?: ArtifactPointer | null;
}

export interface UpsertRequest {
  record: MemoryRecord;
  idempotency_key?: string | null;
}

export interface UpsertReceipt {
  record_id: string;
  deduplicated: boolean;
  summary_refreshed: boolean;
}

export interface BatchUpsertRequest {
  requests: UpsertRequest[];
}

export interface RecallFilters {
  kinds: MemoryRecordKind[];
  required_labels: string[];
  source?: string | null;
  from_unix_ms?: number | null;
  to_unix_ms?: number | null;
  min_importance_score?: number | null;
  trust_levels: MemoryTrustLevel[];
  states: MemoryQualityState[];
  include_archived: boolean;
}

export interface RecallQuery {
  scope: MemoryScope;
  query_text: string;
  max_items: number;
  token_budget?: number | null;
  filters: RecallFilters;
  include_explanation: boolean;
}

export interface RecallScoreBreakdown {
  lexical: number;
  semantic: number;
  graph: number;
  temporal: number;
  metadata: number;
  curation: number;
  policy: number;
  total: number;
}

export interface RecallTraceCandidate {
  record_id: string;
  kind: MemoryRecordKind;
  selected: boolean;
  selection_rank?: number | null;
  matched_terms: string[];
  selected_channels: string[];
  filter_reasons: string[];
  decision_reason: string;
  breakdown: RecallScoreBreakdown;
}

export interface RecallPlanningTrace {
  trace_id: string;
  token_budget_applied: boolean;
  candidates: RecallTraceCandidate[];
}

export interface RecallExplanation {
  selected_channels: string[];
  policy_notes: string[];
  trace_id?: string | null;
  planning_trace?: RecallPlanningTrace | null;
  scorer_kind?: RecallScorerKind | null;
  scoring_profile?: RecallScoringProfile | null;
}

export interface RecallHit {
  record: MemoryRecord;
  breakdown: RecallScoreBreakdown;
  explanation?: RecallExplanation | null;
}

export interface RecallResult {
  hits: RecallHit[];
  total_candidates_examined: number;
  explanation?: RecallExplanation | null;
}

export interface CompactionRequest {
  tenant_id: string;
  namespace?: string | null;
  dry_run: boolean;
  reason: string;
}

export interface CompactionReport {
  deduplicated_records: number;
  archived_records: number;
  summarized_clusters: number;
  pruned_graph_edges: number;
  dry_run: boolean;
}

export interface DeleteRequest {
  tenant_id: string;
  namespace: string;
  record_id: string;
  hard_delete: boolean;
  audit_reason: string;
}

export interface DeleteReceipt {
  record_id: string;
  tombstoned: boolean;
  hard_deleted: boolean;
}

export interface TraceListRequest {
  tenant_id?: string | null;
  namespace?: string | null;
  operation?: string | null;
  status?: string | null;
  before_started_at_unix_ms?: number | null;
  limit?: number | null;
}

export interface OperationTraceSummary {
  record_id?: string | null;
  request_count?: number | null;
  query_text?: string | null;
  max_items?: number | null;
  token_budget?: number | null;
  dry_run?: boolean | null;
}

export interface OperationTrace {
  trace_id: string;
  correlation_id: string;
  operation: string;
  transport: string;
  backend?: string | null;
  admission_class?: string | null;
  tenant_id?: string | null;
  namespace?: string | null;
  principal?: string | null;
  store_span_id?: string | null;
  planning_trace_id?: string | null;
  started_at_unix_ms: number;
  completed_at_unix_ms: number;
  latency_ms: number;
  status: string;
  status_message?: string | null;
  summary: OperationTraceSummary;
  recall_explanation?: RecallExplanation | null;
}

export interface PortableRecord {
  record: MemoryRecord;
  idempotency_key?: string | null;
}

export interface ExportRequest {
  tenant_id?: string | null;
  namespace?: string | null;
  include_archived: boolean;
}

export interface PortableStorePackage {
  package_version: number;
  exported_at_unix_ms: number;
  manifest: SnapshotManifest;
  records: PortableRecord[];
}

export type ImportMode = "Validate" | "Merge" | "Replace";

export interface ImportRequest {
  package: PortableStorePackage;
  mode: ImportMode;
  dry_run: boolean;
}

export interface ImportFailure {
  record_id?: string | null;
  reason: string;
}

export interface ImportReport {
  mode: ImportMode;
  dry_run: boolean;
  applied: boolean;
  compatible_package: boolean;
  package_version: number;
  validated_records: number;
  imported_records: number;
  skipped_records: number;
  replaced_existing: boolean;
  snapshot_id: string;
  failed_records: ImportFailure[];
}

export interface RuntimeAdmissionStatus {
  queued_total: number;
  inflight_total: number;
  queued_reads: number;
  queued_writes: number;
  queued_admin: number;
  inflight_reads: number;
  inflight_writes: number;
  inflight_admin: number;
  tenant_inflight: Record<string, number>;
  queue_wait_timeout_ms: number;
  average_queue_wait_ms: number;
  max_queue_wait_ms: number;
  oldest_queue_wait_ms: number;
  oldest_read_queue_wait_ms: number;
  oldest_write_queue_wait_ms: number;
  oldest_admin_queue_wait_ms: number;
  fair_queue_policy: string;
}

export interface TraceRegistryStatus {
  stored_traces: number;
  trace_capacity: number;
  evicted_traces: number;
  oldest_started_at_unix_ms?: number | null;
  newest_started_at_unix_ms?: number | null;
}

export interface RuntimeStatus {
  backend: string;
  admission: RuntimeAdmissionStatus;
  traces: TraceRegistryStatus;
}

export interface SnapshotManifest {
  snapshot_id: string;
  created_at_unix_ms: number;
  namespaces: string[];
  record_count: number;
  storage_bytes: number;
  engine: EngineTuningInfo;
}

export interface StoreStatsRequest {
  tenant_id?: string | null;
  namespace?: string | null;
}

export interface NamespaceStats {
  tenant_id: string;
  namespace: string;
  active_records: number;
  archived_records: number;
  deleted_records: number;
  suppressed_records: number;
  pinned_records: number;
}

export interface MaintenanceStats {
  duplicate_candidate_groups: number;
  duplicate_candidate_records: number;
  tombstoned_records: number;
  expired_records: number;
  stale_idempotency_keys: number;
}

export interface EngineTuningInfo {
  recall_scorer_kind: RecallScorerKind;
  recall_scoring_profile: RecallScoringProfile;
  embedding_provider_kind: EmbeddingProviderKind;
  embedding_dimensions: number;
  compaction_summarize_after_record_count: number;
  compaction_cold_archive_after_days: number;
  compaction_cold_archive_importance_threshold_per_mille: number;
}

export interface StoreStatsReport {
  generated_at_unix_ms: number;
  total_records: number;
  storage_bytes: number;
  namespaces: NamespaceStats[];
  maintenance: MaintenanceStats;
  engine: EngineTuningInfo;
}

export interface IntegrityCheckRequest {
  tenant_id?: string | null;
  namespace?: string | null;
}

export interface IntegrityCheckReport {
  generated_at_unix_ms: number;
  healthy: boolean;
  scanned_records: number;
  scanned_idempotency_keys: number;
  stale_idempotency_keys: number;
  missing_idempotency_keys: number;
  duplicate_active_records: number;
}

export interface RepairRequest {
  tenant_id?: string | null;
  namespace?: string | null;
  dry_run: boolean;
  reason: string;
  remove_stale_idempotency_keys: boolean;
  rebuild_missing_idempotency_keys: boolean;
}

export interface RepairReport {
  dry_run: boolean;
  scanned_records: number;
  scanned_idempotency_keys: number;
  removed_stale_idempotency_keys: number;
  rebuilt_missing_idempotency_keys: number;
  healthy_after: boolean;
}

export interface HealthStatus {
  status: string;
}

export interface ReadyStatus {
  ready: boolean;
}

export interface MnemaraHttpClientOptions {
  baseUrl: string;
  token?: string | null;
  headers?: Record<string, string>;
  fetchImpl?: typeof fetch;
}

export class MnemaraHttpError extends Error {
  status: number;
  statusText: string;
  body: unknown;
}

export class MnemaraHttpClient {
  constructor(options: MnemaraHttpClientOptions);
  withToken(token: string): MnemaraHttpClient;
  health(): Promise<HealthStatus>;
  ready(): Promise<ReadyStatus>;
  metrics(): Promise<string>;
  upsert(request: UpsertRequest): Promise<UpsertReceipt>;
  batchUpsert(request: BatchUpsertRequest): Promise<UpsertReceipt[]>;
  recall(query: RecallQuery): Promise<RecallResult>;
  snapshot(): Promise<SnapshotManifest>;
  stats(request?: StoreStatsRequest): Promise<StoreStatsReport>;
  integrityCheck(
    request?: IntegrityCheckRequest,
  ): Promise<IntegrityCheckReport>;
  repair(request: RepairRequest): Promise<RepairReport>;
  compact(request: CompactionRequest): Promise<CompactionReport>;
  delete(request: DeleteRequest): Promise<DeleteReceipt>;
  listTraces(request?: TraceListRequest): Promise<OperationTrace[]>;
  getTrace(traceId: string): Promise<OperationTrace>;
  runtimeStatus(): Promise<RuntimeStatus>;
  export(request?: ExportRequest): Promise<PortableStorePackage>;
  import(request: ImportRequest): Promise<ImportReport>;
}

export const MemoryRecordKind: Readonly<
  Record<Exclude<MemoryRecordKind, never>, MemoryRecordKind>
>;
export const MemoryQualityState: Readonly<
  Record<Exclude<MemoryQualityState, never>, MemoryQualityState>
>;
export const MemoryTrustLevel: Readonly<
  Record<Exclude<MemoryTrustLevel, never>, MemoryTrustLevel>
>;
export const RecallScorerKind: Readonly<
  Record<Exclude<RecallScorerKind, never>, RecallScorerKind>
>;
export const RecallScoringProfile: Readonly<
  Record<Exclude<RecallScoringProfile, never>, RecallScoringProfile>
>;
export const EmbeddingProviderKind: Readonly<
  Record<Exclude<EmbeddingProviderKind, never>, EmbeddingProviderKind>
>;
