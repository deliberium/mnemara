use crate::error::Result;
use crate::model::MemoryRecord;
use crate::query::{
    CompactionReport, CompactionRequest, ExportRequest, ImportReport, ImportRequest,
    IntegrityCheckReport, IntegrityCheckRequest, RecallQuery, RecallResult, RepairReport,
    RepairRequest, SnapshotManifest, StoreStatsReport, StoreStatsRequest,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpsertRequest {
    pub record: MemoryRecord,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchUpsertRequest {
    pub requests: Vec<UpsertRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpsertReceipt {
    pub record_id: String,
    pub deduplicated: bool,
    pub summary_refreshed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub record_id: String,
    pub hard_delete: bool,
    pub audit_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteReceipt {
    pub record_id: String,
    pub tombstoned: bool,
    pub hard_deleted: bool,
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    fn backend_kind(&self) -> &'static str {
        "unknown"
    }

    async fn upsert(&self, request: UpsertRequest) -> Result<UpsertReceipt>;

    async fn batch_upsert(&self, request: BatchUpsertRequest) -> Result<Vec<UpsertReceipt>>;

    async fn recall(&self, query: RecallQuery) -> Result<RecallResult>;

    async fn compact(&self, request: CompactionRequest) -> Result<CompactionReport>;

    async fn delete(&self, request: DeleteRequest) -> Result<DeleteReceipt>;

    async fn snapshot(&self) -> Result<SnapshotManifest>;

    async fn stats(&self, request: StoreStatsRequest) -> Result<StoreStatsReport>;

    async fn integrity_check(&self, request: IntegrityCheckRequest)
    -> Result<IntegrityCheckReport>;

    async fn repair(&self, request: RepairRequest) -> Result<RepairReport>;

    async fn export(&self, request: ExportRequest) -> Result<crate::query::PortableStorePackage>;

    async fn import(&self, request: ImportRequest) -> Result<ImportReport>;
}
