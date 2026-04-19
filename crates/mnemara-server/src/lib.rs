#![forbid(unsafe_code)]

mod admission;
mod observability;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use admission::{
    AdmissionClass, AdmissionConfig, AdmissionController, AdmissionError, RuntimeAdmissionStatus,
};
use axum::extract::{Path, Query, Request as AxumRequest};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use mnemara_core::{
    AffectiveAnnotation, AffectiveAnnotationProvenance, ArchiveReceipt, ArchiveRequest,
    ArtifactPointer, BatchUpsertRequest, CompactionReport, CompactionRequest, DeleteReceipt,
    DeleteRequest, EPISODE_SCHEMA_VERSION, EmbeddingProviderKind, EngineTuningInfo, EpisodeContext,
    EpisodeContinuityState, EpisodeSalience, ExportRequest, ImportMode, ImportReport,
    ImportRequest, IntegrityCheckReport, IntegrityCheckRequest, LineageLink, LineageRelationKind,
    MaintenanceStats, MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryStore, MemoryTrustLevel, NamespaceStats, OperationTrace,
    OperationTraceSummary, PortableRecord, PortableStorePackage, RecallCandidateSource,
    RecallExplanation, RecallFilters, RecallHistoricalMode, RecallPlannerStage,
    RecallPlanningProfile, RecallPlanningTrace, RecallPolicyProfile, RecallQuery, RecallResult,
    RecallScorerKind, RecallScoringProfile, RecallTemporalOrder, RecallTraceCandidate,
    RecoverReceipt, RecoverRequest, RepairReport, RepairRequest, SnapshotManifest,
    StoreStatsReport, StoreStatsRequest, SuppressReceipt, SuppressRequest, TraceListRequest,
    TraceOperationKind, TraceStatus, UpsertReceipt, UpsertRequest,
};
use mnemara_protocol::v1::memory_service_server::{MemoryService, MemoryServiceServer};
use mnemara_protocol::v1::{
    AffectiveAnnotation as ProtoAffectiveAnnotation, ArchiveReply,
    ArchiveRequest as ProtoArchiveRequest, ArtifactPointer as ProtoArtifactPointer,
    BatchUpsertMemoryRecordsReply, BatchUpsertMemoryRecordsRequest, CompactReply,
    CompactRequest as ProtoCompactRequest, DeleteReply, DeleteRequest as ProtoDeleteRequest,
    EmbeddingProviderKind as ProtoEmbeddingProviderKind, EngineTuningInfo as ProtoEngineTuningInfo,
    EpisodeContext as ProtoEpisodeContext, EpisodeSalience as ProtoEpisodeSalience,
    ExportReply as ProtoExportReply, ExportRequest as ProtoExportRequest,
    GetTraceRequest as ProtoGetTraceRequest, ImportMode as ProtoImportMode,
    ImportReply as ProtoImportReply, ImportRequest as ProtoImportRequest,
    IntegrityCheckReply as ProtoIntegrityCheckReply,
    IntegrityCheckRequest as ProtoIntegrityCheckRequest, LineageLink as ProtoLineageLink,
    ListTracesReply as ProtoListTracesReply, ListTracesRequest as ProtoListTracesRequest,
    MaintenanceStats as ProtoMaintenanceStats, MemoryRecord as ProtoMemoryRecord,
    MemoryScope as ProtoMemoryScope, NamespaceStats as ProtoNamespaceStats,
    OperationTrace as ProtoOperationTrace, OperationTraceSummary as ProtoOperationTraceSummary,
    PortableRecord as ProtoPortableRecord, RecallExplanation as ProtoRecallExplanation,
    RecallFilters as ProtoRecallFilters, RecallHit as ProtoRecallHit,
    RecallPlanningTrace as ProtoRecallPlanningTrace,
    RecallPolicyProfile as ProtoRecallPolicyProfile, RecallReply,
    RecallRequest as ProtoRecallRequest, RecallScoreBreakdown as ProtoRecallScoreBreakdown,
    RecallScorerKind as ProtoRecallScorerKind, RecallScoringProfile as ProtoRecallScoringProfile,
    RecallTraceCandidate as ProtoRecallTraceCandidate, RecoverReply,
    RecoverRequest as ProtoRecoverRequest, RepairReply as ProtoRepairReply,
    RepairRequest as ProtoRepairRequest, SnapshotReply, SnapshotRequest,
    StoreStatsReply as ProtoStoreStatsReply, StoreStatsRequest as ProtoStoreStatsRequest,
    SuppressReply, SuppressRequest as ProtoSuppressRequest,
    TraceOperationKind as ProtoTraceOperationKind, TraceStatus as ProtoTraceStatus,
    UpsertMemoryRecordReply, UpsertMemoryRecordRequest as ProtoUpsertMemoryRecordRequest,
};
use observability::{TraceRegistry, TraceRegistrySnapshot, now_unix_ms};
use tonic::{Request, Response, Status};

#[derive(Debug, Clone, serde::Serialize)]
struct HealthStatus {
    status: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ReadyStatus {
    status: &'static str,
    record_count: u64,
    namespaces: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ServerRuntimeStatus {
    backend: String,
    admission: RuntimeAdmissionStatus,
    traces: TraceRegistrySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
struct HttpErrorBody {
    error: String,
}

#[derive(Debug, Default)]
struct MethodMetrics {
    started: AtomicU64,
    ok: AtomicU64,
    invalid_argument: AtomicU64,
    conflict: AtomicU64,
    unimplemented: AtomicU64,
    internal: AtomicU64,
}

impl MethodMetrics {
    fn record_started(&self) {
        self.started.fetch_add(1, Ordering::Relaxed);
    }

    fn record_status(&self, status: &Status) {
        match status.code() {
            tonic::Code::Ok => {
                self.ok.fetch_add(1, Ordering::Relaxed);
            }
            tonic::Code::InvalidArgument => {
                self.invalid_argument.fetch_add(1, Ordering::Relaxed);
            }
            tonic::Code::AlreadyExists => {
                self.conflict.fetch_add(1, Ordering::Relaxed);
            }
            tonic::Code::Unimplemented => {
                self.unimplemented.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.internal.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ServerMetrics {
    grpc_upsert: MethodMetrics,
    grpc_batch_upsert: MethodMetrics,
    grpc_recall: MethodMetrics,
    grpc_compact: MethodMetrics,
    grpc_snapshot: MethodMetrics,
    grpc_delete: MethodMetrics,
    grpc_archive: MethodMetrics,
    grpc_suppress: MethodMetrics,
    grpc_recover: MethodMetrics,
    grpc_stats: MethodMetrics,
    grpc_integrity: MethodMetrics,
    grpc_repair: MethodMetrics,
    http_healthz: AtomicU64,
    http_readyz: AtomicU64,
    http_snapshot: AtomicU64,
    http_compact: AtomicU64,
    http_delete: AtomicU64,
    http_archive: AtomicU64,
    http_suppress: AtomicU64,
    http_recover: AtomicU64,
    http_metrics: AtomicU64,
    http_traces: AtomicU64,
    http_runtime: AtomicU64,
    http_export: AtomicU64,
    http_import: AtomicU64,
    http_rejected_body_too_large: AtomicU64,
    admission_rejected: AtomicU64,
    admission_timed_out: AtomicU64,
    trace_records: AtomicU64,
    trace_evictions: AtomicU64,
    trace_reads: AtomicU64,
}

impl ServerMetrics {
    pub fn render(&self) -> String {
        let mut output = String::new();
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_started_total",
            self.grpc_upsert.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_ok_total",
            self.grpc_upsert.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_invalid_argument_total",
            self.grpc_upsert.invalid_argument.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_conflict_total",
            self.grpc_upsert.conflict.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_unimplemented_total",
            self.grpc_upsert.unimplemented.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_upsert_requests_internal_total",
            self.grpc_upsert.internal.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_batch_upsert_requests_started_total",
            self.grpc_batch_upsert.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_batch_upsert_requests_ok_total",
            self.grpc_batch_upsert.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_batch_upsert_requests_invalid_argument_total",
            self.grpc_batch_upsert
                .invalid_argument
                .load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_recall_requests_started_total",
            self.grpc_recall.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_recall_requests_ok_total",
            self.grpc_recall.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_recall_requests_invalid_argument_total",
            self.grpc_recall.invalid_argument.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_compact_requests_started_total",
            self.grpc_compact.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_compact_requests_ok_total",
            self.grpc_compact.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_snapshot_requests_started_total",
            self.grpc_snapshot.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_snapshot_requests_ok_total",
            self.grpc_snapshot.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_delete_requests_started_total",
            self.grpc_delete.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_delete_requests_ok_total",
            self.grpc_delete.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_archive_requests_started_total",
            self.grpc_archive.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_archive_requests_ok_total",
            self.grpc_archive.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_suppress_requests_started_total",
            self.grpc_suppress.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_suppress_requests_ok_total",
            self.grpc_suppress.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_recover_requests_started_total",
            self.grpc_recover.started.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_grpc_recover_requests_ok_total",
            self.grpc_recover.ok.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_healthz_requests_total",
            self.http_healthz.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_readyz_requests_total",
            self.http_readyz.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_snapshot_requests_total",
            self.http_snapshot.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_compact_requests_total",
            self.http_compact.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_delete_requests_total",
            self.http_delete.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_archive_requests_total",
            self.http_archive.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_suppress_requests_total",
            self.http_suppress.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_recover_requests_total",
            self.http_recover.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_metrics_requests_total",
            self.http_metrics.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_traces_requests_total",
            self.http_traces.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_runtime_requests_total",
            self.http_runtime.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_export_requests_total",
            self.http_export.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_import_requests_total",
            self.http_import.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_http_rejected_body_too_large_total",
            self.http_rejected_body_too_large.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_admission_rejected_total",
            self.admission_rejected.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_admission_timed_out_total",
            self.admission_timed_out.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_trace_records_total",
            self.trace_records.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_trace_evictions_total",
            self.trace_evictions.load(Ordering::Relaxed),
        );
        append_counter(
            &mut output,
            "mnemara_trace_reads_total",
            self.trace_reads.load(Ordering::Relaxed),
        );
        output
    }
}

fn append_counter(output: &mut String, name: &str, value: u64) {
    output.push_str("# TYPE ");
    output.push_str(name);
    output.push_str(" counter\n");
    output.push_str(name);
    output.push(' ');
    output.push_str(&value.to_string());
    output.push('\n');
}

#[derive(Debug, Clone)]
pub struct ServerLimits {
    pub max_http_body_bytes: usize,
    pub max_batch_upsert_requests: usize,
    pub max_recall_items: usize,
    pub max_query_text_bytes: usize,
    pub max_record_content_bytes: usize,
    pub max_labels_per_scope: usize,
    pub max_inflight_reads: usize,
    pub max_inflight_writes: usize,
    pub max_inflight_admin: usize,
    pub max_queued_requests: usize,
    pub max_tenant_inflight: usize,
    pub queue_wait_timeout_ms: u64,
    pub trace_retention: usize,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_http_body_bytes: 64 * 1024,
            max_batch_upsert_requests: 128,
            max_recall_items: 64,
            max_query_text_bytes: 4 * 1024,
            max_record_content_bytes: 32 * 1024,
            max_labels_per_scope: 32,
            max_inflight_reads: 64,
            max_inflight_writes: 32,
            max_inflight_admin: 8,
            max_queued_requests: 256,
            max_tenant_inflight: 16,
            queue_wait_timeout_ms: 2_000,
            trace_retention: 256,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub bearer_token: Option<String>,
    pub protect_metrics: bool,
    pub token_policies: Vec<TokenPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPermission {
    Read,
    Write,
    Admin,
    Metrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPolicy {
    pub token: String,
    pub permissions: Vec<AuthPermission>,
}

impl AuthConfig {
    fn has_authentication(&self) -> bool {
        self.bearer_token.is_some() || !self.token_policies.is_empty()
    }

    fn authorize(&self, authorization_header: Option<&str>, permission: AuthPermission) -> bool {
        if !self.has_authentication() {
            return true;
        }

        let Some(token) = bearer_token_from_header(authorization_header) else {
            return false;
        };

        if self.bearer_token.as_deref() == Some(token) {
            return true;
        }

        self.token_policies
            .iter()
            .any(|policy| policy.token == token && policy.permissions.contains(&permission))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeleteHttpRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: String,
    pub hard_delete: bool,
    pub audit_reason: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArchiveHttpRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: String,
    pub dry_run: bool,
    pub audit_reason: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SuppressHttpRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: String,
    pub dry_run: bool,
    pub audit_reason: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RecoverHttpRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: String,
    pub dry_run: bool,
    pub audit_reason: String,
    pub quality_state: MemoryQualityState,
    pub historical_state: Option<MemoryHistoricalState>,
}

pub struct GrpcMemoryService<S> {
    store: Arc<S>,
    runtime: ServerRuntime,
}

impl<S> Clone for GrpcMemoryService<S> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            runtime: self.runtime.clone(),
        }
    }
}

#[derive(Debug)]
struct ServerRuntimeInner {
    backend: Arc<String>,
    limits: Arc<ServerLimits>,
    metrics: Arc<ServerMetrics>,
    auth: Arc<AuthConfig>,
    traces: Arc<TraceRegistry>,
    admission: Arc<AdmissionController>,
}

#[derive(Debug, Clone)]
pub struct ServerRuntime {
    inner: Arc<ServerRuntimeInner>,
}

impl ServerRuntime {
    pub fn new(
        backend: impl Into<String>,
        limits: ServerLimits,
        metrics: Arc<ServerMetrics>,
        auth: AuthConfig,
    ) -> Self {
        let admission = AdmissionController::new(AdmissionConfig {
            max_inflight_reads: limits.max_inflight_reads,
            max_inflight_writes: limits.max_inflight_writes,
            max_inflight_admin: limits.max_inflight_admin,
            max_queued_requests: limits.max_queued_requests,
            max_tenant_inflight: limits.max_tenant_inflight,
            queue_wait_timeout_ms: limits.queue_wait_timeout_ms,
        });
        Self {
            inner: Arc::new(ServerRuntimeInner {
                backend: Arc::new(backend.into()),
                traces: Arc::new(TraceRegistry::new(limits.trace_retention)),
                admission: Arc::new(admission),
                auth: Arc::new(auth),
                metrics,
                limits: Arc::new(limits),
            }),
        }
    }

    fn limits(&self) -> &Arc<ServerLimits> {
        &self.inner.limits
    }

    fn metrics(&self) -> &Arc<ServerMetrics> {
        &self.inner.metrics
    }

    fn backend(&self) -> &Arc<String> {
        &self.inner.backend
    }

    fn auth(&self) -> &Arc<AuthConfig> {
        &self.inner.auth
    }

    fn traces(&self) -> &Arc<TraceRegistry> {
        &self.inner.traces
    }

    fn admission(&self) -> &Arc<AdmissionController> {
        &self.inner.admission
    }
}

impl<S: MemoryStore> GrpcMemoryService<S> {
    pub fn new(store: Arc<S>) -> Self {
        let backend = store.as_ref().backend_kind().to_string();
        Self::with_runtime(
            store,
            ServerRuntime::new(
                backend,
                ServerLimits::default(),
                Arc::new(ServerMetrics::default()),
                AuthConfig::default(),
            ),
        )
    }

    pub fn with_limits(store: Arc<S>, limits: ServerLimits) -> Self {
        let backend = store.as_ref().backend_kind().to_string();
        Self::with_runtime(
            store,
            ServerRuntime::new(
                backend,
                limits,
                Arc::new(ServerMetrics::default()),
                AuthConfig::default(),
            ),
        )
    }

    pub fn with_observability(
        store: Arc<S>,
        limits: ServerLimits,
        metrics: Arc<ServerMetrics>,
    ) -> Self {
        let backend = store.as_ref().backend_kind().to_string();
        Self::with_runtime(
            store,
            ServerRuntime::new(backend, limits, metrics, AuthConfig::default()),
        )
    }

    pub fn with_runtime_config(
        store: Arc<S>,
        limits: ServerLimits,
        metrics: Arc<ServerMetrics>,
        auth: AuthConfig,
    ) -> Self {
        let backend = store.as_ref().backend_kind().to_string();
        Self::with_runtime(store, ServerRuntime::new(backend, limits, metrics, auth))
    }

    pub fn with_runtime(store: Arc<S>, runtime: ServerRuntime) -> Self {
        Self { store, runtime }
    }

    pub fn into_service(self) -> MemoryServiceServer<Self>
    where
        Self: MemoryService,
    {
        MemoryServiceServer::new(self)
    }
}

pub async fn serve<S>(
    addr: SocketAddr,
    store: Arc<S>,
    limits: ServerLimits,
    auth: AuthConfig,
) -> Result<(), tonic::transport::Error>
where
    S: MemoryStore + 'static,
{
    let backend = store.as_ref().backend_kind().to_string();
    serve_with_runtime(
        addr,
        store,
        ServerRuntime::new(backend, limits, Arc::new(ServerMetrics::default()), auth),
    )
    .await
}

pub async fn serve_with_runtime<S>(
    addr: SocketAddr,
    store: Arc<S>,
    runtime: ServerRuntime,
) -> Result<(), tonic::transport::Error>
where
    S: MemoryStore + 'static,
{
    tonic::transport::Server::builder()
        .add_service(GrpcMemoryService::with_runtime(store, runtime).into_service())
        .serve(addr)
        .await
}

pub fn http_app<S>(store: Arc<S>, limits: ServerLimits, auth: AuthConfig) -> Router
where
    S: MemoryStore + 'static,
{
    let backend = store.as_ref().backend_kind().to_string();
    http_app_with_runtime(
        store,
        ServerRuntime::new(backend, limits, Arc::new(ServerMetrics::default()), auth),
    )
}

pub fn http_app_with_metrics<S>(
    store: Arc<S>,
    limits: ServerLimits,
    metrics: Arc<ServerMetrics>,
    auth: AuthConfig,
) -> Router
where
    S: MemoryStore + 'static,
{
    let backend = store.as_ref().backend_kind().to_string();
    http_app_with_runtime(store, ServerRuntime::new(backend, limits, metrics, auth))
}

pub fn http_app_with_runtime<S>(store: Arc<S>, runtime: ServerRuntime) -> Router
where
    S: MemoryStore + 'static,
{
    let limits = Arc::clone(runtime.limits());
    let ready_store = Arc::clone(&store);
    let upsert_store = Arc::clone(&store);
    let batch_upsert_store = Arc::clone(&store);
    let recall_store = Arc::clone(&store);
    let snapshot_store = Arc::clone(&store);
    let stats_store = Arc::clone(&store);
    let integrity_store = Arc::clone(&store);
    let repair_store = Arc::clone(&store);
    let compact_store = Arc::clone(&store);
    let delete_store = Arc::clone(&store);
    let archive_store = Arc::clone(&store);
    let suppress_store = Arc::clone(&store);
    let recover_store = Arc::clone(&store);
    let export_store = Arc::clone(&store);
    let import_store = Arc::clone(&store);
    let ready_runtime = runtime.clone();
    let upsert_runtime = runtime.clone();
    let batch_upsert_runtime = runtime.clone();
    let recall_runtime = runtime.clone();
    let snapshot_runtime = runtime.clone();
    let stats_runtime = runtime.clone();
    let integrity_runtime = runtime.clone();
    let repair_runtime = runtime.clone();
    let compact_runtime = runtime.clone();
    let delete_runtime = runtime.clone();
    let archive_runtime = runtime.clone();
    let suppress_runtime = runtime.clone();
    let recover_runtime = runtime.clone();
    let traces_runtime = runtime.clone();
    let trace_runtime = runtime.clone();
    let export_runtime = runtime.clone();
    let import_runtime = runtime.clone();
    let runtime_status_runtime = runtime.clone();
    let metrics_metrics = Arc::clone(runtime.metrics());
    let middleware_limits = Arc::clone(&limits);
    let middleware_metrics = Arc::clone(runtime.metrics());
    let middleware_auth = Arc::clone(runtime.auth());

    let health_metrics = Arc::clone(runtime.metrics());

    Router::new()
        .route(
            "/healthz",
            get(move || {
                let metrics = Arc::clone(&health_metrics);
                async move { healthz(metrics).await }
            }),
        )
        .route(
            "/readyz",
            get(move || {
                let store = Arc::clone(&ready_store);
                let runtime = ready_runtime.clone();
                async move { readyz(store, runtime).await }
            }),
        )
        .route(
            "/memory/upsert",
            post(
                move |headers: HeaderMap, Json(request): Json<UpsertRequest>| {
                    let store = Arc::clone(&upsert_store);
                    let runtime = upsert_runtime.clone();
                    async move { upsert_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/memory/batch-upsert",
            post(
                move |headers: HeaderMap, Json(request): Json<BatchUpsertRequest>| {
                    let store = Arc::clone(&batch_upsert_store);
                    let runtime = batch_upsert_runtime.clone();
                    async move { batch_upsert_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/memory/recall",
            post(
                move |headers: HeaderMap, Json(request): Json<RecallQuery>| {
                    let store = Arc::clone(&recall_store);
                    let runtime = recall_runtime.clone();
                    async move { recall_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/snapshot",
            get(move |headers: HeaderMap| {
                let store = Arc::clone(&snapshot_store);
                let runtime = snapshot_runtime.clone();
                async move { snapshot_http(store, headers, runtime).await }
            }),
        )
        .route(
            "/admin/stats",
            get(
                move |headers: HeaderMap, Query(request): Query<StoreStatsRequest>| {
                    let store = Arc::clone(&stats_store);
                    let runtime = stats_runtime.clone();
                    async move { stats_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/integrity",
            get(
                move |headers: HeaderMap, Query(request): Query<IntegrityCheckRequest>| {
                    let store = Arc::clone(&integrity_store);
                    let runtime = integrity_runtime.clone();
                    async move { integrity_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/repair",
            post(
                move |headers: HeaderMap, Json(request): Json<RepairRequest>| {
                    let store = Arc::clone(&repair_store);
                    let runtime = repair_runtime.clone();
                    async move { repair_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/compact",
            post(
                move |headers: HeaderMap, Json(request): Json<CompactionRequest>| {
                    let store = Arc::clone(&compact_store);
                    let runtime = compact_runtime.clone();
                    async move { compact_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/delete",
            post(
                move |headers: HeaderMap, Json(request): Json<DeleteHttpRequest>| {
                    let store = Arc::clone(&delete_store);
                    let runtime = delete_runtime.clone();
                    async move { delete_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/archive",
            post(
                move |headers: HeaderMap, Json(request): Json<ArchiveHttpRequest>| {
                    let store = Arc::clone(&archive_store);
                    let runtime = archive_runtime.clone();
                    async move { archive_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/suppress",
            post(
                move |headers: HeaderMap, Json(request): Json<SuppressHttpRequest>| {
                    let store = Arc::clone(&suppress_store);
                    let runtime = suppress_runtime.clone();
                    async move { suppress_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/recover",
            post(
                move |headers: HeaderMap, Json(request): Json<RecoverHttpRequest>| {
                    let store = Arc::clone(&recover_store);
                    let runtime = recover_runtime.clone();
                    async move { recover_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/traces",
            get(move |Query(request): Query<TraceListRequest>| {
                let runtime = traces_runtime.clone();
                async move { traces_http(request, runtime).await }
            }),
        )
        .route(
            "/admin/traces/{trace_id}",
            get(move |Path(trace_id): Path<String>| {
                let runtime = trace_runtime.clone();
                async move { trace_http(trace_id, runtime).await }
            }),
        )
        .route(
            "/admin/runtime",
            get(move || {
                let runtime = runtime_status_runtime.clone();
                async move { runtime_status_http(runtime).await }
            }),
        )
        .route(
            "/admin/export",
            post(
                move |headers: HeaderMap, Json(request): Json<ExportRequest>| {
                    let store = Arc::clone(&export_store);
                    let runtime = export_runtime.clone();
                    async move { export_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/admin/import",
            post(
                move |headers: HeaderMap, Json(request): Json<ImportRequest>| {
                    let store = Arc::clone(&import_store);
                    let runtime = import_runtime.clone();
                    async move { import_http(store, request, headers, runtime).await }
                },
            ),
        )
        .route(
            "/metrics",
            get(move || {
                let metrics = Arc::clone(&metrics_metrics);
                async move { metrics_http(metrics).await }
            }),
        )
        .layer(middleware::from_fn(move |request, next| {
            let limits = Arc::clone(&middleware_limits);
            let metrics = Arc::clone(&middleware_metrics);
            let auth = Arc::clone(&middleware_auth);
            async move { enforce_http_guardrails(request, next, limits, metrics, auth).await }
        }))
}

pub async fn serve_http<S>(
    addr: SocketAddr,
    store: Arc<S>,
    limits: ServerLimits,
    auth: AuthConfig,
) -> std::io::Result<()>
where
    S: MemoryStore + 'static,
{
    let backend = store.as_ref().backend_kind().to_string();
    serve_http_with_runtime(
        addr,
        store,
        ServerRuntime::new(backend, limits, Arc::new(ServerMetrics::default()), auth),
    )
    .await
}

pub async fn serve_http_with_runtime<S>(
    addr: SocketAddr,
    store: Arc<S>,
    runtime: ServerRuntime,
) -> std::io::Result<()>
where
    S: MemoryStore + 'static,
{
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, http_app_with_runtime(store, runtime)).await
}

async fn healthz(metrics: Arc<ServerMetrics>) -> Json<HealthStatus> {
    metrics.http_healthz.fetch_add(1, Ordering::Relaxed);
    Json(HealthStatus { status: "ok" })
}

async fn readyz<S>(
    store: Arc<S>,
    runtime: ServerRuntime,
) -> Result<Json<ReadyStatus>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_readyz
        .fetch_add(1, Ordering::Relaxed);
    let manifest = store.snapshot().await.map_err(map_http_store_error)?;
    Ok(Json(ReadyStatus {
        status: "ready",
        record_count: manifest.record_count,
        namespaces: manifest.namespaces,
    }))
}

async fn upsert_http<S>(
    store: Arc<S>,
    request: UpsertRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<UpsertReceipt>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Write, Some(&request.record.scope.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipt = store
        .upsert(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Upsert,
        "http",
        Some(request.record.scope.tenant_id),
        Some(request.record.scope.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            record_id: Some(request.record.id),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipt))
}

async fn batch_upsert_http<S>(
    store: Arc<S>,
    request: BatchUpsertRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<Vec<UpsertReceipt>>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let tenant_id = request
        .requests
        .first()
        .map(|item| item.record.scope.tenant_id.clone());
    let namespace = request
        .requests
        .first()
        .map(|item| item.record.scope.namespace.clone());
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Write, tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipts = store
        .batch_upsert(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::BatchUpsert,
        "http",
        tenant_id,
        namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            request_count: Some(request.requests.len() as u32),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipts))
}

async fn recall_http<S>(
    store: Arc<S>,
    request: RecallQuery,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<RecallResult>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Read, Some(&request.scope.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let mut result = store
        .recall(request.clone())
        .await
        .map_err(map_http_store_error)?;
    attach_correlation_id(&mut result, &correlation_id);
    record_trace(
        &runtime,
        TraceOperationKind::Recall,
        "http",
        Some(request.scope.tenant_id),
        Some(request.scope.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            query_text: Some(request.query_text),
            max_items: Some(request.max_items as u32),
            token_budget: request.token_budget.map(|value| value as u32),
            ..OperationTraceSummary::default()
        },
        result.explanation.clone(),
        correlation_id,
    );
    Ok(Json(result))
}

async fn snapshot_http<S>(
    store: Arc<S>,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<SnapshotManifest>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_snapshot
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, None)
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let manifest = store.snapshot().await.map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Snapshot,
        "http",
        None,
        None,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary::default(),
        None,
        correlation_id,
    );
    Ok(Json(manifest))
}

async fn stats_http<S>(
    store: Arc<S>,
    request: StoreStatsRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<StoreStatsReport>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, request.tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let report = store
        .stats(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Stats,
        "http",
        request.tenant_id,
        request.namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary::default(),
        None,
        correlation_id,
    );
    Ok(Json(report))
}

async fn integrity_http<S>(
    store: Arc<S>,
    request: IntegrityCheckRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<IntegrityCheckReport>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, request.tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let report = store
        .integrity_check(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::IntegrityCheck,
        "http",
        request.tenant_id,
        request.namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary::default(),
        None,
        correlation_id,
    );
    Ok(Json(report))
}

async fn repair_http<S>(
    store: Arc<S>,
    request: RepairRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<RepairReport>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, request.tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let report = store
        .repair(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Repair,
        "http",
        request.tenant_id,
        request.namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            dry_run: Some(request.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(report))
}

async fn compact_http<S>(
    store: Arc<S>,
    request: CompactionRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<CompactionReport>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_compact
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, Some(&request.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let report = store
        .compact(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Compact,
        "http",
        Some(request.tenant_id),
        request.namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            dry_run: Some(request.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(report))
}

async fn delete_http<S>(
    store: Arc<S>,
    request: DeleteHttpRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<DeleteReceipt>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_delete
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, Some(&request.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipt = store
        .delete(DeleteRequest {
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            record_id: request.record_id,
            hard_delete: request.hard_delete,
            audit_reason: request.audit_reason,
        })
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Delete,
        "http",
        Some(request.tenant_id),
        Some(request.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            record_id: Some(receipt.record_id.clone()),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipt))
}

async fn archive_http<S>(
    store: Arc<S>,
    request: ArchiveHttpRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<ArchiveReceipt>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_archive
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, Some(&request.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipt = store
        .archive(ArchiveRequest {
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            record_id: request.record_id,
            dry_run: request.dry_run,
            audit_reason: request.audit_reason,
        })
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Archive,
        "http",
        Some(request.tenant_id),
        Some(request.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            record_id: Some(receipt.record_id.clone()),
            dry_run: Some(receipt.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipt))
}

async fn suppress_http<S>(
    store: Arc<S>,
    request: SuppressHttpRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<SuppressReceipt>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_suppress
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, Some(&request.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipt = store
        .suppress(SuppressRequest {
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            record_id: request.record_id,
            dry_run: request.dry_run,
            audit_reason: request.audit_reason,
        })
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Suppress,
        "http",
        Some(request.tenant_id),
        Some(request.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            record_id: Some(receipt.record_id.clone()),
            dry_run: Some(receipt.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipt))
}

async fn recover_http<S>(
    store: Arc<S>,
    request: RecoverHttpRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<RecoverReceipt>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_recover
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, Some(&request.tenant_id))
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let receipt = store
        .recover(RecoverRequest {
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            record_id: request.record_id,
            dry_run: request.dry_run,
            audit_reason: request.audit_reason,
            quality_state: request.quality_state,
            historical_state: request.historical_state,
        })
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Recover,
        "http",
        Some(request.tenant_id),
        Some(request.namespace),
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary {
            record_id: Some(receipt.record_id.clone()),
            dry_run: Some(receipt.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(receipt))
}

async fn traces_http(
    request: TraceListRequest,
    runtime: ServerRuntime,
) -> Result<Json<Vec<OperationTrace>>, (StatusCode, Json<HttpErrorBody>)> {
    runtime
        .metrics()
        .http_traces
        .fetch_add(1, Ordering::Relaxed);
    runtime
        .metrics()
        .trace_reads
        .fetch_add(1, Ordering::Relaxed);
    Ok(Json(runtime.traces().list(&request)))
}

async fn trace_http(
    trace_id: String,
    runtime: ServerRuntime,
) -> Result<Json<OperationTrace>, (StatusCode, Json<HttpErrorBody>)> {
    runtime
        .metrics()
        .http_traces
        .fetch_add(1, Ordering::Relaxed);
    runtime
        .metrics()
        .trace_reads
        .fetch_add(1, Ordering::Relaxed);
    runtime.traces().get(&trace_id).map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(HttpErrorBody {
                error: format!("trace {trace_id} not found"),
            }),
        )
    })
}

async fn runtime_status_http(runtime: ServerRuntime) -> Json<ServerRuntimeStatus> {
    runtime
        .metrics()
        .http_runtime
        .fetch_add(1, Ordering::Relaxed);
    Json(ServerRuntimeStatus {
        backend: runtime.backend().as_ref().clone(),
        admission: runtime.admission().snapshot(),
        traces: runtime.traces().snapshot(),
    })
}

async fn export_http<S>(
    store: Arc<S>,
    request: ExportRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<PortableStorePackage>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_export
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, request.tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let package = store
        .export(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Export,
        "http",
        request.tenant_id,
        request.namespace,
        http_principal(&headers),
        started_at_unix_ms,
        TraceStatus::Ok,
        None,
        OperationTraceSummary::default(),
        None,
        correlation_id,
    );
    Ok(Json(package))
}

async fn import_http<S>(
    store: Arc<S>,
    request: ImportRequest,
    headers: HeaderMap,
    runtime: ServerRuntime,
) -> Result<Json<ImportReport>, (StatusCode, Json<HttpErrorBody>)>
where
    S: MemoryStore + 'static,
{
    runtime
        .metrics()
        .http_import
        .fetch_add(1, Ordering::Relaxed);
    let started_at_unix_ms = now_unix_ms();
    let correlation_id = runtime.traces().next_id("corr");
    let tenant_id = request
        .package
        .records
        .first()
        .map(|entry| entry.record.scope.tenant_id.clone());
    let namespace = request
        .package
        .records
        .first()
        .map(|entry| entry.record.scope.namespace.clone());
    let _permit = runtime
        .admission()
        .acquire(AdmissionClass::Admin, tenant_id.as_deref())
        .await
        .map_err(|error| map_http_admission_error(runtime.metrics().as_ref(), error))?;
    let report = store
        .import(request.clone())
        .await
        .map_err(map_http_store_error)?;
    record_trace(
        &runtime,
        TraceOperationKind::Import,
        "http",
        tenant_id,
        namespace,
        http_principal(&headers),
        started_at_unix_ms,
        if report.failed_records.is_empty() {
            TraceStatus::Ok
        } else {
            TraceStatus::Error
        },
        (!report.failed_records.is_empty())
            .then(|| format!("{} import validation failures", report.failed_records.len())),
        OperationTraceSummary {
            request_count: Some(request.package.records.len() as u32),
            dry_run: Some(request.dry_run),
            ..OperationTraceSummary::default()
        },
        None,
        correlation_id,
    );
    Ok(Json(report))
}

async fn metrics_http(metrics: Arc<ServerMetrics>) -> impl IntoResponse {
    metrics.http_metrics.fetch_add(1, Ordering::Relaxed);
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics.render(),
    )
}

async fn enforce_http_guardrails(
    request: AxumRequest,
    next: Next,
    limits: Arc<ServerLimits>,
    metrics: Arc<ServerMetrics>,
    auth: Arc<AuthConfig>,
) -> Result<axum::response::Response, StatusCode> {
    let path = request.uri().path().to_string();
    if let Some(content_length) = request.headers().get(axum::http::header::CONTENT_LENGTH)
        && let Ok(content_length) = content_length.to_str()
        && let Ok(content_length) = content_length.parse::<usize>()
        && content_length > limits.max_http_body_bytes
    {
        metrics
            .http_rejected_body_too_large
            .fetch_add(1, Ordering::Relaxed);
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    validate_http_auth(&request, &path, auth.as_ref())?;
    Ok(next.run(request).await)
}

fn http_principal(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|_| "bearer".to_string())
}

fn grpc_principal<T>(request: &Request<T>) -> Option<String> {
    request
        .metadata()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(|_| "bearer".to_string())
}

fn map_http_admission_error(
    metrics: &ServerMetrics,
    error: AdmissionError,
) -> (StatusCode, Json<HttpErrorBody>) {
    match error {
        AdmissionError::QueueFull | AdmissionError::TenantBusy => {
            metrics.admission_rejected.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(HttpErrorBody {
                    error: "request admission rejected".to_string(),
                }),
            )
        }
        AdmissionError::TimedOut => {
            metrics.admission_timed_out.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(HttpErrorBody {
                    error: "request admission timed out".to_string(),
                }),
            )
        }
    }
}

fn map_grpc_admission_error(metrics: &ServerMetrics, error: AdmissionError) -> Status {
    match error {
        AdmissionError::QueueFull | AdmissionError::TenantBusy => {
            metrics.admission_rejected.fetch_add(1, Ordering::Relaxed);
            Status::resource_exhausted("request admission rejected")
        }
        AdmissionError::TimedOut => {
            metrics.admission_timed_out.fetch_add(1, Ordering::Relaxed);
            Status::resource_exhausted("request admission timed out")
        }
    }
}

fn attach_correlation_id(result: &mut RecallResult, correlation_id: &str) {
    if let Some(explanation) = result.explanation.as_mut() {
        if let Some(existing) = explanation.trace_id.replace(correlation_id.to_string()) {
            explanation
                .policy_notes
                .push(format!("planning_trace_id={existing}"));
        }
        explanation
            .policy_notes
            .push(format!("correlation_id={correlation_id}"));
    }
    for hit in &mut result.hits {
        if let Some(explanation) = hit.explanation.as_mut() {
            if let Some(existing) = explanation.trace_id.replace(correlation_id.to_string()) {
                explanation
                    .policy_notes
                    .push(format!("planning_trace_id={existing}"));
            }
            explanation
                .policy_notes
                .push(format!("correlation_id={correlation_id}"));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_trace(
    runtime: &ServerRuntime,
    operation: TraceOperationKind,
    transport: &str,
    tenant_id: Option<String>,
    namespace: Option<String>,
    principal: Option<String>,
    started_at_unix_ms: u64,
    status: TraceStatus,
    status_message: Option<String>,
    summary: OperationTraceSummary,
    recall_explanation: Option<RecallExplanation>,
    correlation_id: String,
) {
    let completed_at_unix_ms = now_unix_ms();
    let admission_class = admission_class_for_operation(operation.clone()).to_string();
    let planning_trace_id = recall_explanation.as_ref().and_then(|value| {
        value
            .planning_trace
            .as_ref()
            .map(|trace| trace.trace_id.clone())
    });
    let store_span_id = runtime.traces().next_id("store-span");
    let evicted = runtime.traces().record(OperationTrace {
        trace_id: runtime.traces().next_id("trace"),
        correlation_id,
        operation,
        transport: transport.to_string(),
        backend: Some(runtime.backend().as_ref().clone()),
        admission_class: Some(admission_class),
        tenant_id,
        namespace,
        principal,
        store_span_id: Some(store_span_id),
        planning_trace_id,
        started_at_unix_ms,
        completed_at_unix_ms,
        latency_ms: completed_at_unix_ms.saturating_sub(started_at_unix_ms),
        status,
        status_message,
        summary,
        recall_explanation,
    });
    runtime
        .metrics()
        .trace_records
        .fetch_add(1, Ordering::Relaxed);
    if evicted {
        runtime
            .metrics()
            .trace_evictions
            .fetch_add(1, Ordering::Relaxed);
    }
}

fn admission_class_for_operation(operation: TraceOperationKind) -> &'static str {
    match operation {
        TraceOperationKind::Recall | TraceOperationKind::Snapshot | TraceOperationKind::Stats => {
            "read"
        }
        TraceOperationKind::Upsert
        | TraceOperationKind::BatchUpsert
        | TraceOperationKind::Compact
        | TraceOperationKind::Delete
        | TraceOperationKind::Archive
        | TraceOperationKind::Suppress
        | TraceOperationKind::Recover => "write",
        TraceOperationKind::IntegrityCheck
        | TraceOperationKind::Repair
        | TraceOperationKind::Export
        | TraceOperationKind::Import => "admin",
    }
}

fn validate_http_auth(
    request: &AxumRequest,
    path: &str,
    auth: &AuthConfig,
) -> Result<(), StatusCode> {
    if path == "/healthz" || path == "/readyz" || (path == "/metrics" && !auth.protect_metrics) {
        return Ok(());
    }
    let provided = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let permission = match path {
        "/metrics" => AuthPermission::Metrics,
        "/memory/recall" => AuthPermission::Read,
        "/memory/upsert" | "/memory/batch-upsert" => AuthPermission::Write,
        "/admin/stats" | "/admin/integrity" | "/admin/repair" | "/admin/snapshot"
        | "/admin/compact" | "/admin/delete" | "/admin/archive" | "/admin/suppress"
        | "/admin/recover" | "/admin/runtime" | "/admin/export" | "/admin/import" => {
            AuthPermission::Admin
        }
        _ => AuthPermission::Admin,
    };
    if path.starts_with("/admin/traces") {
        return auth
            .authorize(provided, AuthPermission::Admin)
            .then_some(())
            .ok_or(StatusCode::UNAUTHORIZED);
    }
    if auth.authorize(provided, permission) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn validate_grpc_auth<T>(
    request: &Request<T>,
    auth: &AuthConfig,
    permission: AuthPermission,
) -> Result<(), Status> {
    let provided = request
        .metadata()
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    if auth.authorize(provided, permission) {
        Ok(())
    } else {
        Err(Status::unauthenticated("missing or invalid bearer token"))
    }
}

fn bearer_token_from_header(header: Option<&str>) -> Option<&str> {
    header.and_then(|value| value.strip_prefix("Bearer "))
}

fn invalid_argument(message: impl Into<String>) -> Status {
    Status::invalid_argument(message.into())
}

fn internal_status(message: impl Into<String>) -> Status {
    Status::internal(message.into())
}

fn trust_level_from_proto(value: &str) -> Result<MemoryTrustLevel, Status> {
    match value {
        "" | "derived" => Ok(MemoryTrustLevel::Derived),
        "untrusted" => Ok(MemoryTrustLevel::Untrusted),
        "observed" => Ok(MemoryTrustLevel::Observed),
        "verified" => Ok(MemoryTrustLevel::Verified),
        "pinned" => Ok(MemoryTrustLevel::Pinned),
        other => Err(invalid_argument(format!("unknown trust level: {other}"))),
    }
}

fn trust_level_to_proto(value: MemoryTrustLevel) -> String {
    match value {
        MemoryTrustLevel::Untrusted => "untrusted",
        MemoryTrustLevel::Observed => "observed",
        MemoryTrustLevel::Derived => "derived",
        MemoryTrustLevel::Verified => "verified",
        MemoryTrustLevel::Pinned => "pinned",
    }
    .to_string()
}

fn record_kind_from_proto(value: &str) -> Result<MemoryRecordKind, Status> {
    match value {
        "episodic" => Ok(MemoryRecordKind::Episodic),
        "summary" => Ok(MemoryRecordKind::Summary),
        "fact" => Ok(MemoryRecordKind::Fact),
        "preference" => Ok(MemoryRecordKind::Preference),
        "task" => Ok(MemoryRecordKind::Task),
        "artifact" => Ok(MemoryRecordKind::Artifact),
        "hypothesis" => Ok(MemoryRecordKind::Hypothesis),
        other => Err(invalid_argument(format!("unknown record kind: {other}"))),
    }
}

fn record_kind_to_proto(value: MemoryRecordKind) -> String {
    match value {
        MemoryRecordKind::Episodic => "episodic",
        MemoryRecordKind::Summary => "summary",
        MemoryRecordKind::Fact => "fact",
        MemoryRecordKind::Preference => "preference",
        MemoryRecordKind::Task => "task",
        MemoryRecordKind::Artifact => "artifact",
        MemoryRecordKind::Hypothesis => "hypothesis",
    }
    .to_string()
}

fn quality_state_from_proto(value: &str) -> Result<MemoryQualityState, Status> {
    match value {
        "draft" => Ok(MemoryQualityState::Draft),
        "active" => Ok(MemoryQualityState::Active),
        "verified" => Ok(MemoryQualityState::Verified),
        "archived" => Ok(MemoryQualityState::Archived),
        "suppressed" => Ok(MemoryQualityState::Suppressed),
        "deleted" => Ok(MemoryQualityState::Deleted),
        other => Err(invalid_argument(format!("unknown quality state: {other}"))),
    }
}

fn quality_state_to_proto(value: MemoryQualityState) -> String {
    match value {
        MemoryQualityState::Draft => "draft",
        MemoryQualityState::Active => "active",
        MemoryQualityState::Verified => "verified",
        MemoryQualityState::Archived => "archived",
        MemoryQualityState::Suppressed => "suppressed",
        MemoryQualityState::Deleted => "deleted",
    }
    .to_string()
}

fn scope_from_proto(scope: ProtoMemoryScope) -> Result<MemoryScope, Status> {
    Ok(MemoryScope {
        tenant_id: scope.tenant_id,
        namespace: scope.namespace,
        actor_id: scope.actor_id,
        conversation_id: scope.conversation_id,
        session_id: scope.session_id,
        source: scope.source,
        labels: scope.labels,
        trust_level: trust_level_from_proto(&scope.trust_level)?,
    })
}

fn scope_to_proto(scope: MemoryScope) -> ProtoMemoryScope {
    ProtoMemoryScope {
        tenant_id: scope.tenant_id,
        namespace: scope.namespace,
        actor_id: scope.actor_id,
        conversation_id: scope.conversation_id,
        session_id: scope.session_id,
        source: scope.source,
        labels: scope.labels,
        trust_level: trust_level_to_proto(scope.trust_level),
    }
}

fn artifact_from_proto(value: ProtoArtifactPointer) -> ArtifactPointer {
    ArtifactPointer {
        uri: value.uri,
        media_type: value.media_type,
        checksum: value.checksum,
    }
}

fn artifact_to_proto(value: ArtifactPointer) -> ProtoArtifactPointer {
    ProtoArtifactPointer {
        uri: value.uri,
        media_type: value.media_type,
        checksum: value.checksum,
    }
}

fn continuity_state_from_proto(value: &str) -> Result<EpisodeContinuityState, Status> {
    match value {
        "" | "open" => Ok(EpisodeContinuityState::Open),
        "resolved" => Ok(EpisodeContinuityState::Resolved),
        "superseded" => Ok(EpisodeContinuityState::Superseded),
        "abandoned" => Ok(EpisodeContinuityState::Abandoned),
        other => Err(invalid_argument(format!(
            "unknown continuity state: {other}"
        ))),
    }
}

fn continuity_state_to_proto(value: EpisodeContinuityState) -> String {
    match value {
        EpisodeContinuityState::Open => "open",
        EpisodeContinuityState::Resolved => "resolved",
        EpisodeContinuityState::Superseded => "superseded",
        EpisodeContinuityState::Abandoned => "abandoned",
    }
    .to_string()
}

fn affective_provenance_from_proto(value: &str) -> Result<AffectiveAnnotationProvenance, Status> {
    match value {
        "" | "authored" => Ok(AffectiveAnnotationProvenance::Authored),
        "imported" => Ok(AffectiveAnnotationProvenance::Imported),
        "derived" => Ok(AffectiveAnnotationProvenance::Derived),
        other => Err(invalid_argument(format!(
            "unknown affective provenance: {other}"
        ))),
    }
}

fn affective_provenance_to_proto(value: AffectiveAnnotationProvenance) -> String {
    match value {
        AffectiveAnnotationProvenance::Authored => "authored",
        AffectiveAnnotationProvenance::Imported => "imported",
        AffectiveAnnotationProvenance::Derived => "derived",
    }
    .to_string()
}

fn temporal_order_from_proto(value: &str) -> Result<RecallTemporalOrder, Status> {
    match value {
        "" | "relevance" => Ok(RecallTemporalOrder::Relevance),
        "chronological_asc" => Ok(RecallTemporalOrder::ChronologicalAsc),
        "chronological_desc" => Ok(RecallTemporalOrder::ChronologicalDesc),
        other => Err(invalid_argument(format!("unknown temporal order: {other}"))),
    }
}

fn planning_profile_from_proto(value: &str) -> Result<RecallPlanningProfile, Status> {
    match value {
        "" | "fast_path" => Ok(RecallPlanningProfile::FastPath),
        "continuity_aware" => Ok(RecallPlanningProfile::ContinuityAware),
        other => Err(invalid_argument(format!(
            "unknown planning profile: {other}"
        ))),
    }
}

fn planning_profile_to_proto(value: RecallPlanningProfile) -> String {
    match value {
        RecallPlanningProfile::FastPath => "fast_path",
        RecallPlanningProfile::ContinuityAware => "continuity_aware",
    }
    .to_string()
}

fn planner_stage_to_proto(value: RecallPlannerStage) -> String {
    match value {
        RecallPlannerStage::CandidateGeneration => "candidate_generation",
        RecallPlannerStage::GraphExpansion => "graph_expansion",
        RecallPlannerStage::Selection => "selection",
    }
    .to_string()
}

fn candidate_source_to_proto(value: RecallCandidateSource) -> String {
    match value {
        RecallCandidateSource::Lexical => "lexical",
        RecallCandidateSource::Semantic => "semantic",
        RecallCandidateSource::Metadata => "metadata",
        RecallCandidateSource::Episode => "episode",
        RecallCandidateSource::Graph => "graph",
        RecallCandidateSource::Temporal => "temporal",
        RecallCandidateSource::Provenance => "provenance",
    }
    .to_string()
}

fn affective_annotation_from_proto(
    value: ProtoAffectiveAnnotation,
) -> Result<AffectiveAnnotation, Status> {
    Ok(AffectiveAnnotation {
        tone: value.tone,
        sentiment: value.sentiment,
        urgency: value.urgency,
        confidence: value.confidence,
        tension: value.tension,
        provenance: affective_provenance_from_proto(&value.provenance)?,
    })
}

fn affective_annotation_to_proto(value: AffectiveAnnotation) -> ProtoAffectiveAnnotation {
    ProtoAffectiveAnnotation {
        tone: value.tone,
        sentiment: value.sentiment,
        urgency: value.urgency,
        confidence: value.confidence,
        tension: value.tension,
        provenance: affective_provenance_to_proto(value.provenance),
    }
}

fn historical_state_from_proto(value: &str) -> Result<MemoryHistoricalState, Status> {
    match value {
        "" | "current" => Ok(MemoryHistoricalState::Current),
        "historical" => Ok(MemoryHistoricalState::Historical),
        "superseded" => Ok(MemoryHistoricalState::Superseded),
        other => Err(invalid_argument(format!(
            "unknown historical state: {other}"
        ))),
    }
}

fn historical_state_to_proto(value: MemoryHistoricalState) -> String {
    match value {
        MemoryHistoricalState::Current => "current",
        MemoryHistoricalState::Historical => "historical",
        MemoryHistoricalState::Superseded => "superseded",
    }
    .to_string()
}

fn lineage_relation_from_proto(value: &str) -> Result<LineageRelationKind, Status> {
    match value {
        "" | "derived_from" => Ok(LineageRelationKind::DerivedFrom),
        "consolidated_from" => Ok(LineageRelationKind::ConsolidatedFrom),
        "supersedes" => Ok(LineageRelationKind::Supersedes),
        "superseded_by" => Ok(LineageRelationKind::SupersededBy),
        "conflicts_with" => Ok(LineageRelationKind::ConflictsWith),
        other => Err(invalid_argument(format!(
            "unknown lineage relation: {other}"
        ))),
    }
}

fn lineage_relation_to_proto(value: LineageRelationKind) -> String {
    match value {
        LineageRelationKind::DerivedFrom => "derived_from",
        LineageRelationKind::ConsolidatedFrom => "consolidated_from",
        LineageRelationKind::Supersedes => "supersedes",
        LineageRelationKind::SupersededBy => "superseded_by",
        LineageRelationKind::ConflictsWith => "conflicts_with",
    }
    .to_string()
}

fn lineage_link_from_proto(value: ProtoLineageLink) -> Result<LineageLink, Status> {
    Ok(LineageLink {
        record_id: value.record_id,
        relation: lineage_relation_from_proto(&value.relation)?,
        confidence: value.confidence,
    })
}

fn lineage_link_to_proto(value: LineageLink) -> ProtoLineageLink {
    ProtoLineageLink {
        record_id: value.record_id,
        relation: lineage_relation_to_proto(value.relation),
        confidence: value.confidence,
    }
}

fn historical_mode_from_proto(value: &str) -> Result<RecallHistoricalMode, Status> {
    match value {
        "" | "current_only" => Ok(RecallHistoricalMode::CurrentOnly),
        "include_historical" => Ok(RecallHistoricalMode::IncludeHistorical),
        "historical_only" => Ok(RecallHistoricalMode::HistoricalOnly),
        other => Err(invalid_argument(format!(
            "unknown historical mode: {other}"
        ))),
    }
}

fn episode_context_from_proto(value: ProtoEpisodeContext) -> Result<EpisodeContext, Status> {
    Ok(EpisodeContext {
        schema_version: if value.schema_version == 0 {
            EPISODE_SCHEMA_VERSION
        } else {
            value.schema_version
        },
        episode_id: value.episode_id,
        summary: value.summary,
        continuity_state: continuity_state_from_proto(&value.continuity_state)?,
        actor_ids: value.actor_ids,
        goal: value.goal,
        outcome: value.outcome,
        started_at_unix_ms: value.started_at_unix_ms,
        ended_at_unix_ms: value.ended_at_unix_ms,
        last_active_unix_ms: value.last_active_unix_ms,
        recurrence_key: value.recurrence_key,
        recurrence_interval_ms: value.recurrence_interval_ms,
        boundary_label: value.boundary_label,
        previous_record_id: value.previous_record_id,
        next_record_id: value.next_record_id,
        causal_record_ids: value.causal_record_ids,
        related_record_ids: value.related_record_ids,
        linked_artifact_uris: value.linked_artifact_uris,
        salience: value
            .salience
            .map_or_else(EpisodeSalience::default, |salience| EpisodeSalience {
                reuse_count: salience.reuse_count,
                novelty_score: salience.novelty_score,
                goal_relevance: salience.goal_relevance,
                unresolved_weight: salience.unresolved_weight,
            }),
        affective: value
            .affective
            .map(affective_annotation_from_proto)
            .transpose()?,
    })
}

fn episode_context_to_proto(value: EpisodeContext) -> ProtoEpisodeContext {
    ProtoEpisodeContext {
        schema_version: value.schema_version,
        episode_id: value.episode_id,
        summary: value.summary,
        continuity_state: continuity_state_to_proto(value.continuity_state),
        actor_ids: value.actor_ids,
        goal: value.goal,
        outcome: value.outcome,
        started_at_unix_ms: value.started_at_unix_ms,
        ended_at_unix_ms: value.ended_at_unix_ms,
        last_active_unix_ms: value.last_active_unix_ms,
        recurrence_key: value.recurrence_key,
        recurrence_interval_ms: value.recurrence_interval_ms,
        boundary_label: value.boundary_label,
        previous_record_id: value.previous_record_id,
        next_record_id: value.next_record_id,
        causal_record_ids: value.causal_record_ids,
        related_record_ids: value.related_record_ids,
        linked_artifact_uris: value.linked_artifact_uris,
        salience: Some(ProtoEpisodeSalience {
            reuse_count: value.salience.reuse_count,
            novelty_score: value.salience.novelty_score,
            goal_relevance: value.salience.goal_relevance,
            unresolved_weight: value.salience.unresolved_weight,
        }),
        affective: value.affective.map(affective_annotation_to_proto),
    }
}

fn record_from_proto(record: ProtoMemoryRecord) -> Result<MemoryRecord, Status> {
    Ok(MemoryRecord {
        id: record.id,
        scope: scope_from_proto(
            record
                .scope
                .ok_or_else(|| invalid_argument("memory record scope is required"))?,
        )?,
        kind: record_kind_from_proto(&record.kind)?,
        content: record.content,
        summary: record.summary,
        source_id: record.source_id,
        metadata: record.metadata.into_iter().collect(),
        quality_state: quality_state_from_proto(&record.quality_state)?,
        created_at_unix_ms: record.created_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
        expires_at_unix_ms: record.expires_at_unix_ms,
        importance_score: record.importance_score,
        artifact: record.artifact.map(artifact_from_proto),
        episode: record.episode.map(episode_context_from_proto).transpose()?,
        historical_state: historical_state_from_proto(
            record.historical_state.as_deref().unwrap_or(""),
        )?,
        lineage: record
            .lineage
            .into_iter()
            .map(lineage_link_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn record_to_proto(record: MemoryRecord) -> ProtoMemoryRecord {
    ProtoMemoryRecord {
        id: record.id,
        scope: Some(scope_to_proto(record.scope)),
        kind: record_kind_to_proto(record.kind),
        content: record.content,
        summary: record.summary,
        metadata: record.metadata.into_iter().collect(),
        quality_state: quality_state_to_proto(record.quality_state),
        created_at_unix_ms: record.created_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
        expires_at_unix_ms: record.expires_at_unix_ms,
        importance_score: record.importance_score,
        source_id: record.source_id,
        artifact: record.artifact.map(artifact_to_proto),
        episode: record.episode.map(episode_context_to_proto),
        historical_state: Some(historical_state_to_proto(record.historical_state)),
        lineage: record
            .lineage
            .into_iter()
            .map(lineage_link_to_proto)
            .collect(),
    }
}

fn recall_explanation_to_proto(value: RecallExplanation) -> ProtoRecallExplanation {
    ProtoRecallExplanation {
        selected_channels: value.selected_channels,
        policy_notes: value.policy_notes,
        trace_id: value.trace_id,
        planning_trace: value.planning_trace.map(recall_planning_trace_to_proto),
        scorer_kind: value
            .scorer_kind
            .map(recall_scorer_kind_to_proto)
            .unwrap_or(ProtoRecallScorerKind::Unspecified as i32),
        scoring_profile: value
            .scoring_profile
            .map(recall_scoring_profile_to_proto)
            .unwrap_or(ProtoRecallScoringProfile::Unspecified as i32),
        planning_profile: value.planning_profile.map(planning_profile_to_proto),
        policy_profile: value
            .policy_profile
            .map(recall_policy_profile_to_proto)
            .unwrap_or(ProtoRecallPolicyProfile::Unspecified as i32),
    }
}

fn recall_scorer_kind_to_proto(value: RecallScorerKind) -> i32 {
    match value {
        RecallScorerKind::Profile => ProtoRecallScorerKind::Profile as i32,
        RecallScorerKind::Curated => ProtoRecallScorerKind::Curated as i32,
    }
}

fn recall_scoring_profile_to_proto(value: RecallScoringProfile) -> i32 {
    match value {
        RecallScoringProfile::Balanced => ProtoRecallScoringProfile::Balanced as i32,
        RecallScoringProfile::LexicalFirst => ProtoRecallScoringProfile::LexicalFirst as i32,
        RecallScoringProfile::ImportanceFirst => ProtoRecallScoringProfile::ImportanceFirst as i32,
    }
}

fn recall_policy_profile_to_proto(value: RecallPolicyProfile) -> i32 {
    match value {
        RecallPolicyProfile::General => ProtoRecallPolicyProfile::General as i32,
        RecallPolicyProfile::Support => ProtoRecallPolicyProfile::Support as i32,
        RecallPolicyProfile::Research => ProtoRecallPolicyProfile::Research as i32,
        RecallPolicyProfile::Assistant => ProtoRecallPolicyProfile::Assistant as i32,
        RecallPolicyProfile::AutonomousAgent => ProtoRecallPolicyProfile::AutonomousAgent as i32,
    }
}

fn embedding_provider_kind_to_proto(value: EmbeddingProviderKind) -> i32 {
    match value {
        EmbeddingProviderKind::Disabled => ProtoEmbeddingProviderKind::Disabled as i32,
        EmbeddingProviderKind::DeterministicLocal => {
            ProtoEmbeddingProviderKind::DeterministicLocal as i32
        }
    }
}

fn engine_tuning_info_to_proto(value: EngineTuningInfo) -> ProtoEngineTuningInfo {
    ProtoEngineTuningInfo {
        recall_scorer_kind: recall_scorer_kind_to_proto(value.recall_scorer_kind),
        recall_scoring_profile: recall_scoring_profile_to_proto(value.recall_scoring_profile),
        embedding_provider_kind: embedding_provider_kind_to_proto(value.embedding_provider_kind),
        embedding_dimensions: value.embedding_dimensions as u64,
        graph_expansion_max_hops: u32::from(value.graph_expansion_max_hops),
        compaction_summarize_after_record_count: value.compaction_summarize_after_record_count
            as u64,
        compaction_cold_archive_after_days: value.compaction_cold_archive_after_days,
        compaction_cold_archive_importance_threshold_per_mille: u32::from(
            value.compaction_cold_archive_importance_threshold_per_mille,
        ),
        recall_planning_profile: Some(planning_profile_to_proto(value.recall_planning_profile)),
        recall_policy_profile: recall_policy_profile_to_proto(value.recall_policy_profile),
    }
}

fn engine_tuning_info_from_proto(value: ProtoEngineTuningInfo) -> Result<EngineTuningInfo, Status> {
    Ok(EngineTuningInfo {
        recall_scorer_kind: match ProtoRecallScorerKind::try_from(value.recall_scorer_kind)
            .unwrap_or(ProtoRecallScorerKind::Unspecified)
        {
            ProtoRecallScorerKind::Profile => RecallScorerKind::Profile,
            ProtoRecallScorerKind::Curated => RecallScorerKind::Curated,
            ProtoRecallScorerKind::Unspecified => {
                return Err(invalid_argument("engine recall scorer kind is required"));
            }
        },
        recall_scoring_profile: match ProtoRecallScoringProfile::try_from(
            value.recall_scoring_profile,
        )
        .unwrap_or(ProtoRecallScoringProfile::Unspecified)
        {
            ProtoRecallScoringProfile::Balanced => RecallScoringProfile::Balanced,
            ProtoRecallScoringProfile::LexicalFirst => RecallScoringProfile::LexicalFirst,
            ProtoRecallScoringProfile::ImportanceFirst => RecallScoringProfile::ImportanceFirst,
            ProtoRecallScoringProfile::Unspecified => {
                return Err(invalid_argument(
                    "engine recall scoring profile is required",
                ));
            }
        },
        recall_planning_profile: planning_profile_from_proto(
            value.recall_planning_profile.as_deref().unwrap_or(""),
        )?,
        recall_policy_profile: match ProtoRecallPolicyProfile::try_from(value.recall_policy_profile)
            .unwrap_or(ProtoRecallPolicyProfile::Unspecified)
        {
            ProtoRecallPolicyProfile::General => RecallPolicyProfile::General,
            ProtoRecallPolicyProfile::Support => RecallPolicyProfile::Support,
            ProtoRecallPolicyProfile::Research => RecallPolicyProfile::Research,
            ProtoRecallPolicyProfile::Assistant => RecallPolicyProfile::Assistant,
            ProtoRecallPolicyProfile::AutonomousAgent => RecallPolicyProfile::AutonomousAgent,
            ProtoRecallPolicyProfile::Unspecified => RecallPolicyProfile::General,
        },
        embedding_provider_kind: match ProtoEmbeddingProviderKind::try_from(
            value.embedding_provider_kind,
        )
        .unwrap_or(ProtoEmbeddingProviderKind::Unspecified)
        {
            ProtoEmbeddingProviderKind::Disabled => EmbeddingProviderKind::Disabled,
            ProtoEmbeddingProviderKind::DeterministicLocal => {
                EmbeddingProviderKind::DeterministicLocal
            }
            ProtoEmbeddingProviderKind::Unspecified => {
                return Err(invalid_argument(
                    "engine embedding provider kind is required",
                ));
            }
        },
        embedding_dimensions: value.embedding_dimensions as usize,
        graph_expansion_max_hops: value.graph_expansion_max_hops as u8,
        compaction_summarize_after_record_count: value.compaction_summarize_after_record_count
            as usize,
        compaction_cold_archive_after_days: value.compaction_cold_archive_after_days,
        compaction_cold_archive_importance_threshold_per_mille: value
            .compaction_cold_archive_importance_threshold_per_mille
            as u16,
    })
}

fn recall_planning_trace_to_proto(value: RecallPlanningTrace) -> ProtoRecallPlanningTrace {
    ProtoRecallPlanningTrace {
        trace_id: value.trace_id,
        token_budget_applied: value.token_budget_applied,
        candidates: value
            .candidates
            .into_iter()
            .map(recall_trace_candidate_to_proto)
            .collect(),
    }
}

fn recall_trace_candidate_to_proto(value: RecallTraceCandidate) -> ProtoRecallTraceCandidate {
    ProtoRecallTraceCandidate {
        record_id: value.record_id,
        kind: record_kind_to_proto(value.kind),
        selected: value.selected,
        matched_terms: value.matched_terms,
        decision_reason: value.decision_reason,
        breakdown: Some(recall_score_breakdown_to_proto(value.breakdown)),
        selection_rank: value.selection_rank,
        selected_channels: value.selected_channels,
        filter_reasons: value.filter_reasons,
        candidate_sources: value
            .candidate_sources
            .into_iter()
            .map(candidate_source_to_proto)
            .collect(),
        planner_stage: Some(planner_stage_to_proto(value.planner_stage)),
    }
}

fn recall_score_breakdown_to_proto(
    value: mnemara_core::RecallScoreBreakdown,
) -> ProtoRecallScoreBreakdown {
    ProtoRecallScoreBreakdown {
        lexical: value.lexical,
        semantic: value.semantic,
        graph: value.graph,
        temporal: value.temporal,
        policy: value.policy,
        total: value.total,
        metadata: value.metadata,
        curation: value.curation,
        episodic: value.episodic,
        salience: value.salience,
    }
}

fn namespace_stats_to_proto(value: NamespaceStats) -> ProtoNamespaceStats {
    ProtoNamespaceStats {
        tenant_id: value.tenant_id,
        namespace: value.namespace,
        active_records: value.active_records,
        archived_records: value.archived_records,
        deleted_records: value.deleted_records,
        suppressed_records: value.suppressed_records,
        pinned_records: value.pinned_records,
    }
}

fn maintenance_stats_to_proto(value: MaintenanceStats) -> ProtoMaintenanceStats {
    ProtoMaintenanceStats {
        duplicate_candidate_groups: value.duplicate_candidate_groups,
        duplicate_candidate_records: value.duplicate_candidate_records,
        tombstoned_records: value.tombstoned_records,
        expired_records: value.expired_records,
        stale_idempotency_keys: value.stale_idempotency_keys,
        historical_records: value.historical_records,
        superseded_records: value.superseded_records,
        lineage_links: value.lineage_links,
    }
}

fn portable_record_to_proto(value: PortableRecord) -> ProtoPortableRecord {
    ProtoPortableRecord {
        record: Some(record_to_proto(value.record)),
        idempotency_key: value.idempotency_key,
    }
}

fn snapshot_manifest_to_proto(value: SnapshotManifest) -> SnapshotReply {
    SnapshotReply {
        snapshot_id: value.snapshot_id,
        created_at_unix_ms: value.created_at_unix_ms,
        namespaces: value.namespaces,
        record_count: value.record_count,
        storage_bytes: value.storage_bytes,
        engine: Some(engine_tuning_info_to_proto(value.engine)),
    }
}

fn portable_package_to_proto(value: PortableStorePackage) -> ProtoExportReply {
    ProtoExportReply {
        package_version: value.package_version,
        exported_at_unix_ms: value.exported_at_unix_ms,
        manifest: Some(snapshot_manifest_to_proto(value.manifest)),
        records: value
            .records
            .into_iter()
            .map(portable_record_to_proto)
            .collect(),
    }
}

fn import_mode_from_proto(value: i32) -> Result<ImportMode, Status> {
    match ProtoImportMode::try_from(value).unwrap_or(ProtoImportMode::Unspecified) {
        ProtoImportMode::Validate => Ok(ImportMode::Validate),
        ProtoImportMode::Merge => Ok(ImportMode::Merge),
        ProtoImportMode::Replace => Ok(ImportMode::Replace),
        ProtoImportMode::Unspecified => Err(invalid_argument("import mode is required")),
    }
}

fn portable_record_from_proto(value: ProtoPortableRecord) -> Result<PortableRecord, Status> {
    Ok(PortableRecord {
        record: record_from_proto(
            value
                .record
                .ok_or_else(|| invalid_argument("portable record payload is required"))?,
        )?,
        idempotency_key: value.idempotency_key,
    })
}

fn snapshot_manifest_from_proto(value: SnapshotReply) -> Result<SnapshotManifest, Status> {
    Ok(SnapshotManifest {
        snapshot_id: value.snapshot_id,
        created_at_unix_ms: value.created_at_unix_ms,
        namespaces: value.namespaces,
        record_count: value.record_count,
        storage_bytes: value.storage_bytes,
        engine: value
            .engine
            .map(engine_tuning_info_from_proto)
            .transpose()?
            .ok_or_else(|| invalid_argument("snapshot engine tuning info is required"))?,
    })
}

fn portable_package_from_proto(value: ProtoExportReply) -> Result<PortableStorePackage, Status> {
    Ok(PortableStorePackage {
        package_version: value.package_version,
        exported_at_unix_ms: value.exported_at_unix_ms,
        manifest: snapshot_manifest_from_proto(
            value
                .manifest
                .ok_or_else(|| invalid_argument("portable export manifest is required"))?,
        )?,
        records: value
            .records
            .into_iter()
            .map(portable_record_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn trace_operation_kind_to_proto(value: TraceOperationKind) -> i32 {
    match value {
        TraceOperationKind::Upsert => ProtoTraceOperationKind::Upsert as i32,
        TraceOperationKind::BatchUpsert => ProtoTraceOperationKind::BatchUpsert as i32,
        TraceOperationKind::Recall => ProtoTraceOperationKind::Recall as i32,
        TraceOperationKind::Snapshot => ProtoTraceOperationKind::Snapshot as i32,
        TraceOperationKind::Stats => ProtoTraceOperationKind::Stats as i32,
        TraceOperationKind::IntegrityCheck => ProtoTraceOperationKind::IntegrityCheck as i32,
        TraceOperationKind::Repair => ProtoTraceOperationKind::Repair as i32,
        TraceOperationKind::Compact => ProtoTraceOperationKind::Compact as i32,
        TraceOperationKind::Delete => ProtoTraceOperationKind::Delete as i32,
        TraceOperationKind::Archive => ProtoTraceOperationKind::Archive as i32,
        TraceOperationKind::Suppress => ProtoTraceOperationKind::Suppress as i32,
        TraceOperationKind::Recover => ProtoTraceOperationKind::Recover as i32,
        TraceOperationKind::Export => ProtoTraceOperationKind::Export as i32,
        TraceOperationKind::Import => ProtoTraceOperationKind::Import as i32,
    }
}

fn trace_operation_kind_from_proto(value: ProtoTraceOperationKind) -> Option<TraceOperationKind> {
    match value {
        ProtoTraceOperationKind::Upsert => Some(TraceOperationKind::Upsert),
        ProtoTraceOperationKind::BatchUpsert => Some(TraceOperationKind::BatchUpsert),
        ProtoTraceOperationKind::Recall => Some(TraceOperationKind::Recall),
        ProtoTraceOperationKind::Snapshot => Some(TraceOperationKind::Snapshot),
        ProtoTraceOperationKind::Stats => Some(TraceOperationKind::Stats),
        ProtoTraceOperationKind::IntegrityCheck => Some(TraceOperationKind::IntegrityCheck),
        ProtoTraceOperationKind::Repair => Some(TraceOperationKind::Repair),
        ProtoTraceOperationKind::Compact => Some(TraceOperationKind::Compact),
        ProtoTraceOperationKind::Delete => Some(TraceOperationKind::Delete),
        ProtoTraceOperationKind::Archive => Some(TraceOperationKind::Archive),
        ProtoTraceOperationKind::Suppress => Some(TraceOperationKind::Suppress),
        ProtoTraceOperationKind::Recover => Some(TraceOperationKind::Recover),
        ProtoTraceOperationKind::Export => Some(TraceOperationKind::Export),
        ProtoTraceOperationKind::Import => Some(TraceOperationKind::Import),
        ProtoTraceOperationKind::Unspecified => None,
    }
}

fn trace_status_to_proto(value: TraceStatus) -> i32 {
    match value {
        TraceStatus::Ok => ProtoTraceStatus::Ok as i32,
        TraceStatus::Rejected => ProtoTraceStatus::Rejected as i32,
        TraceStatus::Error => ProtoTraceStatus::Error as i32,
    }
}

fn trace_status_from_proto(value: ProtoTraceStatus) -> Option<TraceStatus> {
    match value {
        ProtoTraceStatus::Ok => Some(TraceStatus::Ok),
        ProtoTraceStatus::Rejected => Some(TraceStatus::Rejected),
        ProtoTraceStatus::Error => Some(TraceStatus::Error),
        ProtoTraceStatus::Unspecified => None,
    }
}

fn operation_trace_to_proto(value: OperationTrace) -> ProtoOperationTrace {
    ProtoOperationTrace {
        trace_id: value.trace_id,
        correlation_id: value.correlation_id,
        operation: trace_operation_kind_to_proto(value.operation),
        transport: value.transport,
        backend: value.backend,
        admission_class: value.admission_class,
        tenant_id: value.tenant_id,
        namespace: value.namespace,
        principal: value.principal,
        store_span_id: value.store_span_id,
        planning_trace_id: value.planning_trace_id,
        started_at_unix_ms: value.started_at_unix_ms,
        completed_at_unix_ms: value.completed_at_unix_ms,
        latency_ms: value.latency_ms,
        status: trace_status_to_proto(value.status),
        status_message: value.status_message,
        summary: Some(ProtoOperationTraceSummary {
            record_id: value.summary.record_id,
            request_count: value.summary.request_count,
            query_text: value.summary.query_text,
            max_items: value.summary.max_items,
            token_budget: value.summary.token_budget,
            dry_run: value.summary.dry_run,
        }),
        recall_explanation: value.recall_explanation.map(recall_explanation_to_proto),
    }
}

fn kinds_from_proto(values: Vec<String>) -> Result<Vec<MemoryRecordKind>, Status> {
    values
        .into_iter()
        .map(|value| record_kind_from_proto(&value))
        .collect()
}

fn trust_levels_from_proto(values: Vec<String>) -> Result<Vec<MemoryTrustLevel>, Status> {
    values
        .into_iter()
        .map(|value| trust_level_from_proto(&value))
        .collect()
}

fn quality_states_from_proto(values: Vec<String>) -> Result<Vec<MemoryQualityState>, Status> {
    values
        .into_iter()
        .map(|value| quality_state_from_proto(&value))
        .collect()
}

fn recall_filters_from_proto(filters: Option<ProtoRecallFilters>) -> Result<RecallFilters, Status> {
    let Some(filters) = filters else {
        return Ok(RecallFilters::default());
    };

    Ok(RecallFilters {
        kinds: kinds_from_proto(filters.kinds)?,
        required_labels: filters.required_labels,
        source: filters.source,
        from_unix_ms: filters.from_unix_ms,
        to_unix_ms: filters.to_unix_ms,
        min_importance_score: filters.min_importance_score,
        trust_levels: trust_levels_from_proto(filters.trust_levels)?,
        states: quality_states_from_proto(filters.states)?,
        include_archived: filters.include_archived,
        episode_id: filters.episode_id,
        continuity_states: filters
            .continuity_states
            .into_iter()
            .map(|value| continuity_state_from_proto(&value))
            .collect::<Result<Vec<_>, _>>()?,
        unresolved_only: filters.unresolved_only,
        temporal_order: temporal_order_from_proto(filters.temporal_order.as_deref().unwrap_or(""))?,
        historical_mode: historical_mode_from_proto(
            filters.historical_mode.as_deref().unwrap_or(""),
        )?,
        lineage_record_id: filters.lineage_record_id,
    })
}

fn validate_scope_limits(scope: &MemoryScope, limits: &ServerLimits) -> Result<(), Status> {
    if scope.labels.len() > limits.max_labels_per_scope {
        return Err(invalid_argument(format!(
            "scope labels exceed configured max of {}",
            limits.max_labels_per_scope
        )));
    }
    Ok(())
}

fn validate_record_limits(record: &MemoryRecord, limits: &ServerLimits) -> Result<(), Status> {
    validate_scope_limits(&record.scope, limits)?;
    if record.content.len() > limits.max_record_content_bytes {
        return Err(invalid_argument(format!(
            "record content exceeds configured max of {} bytes",
            limits.max_record_content_bytes
        )));
    }
    if record.summary.as_deref().map(str::len).unwrap_or(0) > limits.max_query_text_bytes {
        return Err(invalid_argument(format!(
            "record summary exceeds configured max of {} bytes",
            limits.max_query_text_bytes
        )));
    }
    Ok(())
}

fn validate_recall_limits(query: &RecallQuery, limits: &ServerLimits) -> Result<(), Status> {
    validate_scope_limits(&query.scope, limits)?;
    if query.max_items == 0 {
        return Err(invalid_argument("recall max_items must be greater than 0"));
    }
    if query.max_items > limits.max_recall_items {
        return Err(invalid_argument(format!(
            "recall max_items exceeds configured max of {}",
            limits.max_recall_items
        )));
    }
    if query.query_text.len() > limits.max_query_text_bytes {
        return Err(invalid_argument(format!(
            "query text exceeds configured max of {} bytes",
            limits.max_query_text_bytes
        )));
    }
    Ok(())
}

fn map_store_error(error: mnemara_core::Error) -> Status {
    match error {
        mnemara_core::Error::InvalidConfig(message)
        | mnemara_core::Error::InvalidRequest(message) => invalid_argument(message),
        mnemara_core::Error::Conflict(message) => Status::already_exists(message),
        mnemara_core::Error::Unsupported(message) => Status::unimplemented(message),
        mnemara_core::Error::Backend(message) => internal_status(message),
    }
}

fn map_http_store_error(error: mnemara_core::Error) -> (StatusCode, Json<HttpErrorBody>) {
    let status = match error {
        mnemara_core::Error::InvalidConfig(_) | mnemara_core::Error::InvalidRequest(_) => {
            StatusCode::BAD_REQUEST
        }
        mnemara_core::Error::Conflict(_) => StatusCode::CONFLICT,
        mnemara_core::Error::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
        mnemara_core::Error::Backend(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(HttpErrorBody {
            error: error.to_string(),
        }),
    )
}

fn record_grpc_result<T>(
    metrics: &MethodMetrics,
    result: Result<Response<T>, Status>,
) -> Result<Response<T>, Status> {
    match result {
        Ok(response) => {
            metrics.record_status(&Status::ok(""));
            Ok(response)
        }
        Err(status) => {
            metrics.record_status(&status);
            Err(status)
        }
    }
}

#[tonic::async_trait]
impl<S> MemoryService for GrpcMemoryService<S>
where
    S: MemoryStore + 'static,
{
    async fn upsert_memory_record(
        &self,
        request: Request<ProtoUpsertMemoryRecordRequest>,
    ) -> Result<Response<UpsertMemoryRecordReply>, Status> {
        self.runtime.metrics().grpc_upsert.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Write,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let record = record_from_proto(
                request
                    .record
                    .ok_or_else(|| invalid_argument("memory record is required"))?,
            )?;
            let tenant_id = record.scope.tenant_id.clone();
            let namespace = record.scope.namespace.clone();
            validate_record_limits(&record, self.runtime.limits().as_ref())?;
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Write, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let receipt = self
                .store
                .upsert(mnemara_core::UpsertRequest {
                    record,
                    idempotency_key: request.idempotency_key,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Upsert,
                "grpc",
                Some(tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    record_id: Some(receipt.record_id.clone()),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(UpsertMemoryRecordReply {
                record_id: receipt.record_id,
                deduplicated: receipt.deduplicated,
                summary_refreshed: receipt.summary_refreshed,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_upsert, result)
    }

    async fn batch_upsert_memory_records(
        &self,
        request: Request<BatchUpsertMemoryRecordsRequest>,
    ) -> Result<Response<BatchUpsertMemoryRecordsReply>, Status> {
        self.runtime.metrics().grpc_batch_upsert.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Write,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            if request.requests.len() > self.runtime.limits().max_batch_upsert_requests {
                return Err(invalid_argument(format!(
                    "batch upsert request count exceeds configured max of {}",
                    self.runtime.limits().max_batch_upsert_requests
                )));
            }
            let requests = request
                .requests
                .into_iter()
                .map(|request| {
                    let record = record_from_proto(
                        request
                            .record
                            .ok_or_else(|| invalid_argument("memory record is required"))?,
                    )?;
                    validate_record_limits(&record, self.runtime.limits().as_ref())?;
                    Ok(mnemara_core::UpsertRequest {
                        record,
                        idempotency_key: request.idempotency_key,
                    })
                })
                .collect::<Result<Vec<_>, Status>>()?;
            let tenant_id = requests
                .first()
                .map(|item| item.record.scope.tenant_id.clone());
            let namespace = requests
                .first()
                .map(|item| item.record.scope.namespace.clone());
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Write, tenant_id.as_deref())
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;

            let receipts: Vec<UpsertMemoryRecordReply> = self
                .store
                .batch_upsert(BatchUpsertRequest { requests })
                .await
                .map_err(map_store_error)?
                .into_iter()
                .map(|receipt| UpsertMemoryRecordReply {
                    record_id: receipt.record_id,
                    deduplicated: receipt.deduplicated,
                    summary_refreshed: receipt.summary_refreshed,
                })
                .collect();
            record_trace(
                &self.runtime,
                TraceOperationKind::BatchUpsert,
                "grpc",
                tenant_id,
                namespace,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    request_count: Some(receipts.len() as u32),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(BatchUpsertMemoryRecordsReply { receipts }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_batch_upsert, result)
    }

    async fn recall(
        &self,
        request: Request<ProtoRecallRequest>,
    ) -> Result<Response<RecallReply>, Status> {
        self.runtime.metrics().grpc_recall.record_started();
        validate_grpc_auth(&request, self.runtime.auth().as_ref(), AuthPermission::Read)?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let query = RecallQuery {
                scope: scope_from_proto(
                    request
                        .scope
                        .ok_or_else(|| invalid_argument("recall scope is required"))?,
                )?,
                query_text: request.query_text,
                max_items: request.max_items as usize,
                token_budget: request.token_budget.map(|value| value as usize),
                filters: recall_filters_from_proto(request.filters)?,
                include_explanation: request.include_explanation,
            };
            validate_recall_limits(&query, self.runtime.limits().as_ref())?;
            let tenant_id = query.scope.tenant_id.clone();
            let namespace = query.scope.namespace.clone();
            let query_text = query.query_text.clone();
            let max_items = query.max_items as u32;
            let token_budget = query.token_budget.map(|value| value as u32);
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Read, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let mut result = self.store.recall(query).await.map_err(map_store_error)?;
            attach_correlation_id(&mut result, &correlation_id);
            let recall_explanation = result.explanation.clone();

            let hits = result
                .hits
                .into_iter()
                .map(|hit| ProtoRecallHit {
                    record: Some(record_to_proto(hit.record)),
                    breakdown: Some(recall_score_breakdown_to_proto(hit.breakdown)),
                    selected_channels: hit
                        .explanation
                        .as_ref()
                        .map(|value| value.selected_channels.clone())
                        .unwrap_or_default(),
                    policy_notes: hit
                        .explanation
                        .as_ref()
                        .map(|value| value.policy_notes.clone())
                        .unwrap_or_default(),
                    explanation: hit.explanation.map(recall_explanation_to_proto),
                })
                .collect();
            record_trace(
                &self.runtime,
                TraceOperationKind::Recall,
                "grpc",
                Some(tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    query_text: Some(query_text),
                    max_items: Some(max_items),
                    token_budget,
                    ..OperationTraceSummary::default()
                },
                recall_explanation,
                correlation_id,
            );

            Ok(Response::new(RecallReply {
                hits,
                total_candidates_examined: result.total_candidates_examined as u32,
                explanation: result.explanation.map(recall_explanation_to_proto),
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_recall, result)
    }

    async fn compact(
        &self,
        request: Request<ProtoCompactRequest>,
    ) -> Result<Response<CompactReply>, Status> {
        self.runtime.metrics().grpc_compact.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let report = self
                .store
                .compact(CompactionRequest {
                    tenant_id,
                    namespace: namespace.clone(),
                    dry_run: request.dry_run,
                    reason: request.reason,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Compact,
                "grpc",
                Some(request.tenant_id),
                namespace,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    dry_run: Some(request.dry_run),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(CompactReply {
                deduplicated_records: report.deduplicated_records,
                archived_records: report.archived_records,
                summarized_clusters: report.summarized_clusters,
                pruned_graph_edges: report.pruned_graph_edges,
                superseded_records: report.superseded_records,
                lineage_links_created: report.lineage_links_created,
                dry_run: report.dry_run,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_compact, result)
    }

    async fn snapshot(
        &self,
        request: Request<SnapshotRequest>,
    ) -> Result<Response<SnapshotReply>, Status> {
        self.runtime.metrics().grpc_snapshot.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, None)
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let manifest = self.store.snapshot().await.map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Snapshot,
                "grpc",
                None,
                None,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary::default(),
                None,
                correlation_id,
            );
            Ok(Response::new(SnapshotReply {
                snapshot_id: manifest.snapshot_id,
                created_at_unix_ms: manifest.created_at_unix_ms,
                namespaces: manifest.namespaces,
                record_count: manifest.record_count,
                storage_bytes: manifest.storage_bytes,
                engine: Some(engine_tuning_info_to_proto(manifest.engine)),
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_snapshot, result)
    }

    async fn delete(
        &self,
        request: Request<ProtoDeleteRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        self.runtime.metrics().grpc_delete.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let receipt = self
                .store
                .delete(DeleteRequest {
                    tenant_id,
                    namespace: namespace.clone(),
                    record_id: request.record_id,
                    hard_delete: request.hard_delete,
                    audit_reason: request.audit_reason,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Delete,
                "grpc",
                Some(request.tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    record_id: Some(receipt.record_id.clone()),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(DeleteReply {
                record_id: receipt.record_id,
                tombstoned: receipt.tombstoned,
                hard_deleted: receipt.hard_deleted,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_delete, result)
    }

    async fn archive(
        &self,
        request: Request<ProtoArchiveRequest>,
    ) -> Result<Response<ArchiveReply>, Status> {
        self.runtime.metrics().grpc_archive.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let receipt = self
                .store
                .archive(ArchiveRequest {
                    tenant_id,
                    namespace: namespace.clone(),
                    record_id: request.record_id,
                    dry_run: request.dry_run,
                    audit_reason: request.audit_reason,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Archive,
                "grpc",
                Some(request.tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    record_id: Some(receipt.record_id.clone()),
                    dry_run: Some(receipt.dry_run),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(ArchiveReply {
                record_id: receipt.record_id,
                previous_quality_state: quality_state_to_proto(receipt.previous_quality_state),
                previous_historical_state: historical_state_to_proto(
                    receipt.previous_historical_state,
                ),
                quality_state: quality_state_to_proto(receipt.quality_state),
                historical_state: historical_state_to_proto(receipt.historical_state),
                changed: receipt.changed,
                dry_run: receipt.dry_run,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_archive, result)
    }

    async fn suppress(
        &self,
        request: Request<ProtoSuppressRequest>,
    ) -> Result<Response<SuppressReply>, Status> {
        self.runtime.metrics().grpc_suppress.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let receipt = self
                .store
                .suppress(SuppressRequest {
                    tenant_id,
                    namespace: namespace.clone(),
                    record_id: request.record_id,
                    dry_run: request.dry_run,
                    audit_reason: request.audit_reason,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Suppress,
                "grpc",
                Some(request.tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    record_id: Some(receipt.record_id.clone()),
                    dry_run: Some(receipt.dry_run),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(SuppressReply {
                record_id: receipt.record_id,
                previous_quality_state: quality_state_to_proto(receipt.previous_quality_state),
                previous_historical_state: historical_state_to_proto(
                    receipt.previous_historical_state,
                ),
                quality_state: quality_state_to_proto(receipt.quality_state),
                historical_state: historical_state_to_proto(receipt.historical_state),
                changed: receipt.changed,
                dry_run: receipt.dry_run,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_suppress, result)
    }

    async fn recover(
        &self,
        request: Request<ProtoRecoverRequest>,
    ) -> Result<Response<RecoverReply>, Status> {
        self.runtime.metrics().grpc_recover.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, Some(&tenant_id))
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let receipt = self
                .store
                .recover(RecoverRequest {
                    tenant_id,
                    namespace: namespace.clone(),
                    record_id: request.record_id,
                    dry_run: request.dry_run,
                    audit_reason: request.audit_reason,
                    quality_state: quality_state_from_proto(&request.quality_state)?,
                    historical_state: request
                        .historical_state
                        .as_deref()
                        .map(historical_state_from_proto)
                        .transpose()?,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Recover,
                "grpc",
                Some(request.tenant_id),
                Some(namespace),
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    record_id: Some(receipt.record_id.clone()),
                    dry_run: Some(receipt.dry_run),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(RecoverReply {
                record_id: receipt.record_id,
                previous_quality_state: quality_state_to_proto(receipt.previous_quality_state),
                previous_historical_state: historical_state_to_proto(
                    receipt.previous_historical_state,
                ),
                quality_state: quality_state_to_proto(receipt.quality_state),
                historical_state: historical_state_to_proto(receipt.historical_state),
                changed: receipt.changed,
                dry_run: receipt.dry_run,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_recover, result)
    }

    async fn stats(
        &self,
        request: Request<ProtoStoreStatsRequest>,
    ) -> Result<Response<ProtoStoreStatsReply>, Status> {
        self.runtime.metrics().grpc_stats.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, tenant_id.as_deref())
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let report = self
                .store
                .stats(StoreStatsRequest {
                    tenant_id: tenant_id.clone(),
                    namespace: namespace.clone(),
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Stats,
                "grpc",
                tenant_id,
                namespace,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary::default(),
                None,
                correlation_id,
            );

            Ok(Response::new(ProtoStoreStatsReply {
                generated_at_unix_ms: report.generated_at_unix_ms,
                total_records: report.total_records,
                storage_bytes: report.storage_bytes,
                namespaces: report
                    .namespaces
                    .into_iter()
                    .map(namespace_stats_to_proto)
                    .collect(),
                maintenance: Some(maintenance_stats_to_proto(report.maintenance)),
                engine: Some(engine_tuning_info_to_proto(report.engine)),
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_stats, result)
    }

    async fn integrity_check(
        &self,
        request: Request<ProtoIntegrityCheckRequest>,
    ) -> Result<Response<ProtoIntegrityCheckReply>, Status> {
        self.runtime.metrics().grpc_integrity.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, tenant_id.as_deref())
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let report = self
                .store
                .integrity_check(IntegrityCheckRequest {
                    tenant_id: tenant_id.clone(),
                    namespace: namespace.clone(),
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::IntegrityCheck,
                "grpc",
                tenant_id,
                namespace,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary::default(),
                None,
                correlation_id,
            );

            Ok(Response::new(ProtoIntegrityCheckReply {
                generated_at_unix_ms: report.generated_at_unix_ms,
                healthy: report.healthy,
                scanned_records: report.scanned_records,
                scanned_idempotency_keys: report.scanned_idempotency_keys,
                stale_idempotency_keys: report.stale_idempotency_keys,
                missing_idempotency_keys: report.missing_idempotency_keys,
                duplicate_active_records: report.duplicate_active_records,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_integrity, result)
    }

    async fn repair(
        &self,
        request: Request<ProtoRepairRequest>,
    ) -> Result<Response<ProtoRepairReply>, Status> {
        self.runtime.metrics().grpc_repair.record_started();
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let result = async {
            let request = request.into_inner();
            let tenant_id = request.tenant_id.clone();
            let namespace = request.namespace.clone();
            let _permit = self
                .runtime
                .admission()
                .acquire(AdmissionClass::Admin, tenant_id.as_deref())
                .await
                .map_err(|error| {
                    map_grpc_admission_error(self.runtime.metrics().as_ref(), error)
                })?;
            let report = self
                .store
                .repair(RepairRequest {
                    tenant_id: tenant_id.clone(),
                    namespace: namespace.clone(),
                    dry_run: request.dry_run,
                    reason: request.reason,
                    remove_stale_idempotency_keys: request.remove_stale_idempotency_keys,
                    rebuild_missing_idempotency_keys: request.rebuild_missing_idempotency_keys,
                })
                .await
                .map_err(map_store_error)?;
            record_trace(
                &self.runtime,
                TraceOperationKind::Repair,
                "grpc",
                tenant_id,
                namespace,
                principal,
                started_at_unix_ms,
                TraceStatus::Ok,
                None,
                OperationTraceSummary {
                    dry_run: Some(request.dry_run),
                    ..OperationTraceSummary::default()
                },
                None,
                correlation_id,
            );

            Ok(Response::new(ProtoRepairReply {
                dry_run: report.dry_run,
                scanned_records: report.scanned_records,
                scanned_idempotency_keys: report.scanned_idempotency_keys,
                removed_stale_idempotency_keys: report.removed_stale_idempotency_keys,
                rebuilt_missing_idempotency_keys: report.rebuilt_missing_idempotency_keys,
                healthy_after: report.healthy_after,
            }))
        }
        .await;
        record_grpc_result(&self.runtime.metrics().grpc_repair, result)
    }

    async fn list_traces(
        &self,
        request: Request<ProtoListTracesRequest>,
    ) -> Result<Response<ProtoListTracesReply>, Status> {
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let request = request.into_inner();
        self.runtime
            .metrics()
            .trace_reads
            .fetch_add(1, Ordering::Relaxed);
        let traces = self.runtime.traces().list(&TraceListRequest {
            tenant_id: request.tenant_id,
            namespace: request.namespace,
            operation: ProtoTraceOperationKind::try_from(request.operation)
                .ok()
                .and_then(trace_operation_kind_from_proto),
            status: ProtoTraceStatus::try_from(request.status)
                .ok()
                .and_then(trace_status_from_proto),
            before_started_at_unix_ms: request.before_started_at_unix_ms,
            limit: request.limit.map(|value| value as usize),
        });
        Ok(Response::new(ProtoListTracesReply {
            traces: traces.into_iter().map(operation_trace_to_proto).collect(),
        }))
    }

    async fn get_trace(
        &self,
        request: Request<ProtoGetTraceRequest>,
    ) -> Result<Response<ProtoOperationTrace>, Status> {
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let trace_id = request.into_inner().trace_id;
        self.runtime
            .metrics()
            .trace_reads
            .fetch_add(1, Ordering::Relaxed);
        let trace = self
            .runtime
            .traces()
            .get(&trace_id)
            .ok_or_else(|| Status::not_found(format!("trace {trace_id} not found")))?;
        Ok(Response::new(operation_trace_to_proto(trace)))
    }

    async fn export(
        &self,
        request: Request<ProtoExportRequest>,
    ) -> Result<Response<ProtoExportReply>, Status> {
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let request = request.into_inner();
        let export_request = ExportRequest {
            tenant_id: request.tenant_id,
            namespace: request.namespace,
            include_archived: request.include_archived,
        };
        let _permit = self
            .runtime
            .admission()
            .acquire(AdmissionClass::Admin, export_request.tenant_id.as_deref())
            .await
            .map_err(|error| map_grpc_admission_error(self.runtime.metrics().as_ref(), error))?;
        let package = self
            .store
            .export(export_request.clone())
            .await
            .map_err(map_store_error)?;
        record_trace(
            &self.runtime,
            TraceOperationKind::Export,
            "grpc",
            export_request.tenant_id,
            export_request.namespace,
            principal,
            started_at_unix_ms,
            TraceStatus::Ok,
            None,
            OperationTraceSummary::default(),
            None,
            correlation_id,
        );
        Ok(Response::new(portable_package_to_proto(package)))
    }

    async fn import(
        &self,
        request: Request<ProtoImportRequest>,
    ) -> Result<Response<ProtoImportReply>, Status> {
        validate_grpc_auth(
            &request,
            self.runtime.auth().as_ref(),
            AuthPermission::Admin,
        )?;
        let principal = grpc_principal(&request);
        let started_at_unix_ms = now_unix_ms();
        let correlation_id = self.runtime.traces().next_id("corr");
        let request = request.into_inner();
        let import_request = ImportRequest {
            package: portable_package_from_proto(
                request
                    .package
                    .ok_or_else(|| invalid_argument("portable package is required"))?,
            )?,
            mode: import_mode_from_proto(request.mode)?,
            dry_run: request.dry_run,
        };
        let tenant_id = import_request
            .package
            .records
            .first()
            .map(|entry| entry.record.scope.tenant_id.clone());
        let namespace = import_request
            .package
            .records
            .first()
            .map(|entry| entry.record.scope.namespace.clone());
        let _permit = self
            .runtime
            .admission()
            .acquire(AdmissionClass::Admin, tenant_id.as_deref())
            .await
            .map_err(|error| map_grpc_admission_error(self.runtime.metrics().as_ref(), error))?;
        let report = self
            .store
            .import(import_request.clone())
            .await
            .map_err(map_store_error)?;
        record_trace(
            &self.runtime,
            TraceOperationKind::Import,
            "grpc",
            tenant_id,
            namespace,
            principal,
            started_at_unix_ms,
            if report.failed_records.is_empty() {
                TraceStatus::Ok
            } else {
                TraceStatus::Error
            },
            (!report.failed_records.is_empty())
                .then(|| format!("{} import validation failures", report.failed_records.len())),
            OperationTraceSummary {
                request_count: Some(import_request.package.records.len() as u32),
                dry_run: Some(import_request.dry_run),
                ..OperationTraceSummary::default()
            },
            None,
            correlation_id,
        );
        Ok(Response::new(ProtoImportReply {
            mode: request.mode,
            dry_run: report.dry_run,
            applied: report.applied,
            compatible_package: report.compatible_package,
            package_version: report.package_version,
            validated_records: report.validated_records,
            imported_records: report.imported_records,
            skipped_records: report.skipped_records,
            replaced_existing: report.replaced_existing,
            snapshot_id: report.snapshot_id,
            failed_records: report
                .failed_records
                .into_iter()
                .map(|failure| mnemara_protocol::v1::ImportFailure {
                    record_id: failure.record_id,
                    reason: failure.reason,
                })
                .collect(),
        }))
    }
}
