use async_trait::async_trait;
use mnemara_core::{
    ArchiveReceipt, ArchiveRequest, BatchUpsertRequest, ChangefeedEvent, ChangefeedEventKind,
    ChangefeedReport, ChangefeedRequest, CompactionReport, CompactionRequest, DeleteReceipt,
    DeleteRequest, EngineConfig, Error, ExportRequest, GraphInspectionReport,
    GraphInspectionRequest, ImportFailure, ImportMode, ImportReport, ImportRequest,
    IntegrityCheckReport, IntegrityCheckRequest, LineageLink, LineageRelationKind,
    MaintenanceStats, MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryStore, MemoryTrustLevel, NamespaceStats, PortableRecord,
    PortableStorePackage, RecallExplanation, RecallHistoricalMode, RecallHit, RecallPlanner,
    RecallPlanningProfile, RecallPlanningTrace, RecallQuery, RecallResult, RecallScorer,
    RecallTemporalOrder, RecallTraceCandidate, RecoverReceipt, RecoverRequest, RepairReport,
    RepairRequest, Result, SemanticEmbedder, SnapshotManifest, StoreStatsReport, StoreStatsRequest,
    SuppressReceipt, SuppressRequest, SynthesisProposal, SynthesisReport, SynthesisRequest,
    TimeTravelRecallRequest, UpsertReceipt, UpsertRequest, build_graph_inspection_report,
};
use serde::{Deserialize, Serialize};
use sled::{Db, Tree};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const RECORDS_TREE: &str = "records";
const IDEMPOTENCY_TREE: &str = "idempotency";
const VERSIONS_TREE: &str = "versions";
const CHANGEFEED_TREE: &str = "changefeed";

#[derive(Clone)]
pub struct SledStoreConfig {
    pub data_dir: PathBuf,
    pub engine_config: EngineConfig,
    pub shared_embedder: Option<SharedEmbedderConfig>,
}

#[derive(Clone)]
pub struct SharedEmbedderConfig {
    pub embedder: Arc<dyn SemanticEmbedder>,
    pub provider_note: String,
}

impl SharedEmbedderConfig {
    fn new(embedder: Arc<dyn SemanticEmbedder>, provider_note: impl Into<String>) -> Self {
        Self {
            embedder,
            provider_note: provider_note.into(),
        }
    }
}

impl fmt::Debug for SharedEmbedderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedEmbedderConfig")
            .field("provider_note", &self.provider_note)
            .finish()
    }
}

impl fmt::Debug for SledStoreConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SledStoreConfig")
            .field("data_dir", &self.data_dir)
            .field("engine_config", &self.engine_config)
            .field("shared_embedder", &self.shared_embedder)
            .finish()
    }
}

impl SledStoreConfig {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            engine_config: EngineConfig::default(),
            shared_embedder: None,
        }
    }

    pub fn with_engine_config(mut self, engine_config: EngineConfig) -> Self {
        self.engine_config = engine_config;
        self
    }

    pub fn with_shared_embedder(
        mut self,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        self.shared_embedder = Some(SharedEmbedderConfig::new(embedder, provider_note));
        self
    }

    fn recall_planner(&self) -> RecallPlanner {
        if let Some(shared_embedder) = &self.shared_embedder {
            RecallPlanner::with_shared_embedder(
                self.engine_config.recall_planning_profile,
                self.engine_config.graph_expansion_max_hops,
                self.engine_config.recall_scorer_kind,
                self.engine_config.recall_scoring_profile,
                self.engine_config.recall_policy_profile,
                Arc::clone(&shared_embedder.embedder),
                shared_embedder.provider_note.clone(),
            )
        } else {
            RecallPlanner::from_engine_config(&self.engine_config)
        }
    }
}

#[derive(Debug)]
pub struct SledMemoryStore {
    db: Db,
    records: Tree,
    idempotency: Tree,
    versions: Tree,
    changefeed: Tree,
    config: SledStoreConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRecord {
    record: MemoryRecord,
    idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionedStoredRecord {
    record_id: String,
    observed_at_unix_ms: u64,
    record: Option<MemoryRecord>,
    idempotency_key: Option<String>,
}

#[derive(Debug, Clone)]
struct IdempotencyMapping {
    scoped_key: String,
    record_id: String,
}

type ScopeKeyParts = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);

#[derive(Debug, Clone, Copy, Default)]
struct IntegritySummary {
    scanned_records: u64,
    scanned_idempotency_keys: u64,
    stale_idempotency_keys: u64,
    missing_idempotency_keys: u64,
    duplicate_active_records: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RelativeTemporalBounds {
    after_unix_ms: Option<u64>,
    before_unix_ms: Option<u64>,
}

const PORTABLE_PACKAGE_VERSION: u32 = 1;

impl SledMemoryStore {
    pub fn open(config: SledStoreConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir).map_err(|err| {
            Error::Backend(format!(
                "failed to create sled store dir {}: {err}",
                config.data_dir.display()
            ))
        })?;
        let db = sled::open(&config.data_dir).map_err(|err| {
            Error::Backend(format!(
                "failed to open sled db {}: {err}",
                config.data_dir.display()
            ))
        })?;
        let records = db
            .open_tree(RECORDS_TREE)
            .map_err(|err| Error::Backend(format!("failed to open records tree: {err}")))?;
        let idempotency = db
            .open_tree(IDEMPOTENCY_TREE)
            .map_err(|err| Error::Backend(format!("failed to open idempotency tree: {err}")))?;
        let versions = db
            .open_tree(VERSIONS_TREE)
            .map_err(|err| Error::Backend(format!("failed to open versions tree: {err}")))?;
        let changefeed = db
            .open_tree(CHANGEFEED_TREE)
            .map_err(|err| Error::Backend(format!("failed to open changefeed tree: {err}")))?;
        Ok(Self {
            db,
            records,
            idempotency,
            versions,
            changefeed,
            config,
        })
    }

    fn validate_record(record: &MemoryRecord) -> Result<()> {
        record.validate()
    }

    fn validate_upsert_request(&self, request: &UpsertRequest) -> Result<()> {
        Self::validate_record(&request.record)?;
        if request.idempotency_key.is_none()
            && self
                .config
                .engine_config
                .ingestion
                .idempotent_writes_required
        {
            return Err(Error::InvalidRequest(
                "idempotency_key is required by the current ingestion policy".to_string(),
            ));
        }
        if self.config.engine_config.ingestion.require_source_labels
            && request.record.scope.labels.is_empty()
        {
            return Err(Error::InvalidRequest(
                "at least one source label is required by the current ingestion policy".to_string(),
            ));
        }
        Ok(())
    }

    fn is_pinned(record: &MemoryRecord) -> bool {
        record.scope.trust_level == MemoryTrustLevel::Pinned
    }

    fn retention_exempt(&self, record: &MemoryRecord) -> bool {
        self.config.engine_config.retention.pinned_records_exempt && Self::is_pinned(record)
    }

    fn validate_delete_request(request: &DeleteRequest) -> Result<()> {
        if request.tenant_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "delete tenant_id is required".to_string(),
            ));
        }
        if request.namespace.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "delete namespace is required".to_string(),
            ));
        }
        if request.record_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "delete record_id is required".to_string(),
            ));
        }
        if request.audit_reason.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "delete audit_reason is required".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_archive_request(request: &ArchiveRequest) -> Result<()> {
        Self::validate_lifecycle_request(
            "archive",
            &request.tenant_id,
            &request.namespace,
            &request.record_id,
            &request.audit_reason,
        )
    }

    fn validate_suppress_request(request: &SuppressRequest) -> Result<()> {
        Self::validate_lifecycle_request(
            "suppress",
            &request.tenant_id,
            &request.namespace,
            &request.record_id,
            &request.audit_reason,
        )
    }

    fn validate_recover_request(request: &RecoverRequest) -> Result<()> {
        Self::validate_lifecycle_request(
            "recover",
            &request.tenant_id,
            &request.namespace,
            &request.record_id,
            &request.audit_reason,
        )?;
        match request.quality_state {
            MemoryQualityState::Active | MemoryQualityState::Verified => {}
            _ => {
                return Err(Error::InvalidRequest(
                    "recover quality_state must be Active or Verified".to_string(),
                ));
            }
        }
        if matches!(
            request.historical_state,
            Some(MemoryHistoricalState::Superseded)
        ) {
            return Err(Error::InvalidRequest(
                "recover historical_state cannot be Superseded".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_lifecycle_request(
        action: &str,
        tenant_id: &str,
        namespace: &str,
        record_id: &str,
        audit_reason: &str,
    ) -> Result<()> {
        if tenant_id.trim().is_empty() {
            return Err(Error::InvalidRequest(format!(
                "{action} tenant_id is required"
            )));
        }
        if namespace.trim().is_empty() {
            return Err(Error::InvalidRequest(format!(
                "{action} namespace is required"
            )));
        }
        if record_id.trim().is_empty() {
            return Err(Error::InvalidRequest(format!(
                "{action} record_id is required"
            )));
        }
        if audit_reason.trim().is_empty() {
            return Err(Error::InvalidRequest(format!(
                "{action} audit_reason is required"
            )));
        }
        Ok(())
    }

    fn validate_record_scope(
        stored: &StoredRecord,
        tenant_id: &str,
        namespace: &str,
    ) -> Result<()> {
        if stored.record.scope.tenant_id != tenant_id {
            return Err(Error::InvalidRequest(format!(
                "record {} does not belong to tenant {}",
                stored.record.id, tenant_id
            )));
        }
        if stored.record.scope.namespace != namespace {
            return Err(Error::InvalidRequest(format!(
                "record {} does not belong to namespace {}",
                stored.record.id, namespace
            )));
        }
        Ok(())
    }

    fn validate_import_request(
        &self,
        request: &ImportRequest,
    ) -> (u64, bool, Vec<ImportFailure>, Vec<PortableRecord>) {
        let mut validated_records = 0u64;
        let mut failures = Vec::new();
        let mut entries = Vec::with_capacity(request.package.records.len());

        if request.package.package_version != PORTABLE_PACKAGE_VERSION {
            failures.push(ImportFailure {
                record_id: None,
                reason: format!(
                    "unsupported portable package version {}; expected {}",
                    request.package.package_version, PORTABLE_PACKAGE_VERSION
                ),
            });
        }

        if request.package.manifest.record_count != request.package.records.len() as u64 {
            failures.push(ImportFailure {
                record_id: None,
                reason: format!(
                    "portable package manifest record_count={} does not match payload records={}",
                    request.package.manifest.record_count,
                    request.package.records.len()
                ),
            });
        }

        for entry in &request.package.records {
            match Self::validate_record(&entry.record) {
                Ok(()) => {
                    validated_records += 1;
                    entries.push(entry.clone());
                }
                Err(error) => failures.push(ImportFailure {
                    record_id: Some(entry.record.id.clone()),
                    reason: error.to_string(),
                }),
            }
        }

        (validated_records, failures.is_empty(), failures, entries)
    }

    fn now_unix_ms() -> Result<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| Error::Backend(format!("system clock error: {err}")))
            .map(|value| value.as_millis() as u64)
    }

    fn encode_record(stored: &StoredRecord) -> Result<Vec<u8>> {
        serde_json::to_vec(stored)
            .map_err(|err| Error::Backend(format!("failed to encode record: {err}")))
    }

    fn decode_record(value: &[u8]) -> Result<StoredRecord> {
        serde_json::from_slice::<StoredRecord>(value)
            .map_err(|err| Error::Backend(format!("failed to decode stored record: {err}")))
    }

    fn version_key(record_id: &str, observed_at_unix_ms: u64) -> String {
        format!("{record_id}\u{1f}{observed_at_unix_ms:020}")
    }

    fn encode_record_version(version: &VersionedStoredRecord) -> Result<Vec<u8>> {
        serde_json::to_vec(version)
            .map_err(|err| Error::Backend(format!("failed to encode record version: {err}")))
    }

    fn decode_record_version(value: &[u8]) -> Result<VersionedStoredRecord> {
        serde_json::from_slice::<VersionedStoredRecord>(value)
            .map_err(|err| Error::Backend(format!("failed to decode record version: {err}")))
    }

    fn persist_record_version(
        &self,
        record_id: &str,
        observed_at_unix_ms: u64,
        stored: Option<&StoredRecord>,
    ) -> Result<()> {
        let version = VersionedStoredRecord {
            record_id: record_id.to_string(),
            observed_at_unix_ms,
            record: stored.map(|value| value.record.clone()),
            idempotency_key: stored.and_then(|value| value.idempotency_key.clone()),
        };
        self.versions
            .insert(
                Self::version_key(record_id, observed_at_unix_ms).as_bytes(),
                Self::encode_record_version(&version)?,
            )
            .map_err(|err| Error::Backend(format!("failed to write record version: {err}")))?;
        Ok(())
    }

    fn records_as_of(&self, as_of_unix_ms: u64) -> Result<Vec<StoredRecord>> {
        let mut latest = BTreeMap::<String, VersionedStoredRecord>::new();
        for item in self.versions.iter() {
            let (_, value) =
                item.map_err(|err| Error::Backend(format!("sled iteration failed: {err}")))?;
            let version = Self::decode_record_version(&value)?;
            if version.observed_at_unix_ms > as_of_unix_ms {
                continue;
            }
            let replace = latest
                .get(&version.record_id)
                .is_none_or(|existing| existing.observed_at_unix_ms <= version.observed_at_unix_ms);
            if replace {
                latest.insert(version.record_id.clone(), version);
            }
        }
        Ok(latest
            .into_values()
            .filter_map(|version| {
                version.record.map(|record| StoredRecord {
                    record,
                    idempotency_key: version.idempotency_key,
                })
            })
            .collect())
    }

    fn next_changefeed_sequence(&self) -> Result<u64> {
        self.changefeed
            .last()
            .map_err(|err| Error::Backend(format!("failed to read changefeed tail: {err}")))?
            .map(|(key, _)| {
                let raw = String::from_utf8(key.to_vec()).map_err(|err| {
                    Error::Backend(format!("invalid changefeed sequence encoding: {err}"))
                })?;
                raw.parse::<u64>()
                    .map(|value| value.saturating_add(1))
                    .map_err(|err| Error::Backend(format!("invalid changefeed sequence: {err}")))
            })
            .transpose()
            .map(|value| value.unwrap_or(1))
    }

    #[allow(clippy::too_many_arguments)]
    fn append_changefeed_event(
        &self,
        kind: ChangefeedEventKind,
        record: Option<&MemoryRecord>,
        tenant_id: &str,
        namespace: &str,
        record_id: Option<&str>,
        summary: Option<String>,
        occurred_at_unix_ms: u64,
    ) -> Result<()> {
        let sequence = self.next_changefeed_sequence()?;
        let event = ChangefeedEvent {
            sequence,
            event_id: format!("change:{}:{sequence}", self.backend_kind()),
            kind,
            tenant_id: tenant_id.to_string(),
            namespace: namespace.to_string(),
            record_id: record_id.map(ToOwned::to_owned),
            occurred_at_unix_ms,
            summary,
            record: record.cloned(),
        };
        let encoded = serde_json::to_vec(&event)
            .map_err(|err| Error::Backend(format!("failed to encode changefeed event: {err}")))?;
        self.changefeed
            .insert(format!("{sequence:020}").as_bytes(), encoded)
            .map_err(|err| Error::Backend(format!("failed to write changefeed event: {err}")))?;
        Ok(())
    }

    fn idempotency_scope_key(scope: &MemoryScope, key: &str) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            scope.tenant_id,
            scope.namespace,
            scope.actor_id,
            scope.conversation_id.as_deref().unwrap_or(""),
            scope.session_id.as_deref().unwrap_or(""),
            key
        )
    }

    fn fetch_record(&self, record_id: &str) -> Result<Option<StoredRecord>> {
        self.records
            .get(record_id.as_bytes())
            .map_err(|err| Error::Backend(format!("failed to read record: {err}")))?
            .map(|value| Self::decode_record(&value))
            .transpose()
    }

    fn remove_idempotency_mapping(&self, record: &StoredRecord) -> Result<()> {
        if let Some(idempotency_key) = &record.idempotency_key {
            let scoped_key = Self::idempotency_scope_key(&record.record.scope, idempotency_key);
            self.idempotency
                .remove(scoped_key.as_bytes())
                .map_err(|err| {
                    Error::Backend(format!("failed to remove idempotency key: {err}"))
                })?;
        }
        Ok(())
    }

    fn matches_scope(candidate: &MemoryScope, query: &MemoryScope) -> bool {
        candidate.tenant_id == query.tenant_id
            && candidate.namespace == query.namespace
            && candidate.actor_id == query.actor_id
            && (query.conversation_id.is_none()
                || candidate.conversation_id == query.conversation_id)
            && (query.session_id.is_none() || candidate.session_id == query.session_id)
    }

    fn record_passes_filters(
        record: &MemoryRecord,
        query: &RecallQuery,
        relative_bounds: RelativeTemporalBounds,
    ) -> bool {
        if !Self::matches_scope(&record.scope, &query.scope) {
            return false;
        }

        if let Some(expires_at_unix_ms) = record.expires_at_unix_ms
            && expires_at_unix_ms <= Self::now_unix_ms().unwrap_or(u64::MAX)
        {
            return false;
        }

        if let Some(min_score) = query.filters.min_importance_score
            && record.importance_score < min_score
        {
            return false;
        }

        if let Some(source) = &query.filters.source
            && &record.scope.source != source
        {
            return false;
        }

        if let Some(from_unix_ms) = query.filters.from_unix_ms
            && record.updated_at_unix_ms < from_unix_ms
        {
            return false;
        }

        if let Some(to_unix_ms) = query.filters.to_unix_ms
            && record.updated_at_unix_ms > to_unix_ms
        {
            return false;
        }

        if let Some(after_unix_ms) = relative_bounds.after_unix_ms
            && Self::record_temporal_anchor(record) <= after_unix_ms
        {
            return false;
        }

        if let Some(before_unix_ms) = relative_bounds.before_unix_ms
            && Self::record_temporal_anchor(record) >= before_unix_ms
        {
            return false;
        }

        if !query.filters.trust_levels.is_empty()
            && !query
                .filters
                .trust_levels
                .contains(&record.scope.trust_level)
        {
            return false;
        }

        if !query.filters.required_labels.is_empty()
            && !query.filters.required_labels.iter().all(|label| {
                record
                    .scope
                    .labels
                    .iter()
                    .any(|candidate| candidate == label)
            })
        {
            return false;
        }

        if !query.filters.kinds.is_empty() && !query.filters.kinds.contains(&record.kind) {
            return false;
        }

        if let Some(episode_id) = &query.filters.episode_id
            && record.episode.as_ref().map(|episode| &episode.episode_id) != Some(episode_id)
        {
            return false;
        }

        if !query.filters.continuity_states.is_empty()
            && !record.episode.as_ref().is_some_and(|episode| {
                query
                    .filters
                    .continuity_states
                    .contains(&episode.continuity_state)
            })
        {
            return false;
        }

        if query.filters.unresolved_only
            && !record
                .episode
                .as_ref()
                .is_some_and(|episode| episode.continuity_state.is_unresolved())
        {
            return false;
        }

        if let Some(lineage_record_id) = &query.filters.lineage_record_id
            && record.id != *lineage_record_id
            && !record
                .lineage
                .iter()
                .any(|link| &link.record_id == lineage_record_id)
        {
            return false;
        }

        if !query.filters.boundary_labels.is_empty()
            && !record.episode.as_ref().is_some_and(|episode| {
                episode.boundary_label.as_ref().is_some_and(|label| {
                    query
                        .filters
                        .boundary_labels
                        .iter()
                        .any(|expected| expected == label)
                })
            })
        {
            return false;
        }

        if let Some(recurrence_key) = &query.filters.recurrence_key
            && record
                .episode
                .as_ref()
                .and_then(|episode| episode.recurrence_key.as_ref())
                != Some(recurrence_key)
        {
            return false;
        }

        if !query.filters.conflict_states.is_empty()
            && !record
                .conflict
                .as_ref()
                .is_some_and(|conflict| query.filters.conflict_states.contains(&conflict.state))
        {
            return false;
        }

        if !query.filters.resolution_kinds.is_empty()
            && !record.conflict.as_ref().is_some_and(|conflict| {
                query
                    .filters
                    .resolution_kinds
                    .contains(&conflict.resolution)
            })
        {
            return false;
        }

        if query.filters.unresolved_conflicts_only
            && !record.conflict.as_ref().is_some_and(|conflict| {
                matches!(
                    conflict.state,
                    mnemara_core::ConflictReviewState::PotentialConflict
                        | mnemara_core::ConflictReviewState::UnderReview
                )
            })
        {
            return false;
        }

        match query.filters.historical_mode {
            RecallHistoricalMode::CurrentOnly => {
                if !matches!(record.historical_state, MemoryHistoricalState::Current) {
                    return false;
                }
            }
            RecallHistoricalMode::HistoricalOnly => {
                if matches!(record.historical_state, MemoryHistoricalState::Current) {
                    return false;
                }
            }
            RecallHistoricalMode::IncludeHistorical => {}
        }

        if !query.filters.states.is_empty() {
            if !query.filters.states.contains(&record.quality_state) {
                return false;
            }
        } else {
            match record.quality_state {
                MemoryQualityState::Archived if !query.filters.include_archived => return false,
                MemoryQualityState::Deleted | MemoryQualityState::Suppressed => return false,
                _ => {}
            }
        }

        true
    }

    fn relative_temporal_bounds(
        records: &[StoredRecord],
        query: &RecallQuery,
    ) -> Result<RelativeTemporalBounds> {
        let mut bounds = RelativeTemporalBounds::default();
        if let Some(after_record_id) = &query.filters.after_record_id {
            let Some(anchor) = records
                .iter()
                .find(|stored| {
                    stored.record.id == *after_record_id
                        && Self::matches_scope(&stored.record.scope, &query.scope)
                })
                .map(|stored| Self::record_temporal_anchor(&stored.record))
            else {
                return Err(Error::InvalidRequest(format!(
                    "after_record_id '{after_record_id}' was not found in recall scope"
                )));
            };
            bounds.after_unix_ms = Some(anchor);
        }
        if let Some(before_record_id) = &query.filters.before_record_id {
            let Some(anchor) = records
                .iter()
                .find(|stored| {
                    stored.record.id == *before_record_id
                        && Self::matches_scope(&stored.record.scope, &query.scope)
                })
                .map(|stored| Self::record_temporal_anchor(&stored.record))
            else {
                return Err(Error::InvalidRequest(format!(
                    "before_record_id '{before_record_id}' was not found in recall scope"
                )));
            };
            bounds.before_unix_ms = Some(anchor);
        }
        if let (Some(after), Some(before)) = (bounds.after_unix_ms, bounds.before_unix_ms)
            && after >= before
        {
            return Err(Error::InvalidRequest(
                "after_record_id must refer to an earlier record than before_record_id".to_string(),
            ));
        }
        Ok(bounds)
    }

    fn iterate_records(&self) -> Result<Vec<StoredRecord>> {
        let mut records = Vec::new();
        for item in self.records.iter() {
            let (_, value) =
                item.map_err(|err| Error::Backend(format!("sled iteration failed: {err}")))?;
            records.push(Self::decode_record(&value)?);
        }
        Ok(records)
    }

    fn iterate_idempotency_mappings(&self) -> Result<Vec<IdempotencyMapping>> {
        let mut mappings = Vec::new();
        for item in self.idempotency.iter() {
            let (key, value) =
                item.map_err(|err| Error::Backend(format!("sled iteration failed: {err}")))?;
            let scoped_key = String::from_utf8(key.to_vec()).map_err(|err| {
                Error::Backend(format!("invalid idempotency key encoding: {err}"))
            })?;
            let record_id = String::from_utf8(value.to_vec()).map_err(|err| {
                Error::Backend(format!("invalid idempotency value encoding: {err}"))
            })?;
            mappings.push(IdempotencyMapping {
                scoped_key,
                record_id,
            });
        }
        Ok(mappings)
    }

    fn parse_scope_key(scoped_key: &str) -> Option<ScopeKeyParts> {
        let parts = scoped_key.split('\u{1f}').collect::<Vec<_>>();
        if parts.len() != 6 {
            return None;
        }
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
            (!parts[3].is_empty()).then(|| parts[3].to_string()),
            (!parts[4].is_empty()).then(|| parts[4].to_string()),
            parts[5].to_string(),
        ))
    }

    fn scope_matches_filters(
        tenant_id: &str,
        namespace: &str,
        tenant_filter: Option<&str>,
        namespace_filter: Option<&str>,
    ) -> bool {
        tenant_filter.is_none_or(|expected| tenant_id == expected)
            && namespace_filter.is_none_or(|expected| namespace == expected)
    }

    fn build_integrity_summary(
        &self,
        tenant_filter: Option<&str>,
        namespace_filter: Option<&str>,
    ) -> Result<IntegritySummary> {
        let records = self.iterate_records()?;
        let mappings = self.iterate_idempotency_mappings()?;
        let now_unix_ms = Self::now_unix_ms()?;

        let filtered_records = records
            .iter()
            .filter(|stored| {
                Self::scope_matches_filters(
                    &stored.record.scope.tenant_id,
                    &stored.record.scope.namespace,
                    tenant_filter,
                    namespace_filter,
                )
            })
            .collect::<Vec<_>>();

        let mut mapping_lookup = HashMap::new();
        let mut stale_idempotency_keys = 0u64;
        let mut scanned_idempotency_keys = 0u64;

        for mapping in &mappings {
            let Some((tenant_id, namespace, _, _, _, idempotency_key)) =
                Self::parse_scope_key(&mapping.scoped_key)
            else {
                stale_idempotency_keys += 1;
                scanned_idempotency_keys += 1;
                continue;
            };
            if !Self::scope_matches_filters(&tenant_id, &namespace, tenant_filter, namespace_filter)
            {
                continue;
            }
            scanned_idempotency_keys += 1;
            let Some(stored) = self.fetch_record(&mapping.record_id)? else {
                stale_idempotency_keys += 1;
                continue;
            };
            if stored.record.scope.tenant_id != tenant_id
                || stored.record.scope.namespace != namespace
                || stored.idempotency_key.as_deref() != Some(idempotency_key.as_str())
                || Self::idempotency_scope_key(&stored.record.scope, &idempotency_key)
                    != mapping.scoped_key
            {
                stale_idempotency_keys += 1;
                continue;
            }
            mapping_lookup.insert(mapping.scoped_key.clone(), mapping.record_id.clone());
        }

        let mut duplicate_groups = HashMap::<String, usize>::new();
        let mut missing_idempotency_keys = 0u64;
        let mut duplicate_active_records = 0u64;

        for stored in &filtered_records {
            if let Some(idempotency_key) = &stored.idempotency_key {
                let scoped_key = Self::idempotency_scope_key(&stored.record.scope, idempotency_key);
                if mapping_lookup.get(&scoped_key) != Some(&stored.record.id) {
                    missing_idempotency_keys += 1;
                }
            }

            if stored
                .record
                .expires_at_unix_ms
                .is_some_and(|value| value <= now_unix_ms)
            {
                // Counted in stats, not integrity.
            }

            if !matches!(
                stored.record.quality_state,
                MemoryQualityState::Archived
                    | MemoryQualityState::Deleted
                    | MemoryQualityState::Suppressed
            ) {
                *duplicate_groups
                    .entry(Self::dedup_signature(&stored.record))
                    .or_default() += 1;
            }
        }

        for group_size in duplicate_groups.into_values() {
            if group_size > 1 {
                duplicate_active_records += (group_size - 1) as u64;
            }
        }

        Ok(IntegritySummary {
            scanned_records: filtered_records.len() as u64,
            scanned_idempotency_keys,
            stale_idempotency_keys,
            missing_idempotency_keys,
            duplicate_active_records,
        })
    }

    fn build_stats_report(&self, request: &StoreStatsRequest) -> Result<StoreStatsReport> {
        let records = self.iterate_records()?;
        let tenant_filter = request.tenant_id.as_deref();
        let namespace_filter = request.namespace.as_deref();
        let filtered_records = records
            .iter()
            .filter(|stored| {
                Self::scope_matches_filters(
                    &stored.record.scope.tenant_id,
                    &stored.record.scope.namespace,
                    tenant_filter,
                    namespace_filter,
                )
            })
            .collect::<Vec<_>>();
        let now_unix_ms = Self::now_unix_ms()?;
        let integrity = self.build_integrity_summary(tenant_filter, namespace_filter)?;
        let mut namespace_map = BTreeMap::<(String, String), NamespaceStats>::new();
        let mut duplicate_groups = HashMap::<String, usize>::new();
        let mut tombstoned_records = 0u64;
        let mut expired_records = 0u64;

        for stored in &filtered_records {
            let key = (
                stored.record.scope.tenant_id.clone(),
                stored.record.scope.namespace.clone(),
            );
            let entry = namespace_map
                .entry(key.clone())
                .or_insert_with(|| NamespaceStats {
                    tenant_id: key.0.clone(),
                    namespace: key.1.clone(),
                    active_records: 0,
                    archived_records: 0,
                    deleted_records: 0,
                    suppressed_records: 0,
                    pinned_records: 0,
                });
            match stored.record.quality_state {
                MemoryQualityState::Archived => entry.archived_records += 1,
                MemoryQualityState::Deleted => {
                    entry.deleted_records += 1;
                    tombstoned_records += 1;
                }
                MemoryQualityState::Suppressed => entry.suppressed_records += 1,
                _ => entry.active_records += 1,
            }
            if stored.record.scope.trust_level == MemoryTrustLevel::Pinned {
                entry.pinned_records += 1;
            }
            if stored
                .record
                .expires_at_unix_ms
                .is_some_and(|value| value <= now_unix_ms)
            {
                expired_records += 1;
            }
            if !matches!(
                stored.record.quality_state,
                MemoryQualityState::Archived
                    | MemoryQualityState::Deleted
                    | MemoryQualityState::Suppressed
            ) {
                *duplicate_groups
                    .entry(Self::dedup_signature(&stored.record))
                    .or_default() += 1;
            }
        }

        let mut duplicate_candidate_groups = 0u64;
        let mut duplicate_candidate_records = 0u64;
        let mut historical_records = 0u64;
        let mut superseded_records = 0u64;
        let mut lineage_links = 0u64;

        for stored in &filtered_records {
            if matches!(
                stored.record.historical_state,
                MemoryHistoricalState::Historical
            ) {
                historical_records += 1;
            }
            if matches!(
                stored.record.historical_state,
                MemoryHistoricalState::Superseded
            ) {
                superseded_records += 1;
            }
            lineage_links += stored.record.lineage.len() as u64;
        }

        for group_size in duplicate_groups.into_values() {
            if group_size > 1 {
                duplicate_candidate_groups += 1;
                duplicate_candidate_records += (group_size - 1) as u64;
            }
        }

        Ok(StoreStatsReport {
            generated_at_unix_ms: now_unix_ms,
            total_records: filtered_records.len() as u64,
            storage_bytes: self
                .db
                .size_on_disk()
                .map_err(|err| Error::Backend(format!("failed to determine db size: {err}")))?,
            namespaces: namespace_map.into_values().collect(),
            maintenance: MaintenanceStats {
                duplicate_candidate_groups,
                duplicate_candidate_records,
                tombstoned_records,
                expired_records,
                stale_idempotency_keys: integrity.stale_idempotency_keys,
                historical_records,
                superseded_records,
                lineage_links,
            },
            engine: self.config.engine_config.tuning_info(),
        })
    }

    fn approximate_tokens(record: &MemoryRecord) -> usize {
        let content_tokens = record.content.split_whitespace().count();
        let summary_tokens = record
            .summary
            .as_deref()
            .map(|summary| summary.split_whitespace().count())
            .unwrap_or(0);
        content_tokens + summary_tokens
    }

    fn record_temporal_anchor(record: &MemoryRecord) -> u64 {
        record
            .episode
            .as_ref()
            .and_then(|episode| episode.last_active_unix_ms.or(episode.started_at_unix_ms))
            .unwrap_or(record.updated_at_unix_ms)
    }

    fn selected_channels_for_hit(hit: &RecallHit, empty_query: bool) -> Vec<String> {
        let mut selected_channels = if empty_query {
            vec!["temporal".to_string(), "policy".to_string()]
        } else {
            vec!["lexical".to_string(), "policy".to_string()]
        };
        if hit.breakdown.semantic > 0.0 {
            selected_channels.push("semantic".to_string());
        }
        if hit.breakdown.metadata > 0.0 {
            selected_channels.push("metadata".to_string());
        }
        if hit.breakdown.episodic > 0.0 {
            selected_channels.push("episodic".to_string());
        }
        if hit.breakdown.salience > 0.0 {
            selected_channels.push("salience".to_string());
        }
        if hit.breakdown.curation > 0.0 {
            selected_channels.push("curation".to_string());
        }
        if hit.record.conflict.is_some() {
            selected_channels.push("conflict".to_string());
        }
        selected_channels.sort();
        selected_channels.dedup();
        selected_channels
    }

    fn planning_profile_note(profile: RecallPlanningProfile) -> &'static str {
        match profile {
            RecallPlanningProfile::FastPath => "planning_profile=fast_path",
            RecallPlanningProfile::ContinuityAware => "planning_profile=continuity_aware",
        }
    }

    fn dedup_signature(record: &MemoryRecord) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            record.scope.tenant_id,
            record.scope.namespace,
            record.scope.actor_id,
            record.kind as u8,
            record.content.trim().to_ascii_lowercase(),
            record
                .summary
                .clone()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        )
    }

    fn summary_record_id(signature: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        signature.hash(&mut hasher);
        format!("compacted-summary-{:016x}", hasher.finish())
    }

    fn compaction_summary_record(
        group: &[StoredRecord],
        signature: &str,
        now_unix_ms: u64,
    ) -> StoredRecord {
        let canonical = &group[0].record;
        let representative_summary = canonical
            .summary
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| canonical.content.clone());
        let cluster_size = group.len();
        let max_importance_score = group
            .iter()
            .map(|stored| stored.record.importance_score)
            .fold(canonical.importance_score, f32::max);

        let mut metadata = BTreeMap::new();
        metadata.insert(
            "compaction_reason".to_string(),
            "duplicate_cluster_rollup".to_string(),
        );
        metadata.insert(
            "compaction_cluster_size".to_string(),
            cluster_size.to_string(),
        );
        metadata.insert("representative_record_id".to_string(), canonical.id.clone());

        let mut labels = canonical.scope.labels.clone();
        if !labels.iter().any(|label| label == "compacted") {
            labels.push("compacted".to_string());
        }

        StoredRecord {
            record: MemoryRecord {
                id: Self::summary_record_id(signature),
                scope: MemoryScope {
                    tenant_id: canonical.scope.tenant_id.clone(),
                    namespace: canonical.scope.namespace.clone(),
                    actor_id: canonical.scope.actor_id.clone(),
                    conversation_id: canonical.scope.conversation_id.clone(),
                    session_id: canonical.scope.session_id.clone(),
                    source: canonical.scope.source.clone(),
                    labels,
                    trust_level: canonical.scope.trust_level,
                },
                kind: mnemara_core::MemoryRecordKind::Summary,
                content: format!(
                    "Compacted {} related records into a durable summary. Representative memory: {}",
                    cluster_size, representative_summary
                ),
                summary: Some(format!(
                    "{} related records: {}",
                    cluster_size, representative_summary
                )),
                source_id: None,
                metadata,
                quality_state: if matches!(canonical.quality_state, MemoryQualityState::Verified) {
                    MemoryQualityState::Verified
                } else {
                    MemoryQualityState::Active
                },
                created_at_unix_ms: now_unix_ms,
                updated_at_unix_ms: now_unix_ms,
                expires_at_unix_ms: None,
                importance_score: max_importance_score,
                artifact: canonical.artifact.clone(),
                episode: canonical.episode.clone(),
                historical_state: MemoryHistoricalState::Current,
                lineage: group
                    .iter()
                    .map(|stored| LineageLink {
                        record_id: stored.record.id.clone(),
                        relation: LineageRelationKind::ConsolidatedFrom,
                        confidence: 1.0,
                    })
                    .collect(),
                conflict: canonical.conflict.clone(),
            },
            idempotency_key: None,
        }
    }

    fn cold_tiering_candidates(
        &self,
        tenant_id: &str,
        namespace: Option<&str>,
        now_unix_ms: u64,
    ) -> Result<Vec<StoredRecord>> {
        let cold_archive_after_days = self.config.engine_config.compaction.cold_archive_after_days;
        if cold_archive_after_days == 0 {
            return Ok(Vec::new());
        }
        let archive_threshold_ms =
            u64::from(cold_archive_after_days).saturating_mul(24 * 60 * 60 * 1_000);
        let max_importance = f32::from(
            self.config
                .engine_config
                .compaction
                .cold_archive_importance_threshold_per_mille,
        ) / 1000.0;

        Ok(self
            .iterate_records()?
            .into_iter()
            .filter(|stored| stored.record.scope.tenant_id == tenant_id)
            .filter(|stored| namespace.is_none_or(|value| stored.record.scope.namespace == value))
            .filter(|stored| {
                matches!(
                    stored.record.quality_state,
                    MemoryQualityState::Draft
                        | MemoryQualityState::Active
                        | MemoryQualityState::Verified
                )
            })
            .filter(|stored| !Self::is_pinned(&stored.record))
            .filter(|stored| {
                now_unix_ms.saturating_sub(stored.record.updated_at_unix_ms) > archive_threshold_ms
                    && stored.record.importance_score <= max_importance
            })
            .collect())
    }

    fn synthesis_group_key(record: &MemoryRecord) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            record.scope.namespace,
            record.scope.actor_id,
            record.scope.conversation_id.as_deref().unwrap_or(""),
            record.scope.session_id.as_deref().unwrap_or(""),
            record.scope.source
        )
    }

    fn synthesis_record_id(source_ids: &[String]) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for source_id in source_ids {
            source_id.hash(&mut hasher);
        }
        format!("synthesis-proposal-{:016x}", hasher.finish())
    }

    fn synthesis_content(group: &[StoredRecord]) -> String {
        let snippets = group
            .iter()
            .take(5)
            .map(|stored| {
                stored
                    .record
                    .summary
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| stored.record.content.clone())
            })
            .map(|value| {
                let value = value.trim();
                let truncated = value.chars().take(160).collect::<String>();
                if truncated.len() < value.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            })
            .collect::<Vec<_>>();
        format!(
            "Synthesized {} source memories into a reviewable summary: {}",
            group.len(),
            snippets.join("; ")
        )
    }

    fn synthesis_proposal_from_group(
        group: &[StoredRecord],
        reason: &str,
        now_unix_ms: u64,
    ) -> SynthesisProposal {
        let canonical = &group[0].record;
        let source_record_ids = group
            .iter()
            .map(|stored| stored.record.id.clone())
            .collect::<Vec<_>>();
        let max_importance_score = group
            .iter()
            .map(|stored| stored.record.importance_score)
            .fold(canonical.importance_score, f32::max);
        let mut labels = canonical.scope.labels.clone();
        for label in ["synthesized", "review-required"] {
            if !labels.iter().any(|existing| existing == label) {
                labels.push(label.to_string());
            }
        }
        let content = Self::synthesis_content(group);
        let mut metadata = BTreeMap::new();
        metadata.insert("synthesis_reason".to_string(), reason.to_string());
        metadata.insert(
            "synthesis_strategy".to_string(),
            "deterministic_scope_rollup".to_string(),
        );
        metadata.insert(
            "synthesis_source_count".to_string(),
            group.len().to_string(),
        );
        metadata.insert("review_state".to_string(), "proposed".to_string());
        let confidence = (0.55 + (group.len().min(9) as f32 * 0.05)).min(0.95);
        let proposed_record = MemoryRecord {
            id: Self::synthesis_record_id(&source_record_ids),
            scope: MemoryScope {
                tenant_id: canonical.scope.tenant_id.clone(),
                namespace: canonical.scope.namespace.clone(),
                actor_id: canonical.scope.actor_id.clone(),
                conversation_id: canonical.scope.conversation_id.clone(),
                session_id: canonical.scope.session_id.clone(),
                source: canonical.scope.source.clone(),
                labels,
                trust_level: MemoryTrustLevel::Derived,
            },
            kind: MemoryRecordKind::Summary,
            content: content.clone(),
            summary: Some(content),
            source_id: None,
            metadata: metadata.clone(),
            quality_state: MemoryQualityState::Draft,
            created_at_unix_ms: now_unix_ms,
            updated_at_unix_ms: now_unix_ms,
            expires_at_unix_ms: None,
            importance_score: max_importance_score,
            artifact: None,
            episode: canonical.episode.clone(),
            historical_state: MemoryHistoricalState::Current,
            lineage: source_record_ids
                .iter()
                .map(|record_id| LineageLink {
                    record_id: record_id.clone(),
                    relation: LineageRelationKind::DerivedFrom,
                    confidence,
                })
                .collect(),
            conflict: None,
        };
        SynthesisProposal {
            proposed_record,
            source_record_ids,
            confidence,
            rationale: "records share tenant, namespace, actor, conversation, session, and source"
                .to_string(),
            metadata,
        }
    }

    fn persist_record(&self, stored: &StoredRecord) -> Result<()> {
        self.records
            .insert(stored.record.id.as_bytes(), Self::encode_record(stored)?)
            .map_err(|err| Error::Backend(format!("failed to write record: {err}")))?;
        Ok(())
    }

    fn persist_imported_record(&self, stored: &StoredRecord) -> Result<()> {
        self.persist_record(stored)?;
        if let Some(idempotency_key) = &stored.idempotency_key {
            let scoped_key = Self::idempotency_scope_key(&stored.record.scope, idempotency_key);
            self.idempotency
                .insert(scoped_key.as_bytes(), stored.record.id.as_bytes())
                .map_err(|err| Error::Backend(format!("failed to write idempotency key: {err}")))?;
        }
        Ok(())
    }

    fn retention_delete(&self, stored: StoredRecord) -> Result<()> {
        self.records
            .remove(stored.record.id.as_bytes())
            .map_err(|err| Error::Backend(format!("failed to delete expired record: {err}")))?;
        self.remove_idempotency_mapping(&stored)?;
        Ok(())
    }

    fn clear_all_records(&self) -> Result<()> {
        for stored in self.iterate_records()? {
            self.records
                .remove(stored.record.id.as_bytes())
                .map_err(|err| Error::Backend(format!("failed to clear record: {err}")))?;
            self.remove_idempotency_mapping(&stored)?;
        }
        Ok(())
    }

    fn retention_archive(&self, stored: &mut StoredRecord, now_unix_ms: u64) -> Result<bool> {
        if stored.record.quality_state == MemoryQualityState::Archived {
            return Ok(false);
        }
        stored.record.quality_state = MemoryQualityState::Archived;
        stored.record.historical_state = MemoryHistoricalState::Historical;
        stored.record.updated_at_unix_ms = now_unix_ms;
        self.persist_record(stored)?;
        Ok(true)
    }

    fn recall_from_stored_records(
        &self,
        query: RecallQuery,
        stored_records: Vec<StoredRecord>,
    ) -> Result<RecallResult> {
        let empty_query = query.query_text.trim().is_empty();
        let planner = self.config.recall_planner();
        let scorer = planner.scorer();
        let planning_profile = planner.effective_profile(&query);
        let relative_bounds = Self::relative_temporal_bounds(&stored_records, &query)?;
        let records = stored_records
            .into_iter()
            .filter(|stored| Self::record_passes_filters(&stored.record, &query, relative_bounds))
            .map(|stored| stored.record)
            .collect::<Vec<_>>();
        let mut scored = planner.plan(&records, &query);
        match query.filters.temporal_order {
            RecallTemporalOrder::Relevance if empty_query => {
                scored.sort_by(|left, right| {
                    Self::record_temporal_anchor(&right.hit.record)
                        .cmp(&Self::record_temporal_anchor(&left.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .record
                                .importance_score
                                .total_cmp(&left.hit.record.importance_score)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::Relevance => {
                scored.sort_by(|left, right| {
                    right
                        .hit
                        .breakdown
                        .total
                        .total_cmp(&left.hit.breakdown.total)
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::ChronologicalAsc => {
                scored.sort_by(|left, right| {
                    Self::record_temporal_anchor(&left.hit.record)
                        .cmp(&Self::record_temporal_anchor(&right.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .breakdown
                                .total
                                .total_cmp(&left.hit.breakdown.total)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::ChronologicalDesc => {
                scored.sort_by(|left, right| {
                    Self::record_temporal_anchor(&right.hit.record)
                        .cmp(&Self::record_temporal_anchor(&left.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .breakdown
                                .total
                                .total_cmp(&left.hit.breakdown.total)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
        }

        let examined = scored.len();
        let mut selected_ids = Vec::with_capacity(query.max_items);
        let mut remaining_budget = query.token_budget.unwrap_or(usize::MAX);
        for candidate in &scored {
            if selected_ids.len() >= query.max_items {
                break;
            }
            let estimated_tokens = Self::approximate_tokens(&candidate.hit.record);
            if selected_ids.is_empty() || estimated_tokens <= remaining_budget {
                remaining_budget = remaining_budget.saturating_sub(estimated_tokens);
                selected_ids.push(candidate.hit.record.id.clone());
            }
        }

        let trace_id = format!(
            "recall:{}:{}:{}",
            query.scope.tenant_id, query.scope.namespace, examined
        );
        let selected_set = selected_ids.iter().cloned().collect::<BTreeSet<_>>();
        let selected = scored
            .iter()
            .filter(|candidate| selected_set.contains(&candidate.hit.record.id))
            .map(|candidate| {
                let mut enriched = candidate.hit.clone();
                if query.include_explanation {
                    let selected_channels =
                        Self::selected_channels_for_hit(&candidate.hit, empty_query);
                    enriched.explanation = Some(RecallExplanation {
                        selected_channels,
                        policy_notes: vec![if empty_query {
                            "recent_scope_scan".to_string()
                        } else {
                            "initial_sled_backend_scoring".to_string()
                        }],
                        trace_id: Some(trace_id.clone()),
                        planning_trace: None,
                        planning_profile: Some(planning_profile),
                        policy_profile: Some(scorer.policy_profile()),
                        scorer_kind: Some(scorer.scorer_kind()),
                        scoring_profile: Some(scorer.scoring_profile()),
                    });
                    if let Some(explanation) = enriched.explanation.as_mut() {
                        explanation
                            .policy_notes
                            .push(scorer.profile_note().to_string());
                        explanation
                            .policy_notes
                            .push(scorer.policy_profile_note().to_string());
                        explanation
                            .policy_notes
                            .push(Self::planning_profile_note(planning_profile).to_string());
                        if let Some(note) = scorer.embedding_note() {
                            explanation.policy_notes.push(note.to_string());
                        }
                        if query.filters.episode_id.is_some() {
                            explanation
                                .policy_notes
                                .push("episode_filter_applied".to_string());
                        }
                        if query.filters.unresolved_only {
                            explanation
                                .policy_notes
                                .push("unresolved_only_filter_applied".to_string());
                        }
                        if !candidate.matched_terms.is_empty() {
                            explanation.policy_notes.push(format!(
                                "matched_terms={}",
                                candidate.matched_terms.join(",")
                            ));
                        }
                    }
                }
                enriched
            })
            .collect::<Vec<_>>();

        let mut policy_notes = vec![if empty_query {
            "recent_scope_scan".to_string()
        } else {
            "initial_sled_backend_scoring".to_string()
        }];
        policy_notes.push(scorer.profile_note().to_string());
        policy_notes.push(scorer.policy_profile_note().to_string());
        policy_notes.push(Self::planning_profile_note(planning_profile).to_string());
        if let Some(note) = scorer.embedding_note() {
            policy_notes.push(note.to_string());
        }
        if query.token_budget.is_some() {
            policy_notes.push("token_budget_applied".to_string());
        }
        if query.filters.episode_id.is_some() {
            policy_notes.push("episode_filter_applied".to_string());
        }
        if query.filters.unresolved_only {
            policy_notes.push("unresolved_only_filter_applied".to_string());
        }
        if query.filters.before_record_id.is_some() || query.filters.after_record_id.is_some() {
            policy_notes.push("relative_temporal_filter_applied".to_string());
        }
        if !query.filters.boundary_labels.is_empty() || query.filters.recurrence_key.is_some() {
            policy_notes.push("episodic_boundary_filter_applied".to_string());
        }
        if !query.filters.conflict_states.is_empty()
            || !query.filters.resolution_kinds.is_empty()
            || query.filters.unresolved_conflicts_only
        {
            policy_notes.push("conflict_review_filter_applied".to_string());
        }
        let mut selected_channels = if empty_query {
            vec!["temporal".to_string(), "policy".to_string()]
        } else {
            vec!["lexical".to_string(), "policy".to_string()]
        };
        for channel in [
            "semantic", "metadata", "episodic", "salience", "curation", "conflict",
        ] {
            let present = scored.iter().any(|candidate| match channel {
                "semantic" => candidate.hit.breakdown.semantic > 0.0,
                "metadata" => candidate.hit.breakdown.metadata > 0.0,
                "episodic" => candidate.hit.breakdown.episodic > 0.0,
                "salience" => candidate.hit.breakdown.salience > 0.0,
                "curation" => candidate.hit.breakdown.curation > 0.0,
                "conflict" => candidate.hit.record.conflict.is_some(),
                _ => false,
            });
            if present && !selected_channels.iter().any(|existing| existing == channel) {
                selected_channels.push(channel.to_string());
            }
        }

        Ok(RecallResult {
            hits: selected,
            total_candidates_examined: examined,
            explanation: query.include_explanation.then(|| RecallExplanation {
                selected_channels,
                policy_notes,
                trace_id: Some(trace_id.clone()),
                planning_profile: Some(planning_profile),
                policy_profile: Some(scorer.policy_profile()),
                planning_trace: Some(RecallPlanningTrace {
                    trace_id,
                    token_budget_applied: query.token_budget.is_some(),
                    candidates: scored
                        .into_iter()
                        .enumerate()
                        .map(|(index, candidate)| {
                            let record_id = candidate.hit.record.id.clone();
                            let selected = selected_set.contains(&record_id);
                            let selection_rank = selected_ids
                                .iter()
                                .position(|candidate_id| candidate_id == &record_id)
                                .map(|index| index as u32 + 1);
                            let candidate_channels =
                                Self::selected_channels_for_hit(&candidate.hit, empty_query);
                            let mut filter_reasons = Vec::new();
                            if selected {
                                filter_reasons.push("retained".to_string());
                            } else {
                                if index >= query.max_items {
                                    filter_reasons.push("max_items_exhausted".to_string());
                                }
                                if query.token_budget.is_some() {
                                    filter_reasons.push("token_budget_exhausted".to_string());
                                }
                            }
                            RecallTraceCandidate {
                                record_id,
                                kind: candidate.hit.record.kind,
                                selected,
                                planner_stage: candidate.planner_stage,
                                candidate_sources: candidate.candidate_sources,
                                relation_reasons: candidate.relation_reasons,
                                selection_rank,
                                matched_terms: candidate.matched_terms,
                                selected_channels: candidate_channels,
                                filter_reasons,
                                decision_reason: if selected {
                                    "selected_by_rank".to_string()
                                } else if query.token_budget.is_some() {
                                    "excluded_by_rank_or_budget".to_string()
                                } else {
                                    "excluded_by_rank".to_string()
                                },
                                breakdown: candidate.hit.breakdown,
                            }
                        })
                        .collect(),
                }),
                scorer_kind: Some(scorer.scorer_kind()),
                scoring_profile: Some(scorer.scoring_profile()),
            }),
        })
    }

    fn apply_retention_for_namespace(
        &self,
        tenant_id: &str,
        namespace: &str,
    ) -> Result<(u64, u64)> {
        let now_unix_ms = Self::now_unix_ms()?;
        let retention = &self.config.engine_config.retention;
        let ttl_window_ms = u64::from(retention.ttl_days).saturating_mul(24 * 60 * 60 * 1_000);
        let archive_window_ms =
            u64::from(retention.archive_after_days).saturating_mul(24 * 60 * 60 * 1_000);

        let mut archived_records = 0u64;
        let mut deleted_records = 0u64;
        let mut namespace_records = self
            .iterate_records()?
            .into_iter()
            .filter(|stored| {
                stored.record.scope.tenant_id == tenant_id
                    && stored.record.scope.namespace == namespace
            })
            .collect::<Vec<_>>();

        for stored in &mut namespace_records {
            if self.retention_exempt(&stored.record) {
                continue;
            }

            let expired_by_explicit_deadline = stored
                .record
                .expires_at_unix_ms
                .is_some_and(|expires_at| expires_at <= now_unix_ms);
            let expired_by_ttl = ttl_window_ms > 0
                && now_unix_ms.saturating_sub(stored.record.created_at_unix_ms) > ttl_window_ms;

            if expired_by_explicit_deadline || expired_by_ttl {
                if !matches!(stored.record.quality_state, MemoryQualityState::Deleted) {
                    self.retention_delete(stored.clone())?;
                    deleted_records += 1;
                }
                continue;
            }

            let should_archive_by_age = archive_window_ms > 0
                && now_unix_ms.saturating_sub(stored.record.created_at_unix_ms) > archive_window_ms
                && matches!(
                    stored.record.quality_state,
                    MemoryQualityState::Draft
                        | MemoryQualityState::Active
                        | MemoryQualityState::Verified
                );

            if should_archive_by_age && self.retention_archive(stored, now_unix_ms)? {
                archived_records += 1;
            }
        }

        if retention.max_records_per_namespace > 0 {
            let mut candidates = self
                .iterate_records()?
                .into_iter()
                .filter(|stored| {
                    stored.record.scope.tenant_id == tenant_id
                        && stored.record.scope.namespace == namespace
                        && !self.retention_exempt(&stored.record)
                        && matches!(
                            stored.record.quality_state,
                            MemoryQualityState::Draft
                                | MemoryQualityState::Active
                                | MemoryQualityState::Verified
                        )
                })
                .collect::<Vec<_>>();

            if candidates.len() > retention.max_records_per_namespace {
                candidates.sort_by(|left, right| {
                    left.record
                        .updated_at_unix_ms
                        .cmp(&right.record.updated_at_unix_ms)
                        .then_with(|| {
                            left.record
                                .importance_score
                                .total_cmp(&right.record.importance_score)
                        })
                        .then_with(|| left.record.id.cmp(&right.record.id))
                });

                let archive_count = candidates.len() - retention.max_records_per_namespace;
                for stored in candidates.iter_mut().take(archive_count) {
                    if self.retention_archive(stored, now_unix_ms)? {
                        archived_records += 1;
                    }
                }
            }
        }

        Ok((archived_records, deleted_records))
    }
}

#[async_trait]
impl MemoryStore for SledMemoryStore {
    fn backend_kind(&self) -> &'static str {
        "sled"
    }

    async fn upsert(&self, request: UpsertRequest) -> Result<UpsertReceipt> {
        self.validate_upsert_request(&request)?;

        if let Some(idempotency_key) = &request.idempotency_key {
            let scoped_key = Self::idempotency_scope_key(&request.record.scope, idempotency_key);
            if let Some(existing_record_id) = self
                .idempotency
                .get(scoped_key.as_bytes())
                .map_err(|err| Error::Backend(format!("failed to read idempotency key: {err}")))?
            {
                let existing_record_id =
                    String::from_utf8(existing_record_id.to_vec()).map_err(|err| {
                        Error::Backend(format!(
                            "stored idempotency mapping was not valid utf-8: {err}"
                        ))
                    })?;
                if existing_record_id != request.record.id {
                    return Err(Error::Conflict(format!(
                        "idempotency key already belongs to record {}",
                        existing_record_id
                    )));
                }
                if self.fetch_record(&existing_record_id)?.is_some() {
                    return Ok(UpsertReceipt {
                        record_id: existing_record_id,
                        deduplicated: true,
                        summary_refreshed: false,
                    });
                }
                self.idempotency
                    .remove(scoped_key.as_bytes())
                    .map_err(|err| {
                        Error::Backend(format!("failed to clear stale idempotency key: {err}"))
                    })?;
            }
        }

        let key = request.record.id.clone();
        let tenant_id = request.record.scope.tenant_id.clone();
        let namespace = request.record.scope.namespace.clone();
        let observed_at_unix_ms = request.record.updated_at_unix_ms;
        let existing = self.fetch_record(&key)?;
        let deduplicated = existing.is_some();
        let stored = StoredRecord {
            record: request.record,
            idempotency_key: request.idempotency_key,
        };
        if let Some(existing) = existing
            && existing.idempotency_key != stored.idempotency_key
        {
            self.remove_idempotency_mapping(&existing)?;
        }
        self.persist_record(&stored)?;
        if let Some(idempotency_key) = &stored.idempotency_key {
            let scoped_key = Self::idempotency_scope_key(&stored.record.scope, idempotency_key);
            self.idempotency
                .insert(scoped_key.as_bytes(), key.as_bytes())
                .map_err(|err| Error::Backend(format!("failed to write idempotency key: {err}")))?;
        }
        self.persist_record_version(&key, observed_at_unix_ms, Some(&stored))?;
        self.append_changefeed_event(
            ChangefeedEventKind::Upserted,
            Some(&stored.record),
            &tenant_id,
            &namespace,
            Some(&key),
            Some(if deduplicated {
                "record replaced".to_string()
            } else {
                "record inserted".to_string()
            }),
            observed_at_unix_ms,
        )?;
        self.apply_retention_for_namespace(&tenant_id, &namespace)?;
        self.db
            .flush_async()
            .await
            .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        Ok(UpsertReceipt {
            record_id: key,
            deduplicated,
            summary_refreshed: false,
        })
    }

    async fn batch_upsert(&self, request: BatchUpsertRequest) -> Result<Vec<UpsertReceipt>> {
        if request.requests.len() > self.config.engine_config.max_batch_size {
            return Err(Error::InvalidRequest(format!(
                "batch size {} exceeds configured max_batch_size {}",
                request.requests.len(),
                self.config.engine_config.max_batch_size
            )));
        }
        for item in &request.requests {
            self.validate_upsert_request(item)?;
        }
        let mut receipts = Vec::with_capacity(request.requests.len());
        for item in request.requests {
            receipts.push(self.upsert(item).await?);
        }
        Ok(receipts)
    }

    async fn recall(&self, query: RecallQuery) -> Result<RecallResult> {
        self.recall_from_stored_records(query, self.iterate_records()?)
    }

    async fn recall_as_of(&self, request: TimeTravelRecallRequest) -> Result<RecallResult> {
        self.recall_from_stored_records(request.query, self.records_as_of(request.as_of_unix_ms)?)
    }

    async fn compact(&self, request: CompactionRequest) -> Result<CompactionReport> {
        if request.tenant_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "compaction tenant_id is required".to_string(),
            ));
        }

        let records = self.iterate_records()?;
        let mut groups: HashMap<String, Vec<StoredRecord>> = HashMap::new();
        for stored in records {
            if stored.record.scope.tenant_id != request.tenant_id {
                continue;
            }
            if let Some(namespace) = &request.namespace
                && stored.record.scope.namespace != *namespace
            {
                continue;
            }
            if matches!(
                stored.record.quality_state,
                MemoryQualityState::Archived
                    | MemoryQualityState::Deleted
                    | MemoryQualityState::Suppressed
            ) {
                continue;
            }
            groups
                .entry(Self::dedup_signature(&stored.record))
                .or_default()
                .push(stored);
        }

        let mut deduplicated_records = 0u64;
        let mut archived_records = 0u64;
        let mut summarized_clusters = 0u64;
        let mut superseded_records = 0u64;
        let mut lineage_links_created = 0u64;
        let now_unix_ms = Self::now_unix_ms()?;
        for group in groups.values_mut() {
            if group.len() < 2 {
                continue;
            }
            group.sort_by(|left, right| {
                right
                    .record
                    .updated_at_unix_ms
                    .cmp(&left.record.updated_at_unix_ms)
                    .then_with(|| {
                        right
                            .record
                            .importance_score
                            .total_cmp(&left.record.importance_score)
                    })
                    .then_with(|| left.record.id.cmp(&right.record.id))
            });

            let signature = Self::dedup_signature(&group[0].record);
            if self
                .config
                .engine_config
                .compaction
                .summarize_after_record_count
                > 0
                && group.len()
                    >= self
                        .config
                        .engine_config
                        .compaction
                        .summarize_after_record_count
            {
                summarized_clusters += 1;
                lineage_links_created += group.len() as u64;
                if !request.dry_run {
                    let summary = Self::compaction_summary_record(group, &signature, now_unix_ms);
                    self.persist_record(&summary)?;
                }
            }

            for duplicate in group.iter().skip(1) {
                deduplicated_records += 1;
                archived_records += 1;
                superseded_records += 1;
                if request.dry_run {
                    continue;
                }
                let mut archived = duplicate.clone();
                archived.record.quality_state = MemoryQualityState::Archived;
                archived.record.historical_state = MemoryHistoricalState::Superseded;
                archived.record.lineage.push(LineageLink {
                    record_id: Self::summary_record_id(&signature),
                    relation: LineageRelationKind::SupersededBy,
                    confidence: 1.0,
                });
                lineage_links_created += 1;
                archived.record.updated_at_unix_ms = Self::now_unix_ms()?;
                self.records
                    .insert(
                        archived.record.id.as_bytes(),
                        Self::encode_record(&archived)?,
                    )
                    .map_err(|err| {
                        Error::Backend(format!("failed to archive duplicate record: {err}"))
                    })?;
            }
        }

        for candidate in self.cold_tiering_candidates(
            &request.tenant_id,
            request.namespace.as_deref(),
            now_unix_ms,
        )? {
            archived_records += 1;
            if request.dry_run {
                continue;
            }
            let mut archived = candidate;
            archived.record.quality_state = MemoryQualityState::Archived;
            archived.record.historical_state = MemoryHistoricalState::Historical;
            archived.record.updated_at_unix_ms = now_unix_ms;
            self.persist_record(&archived)?;
        }

        if !request.dry_run {
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(CompactionReport {
            deduplicated_records,
            archived_records,
            summarized_clusters,
            pruned_graph_edges: 0,
            superseded_records,
            lineage_links_created,
            dry_run: request.dry_run,
        })
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<SynthesisReport> {
        if request.tenant_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "synthesis tenant_id is required".to_string(),
            ));
        }
        if request.min_source_records == 0 {
            return Err(Error::InvalidRequest(
                "synthesis min_source_records must be greater than zero".to_string(),
            ));
        }
        if request.max_source_records < request.min_source_records {
            return Err(Error::InvalidRequest(
                "synthesis max_source_records must be greater than or equal to min_source_records"
                    .to_string(),
            ));
        }

        let mut scanned_records = 0u64;
        let mut groups: HashMap<String, Vec<StoredRecord>> = HashMap::new();
        for stored in self.iterate_records()? {
            scanned_records += 1;
            let record = &stored.record;
            if record.scope.tenant_id != request.tenant_id {
                continue;
            }
            if let Some(namespace) = &request.namespace
                && record.scope.namespace != *namespace
            {
                continue;
            }
            if let Some(actor_id) = &request.actor_id
                && record.scope.actor_id != *actor_id
            {
                continue;
            }
            if let Some(conversation_id) = &request.conversation_id
                && record.scope.conversation_id.as_deref() != Some(conversation_id.as_str())
            {
                continue;
            }
            if let Some(session_id) = &request.session_id
                && record.scope.session_id.as_deref() != Some(session_id.as_str())
            {
                continue;
            }
            if request
                .from_unix_ms
                .is_some_and(|from| record.updated_at_unix_ms < from)
                || request
                    .to_unix_ms
                    .is_some_and(|to| record.updated_at_unix_ms > to)
            {
                continue;
            }
            if record.kind == MemoryRecordKind::Summary
                || matches!(
                    record.quality_state,
                    MemoryQualityState::Archived
                        | MemoryQualityState::Deleted
                        | MemoryQualityState::Suppressed
                )
            {
                continue;
            }
            groups
                .entry(Self::synthesis_group_key(record))
                .or_default()
                .push(stored);
        }

        let now_unix_ms = Self::now_unix_ms()?;
        let mut eligible_records = 0u64;
        let mut proposals = Vec::new();
        for group in groups.values_mut() {
            if group.len() < request.min_source_records {
                continue;
            }
            group.sort_by(|left, right| {
                right
                    .record
                    .updated_at_unix_ms
                    .cmp(&left.record.updated_at_unix_ms)
                    .then_with(|| {
                        right
                            .record
                            .importance_score
                            .total_cmp(&left.record.importance_score)
                    })
                    .then_with(|| left.record.id.cmp(&right.record.id))
            });
            group.truncate(request.max_source_records);
            eligible_records += group.len() as u64;
            proposals.push(Self::synthesis_proposal_from_group(
                group,
                &request.reason,
                now_unix_ms,
            ));
        }
        proposals.sort_by(|left, right| {
            right
                .proposed_record
                .importance_score
                .total_cmp(&left.proposed_record.importance_score)
                .then_with(|| left.proposed_record.id.cmp(&right.proposed_record.id))
        });
        if request.max_proposals > 0 {
            proposals.truncate(request.max_proposals);
        }

        let mut persisted_records = 0u64;
        let lineage_links_created = proposals
            .iter()
            .map(|proposal| proposal.source_record_ids.len() as u64)
            .sum();
        if !request.dry_run {
            for proposal in &proposals {
                let stored = StoredRecord {
                    record: proposal.proposed_record.clone(),
                    idempotency_key: None,
                };
                self.persist_record(&stored)?;
                self.persist_record_version(
                    &stored.record.id,
                    stored.record.updated_at_unix_ms,
                    Some(&stored),
                )?;
                self.append_changefeed_event(
                    ChangefeedEventKind::Upserted,
                    Some(&stored.record),
                    &stored.record.scope.tenant_id,
                    &stored.record.scope.namespace,
                    Some(&stored.record.id),
                    Some("synthesis proposal persisted".to_string()),
                    stored.record.updated_at_unix_ms,
                )?;
                persisted_records += 1;
            }
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(SynthesisReport {
            dry_run: request.dry_run,
            scanned_records,
            eligible_records,
            proposed_records: proposals.len() as u64,
            persisted_records,
            lineage_links_created,
            proposals,
        })
    }

    async fn delete(&self, request: DeleteRequest) -> Result<DeleteReceipt> {
        Self::validate_delete_request(&request)?;

        let Some(stored) = self.fetch_record(&request.record_id)? else {
            return Ok(DeleteReceipt {
                record_id: request.record_id,
                tombstoned: false,
                hard_deleted: false,
            });
        };

        if stored.record.scope.tenant_id != request.tenant_id {
            return Err(Error::InvalidRequest(format!(
                "record {} does not belong to tenant {}",
                request.record_id, request.tenant_id
            )));
        }
        if stored.record.scope.namespace != request.namespace {
            return Err(Error::InvalidRequest(format!(
                "record {} does not belong to namespace {}",
                request.record_id, request.namespace
            )));
        }

        if request.hard_delete {
            let now_unix_ms = Self::now_unix_ms()?;
            self.records
                .remove(request.record_id.as_bytes())
                .map_err(|err| Error::Backend(format!("failed to delete record: {err}")))?;
            self.remove_idempotency_mapping(&stored)?;
            self.persist_record_version(&request.record_id, now_unix_ms, None)?;
            self.append_changefeed_event(
                ChangefeedEventKind::Deleted,
                None,
                &request.tenant_id,
                &request.namespace,
                Some(&request.record_id),
                Some("record hard deleted".to_string()),
                now_unix_ms,
            )?;
        } else {
            let mut tombstone = stored;
            tombstone.record.quality_state = MemoryQualityState::Deleted;
            tombstone.record.updated_at_unix_ms = Self::now_unix_ms()?;
            self.records
                .insert(
                    tombstone.record.id.as_bytes(),
                    Self::encode_record(&tombstone)?,
                )
                .map_err(|err| Error::Backend(format!("failed to write tombstone: {err}")))?;
            self.persist_record_version(
                &request.record_id,
                tombstone.record.updated_at_unix_ms,
                Some(&tombstone),
            )?;
            self.append_changefeed_event(
                ChangefeedEventKind::Deleted,
                Some(&tombstone.record),
                &request.tenant_id,
                &request.namespace,
                Some(&request.record_id),
                Some("record tombstoned".to_string()),
                tombstone.record.updated_at_unix_ms,
            )?;
        }

        self.db
            .flush_async()
            .await
            .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        Ok(DeleteReceipt {
            record_id: request.record_id,
            tombstoned: !request.hard_delete,
            hard_deleted: request.hard_delete,
        })
    }

    async fn archive(&self, request: ArchiveRequest) -> Result<ArchiveReceipt> {
        Self::validate_archive_request(&request)?;

        let Some(mut stored) = self.fetch_record(&request.record_id)? else {
            return Err(Error::InvalidRequest(format!(
                "record {} was not found",
                request.record_id
            )));
        };
        Self::validate_record_scope(&stored, &request.tenant_id, &request.namespace)?;

        let previous_quality_state = stored.record.quality_state;
        let previous_historical_state = stored.record.historical_state;
        let changed = previous_quality_state != MemoryQualityState::Archived
            || previous_historical_state == MemoryHistoricalState::Current;
        let historical_state = match previous_historical_state {
            MemoryHistoricalState::Current => MemoryHistoricalState::Historical,
            other => other,
        };

        if changed && !request.dry_run {
            stored.record.quality_state = MemoryQualityState::Archived;
            stored.record.historical_state = historical_state;
            stored.record.updated_at_unix_ms = Self::now_unix_ms()?;
            self.records
                .insert(stored.record.id.as_bytes(), Self::encode_record(&stored)?)
                .map_err(|err| Error::Backend(format!("failed to archive record: {err}")))?;
            self.persist_record_version(
                &request.record_id,
                stored.record.updated_at_unix_ms,
                Some(&stored),
            )?;
            self.append_changefeed_event(
                ChangefeedEventKind::Archived,
                Some(&stored.record),
                &request.tenant_id,
                &request.namespace,
                Some(&request.record_id),
                Some("record archived".to_string()),
                stored.record.updated_at_unix_ms,
            )?;
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(ArchiveReceipt {
            record_id: request.record_id,
            previous_quality_state,
            previous_historical_state,
            quality_state: MemoryQualityState::Archived,
            historical_state,
            changed,
            dry_run: request.dry_run,
        })
    }

    async fn suppress(&self, request: SuppressRequest) -> Result<SuppressReceipt> {
        Self::validate_suppress_request(&request)?;

        let Some(mut stored) = self.fetch_record(&request.record_id)? else {
            return Err(Error::InvalidRequest(format!(
                "record {} was not found",
                request.record_id
            )));
        };
        Self::validate_record_scope(&stored, &request.tenant_id, &request.namespace)?;

        let previous_quality_state = stored.record.quality_state;
        let previous_historical_state = stored.record.historical_state;
        let changed = previous_quality_state != MemoryQualityState::Suppressed;

        if changed && !request.dry_run {
            stored.record.quality_state = MemoryQualityState::Suppressed;
            stored.record.updated_at_unix_ms = Self::now_unix_ms()?;
            self.records
                .insert(stored.record.id.as_bytes(), Self::encode_record(&stored)?)
                .map_err(|err| Error::Backend(format!("failed to suppress record: {err}")))?;
            self.persist_record_version(
                &request.record_id,
                stored.record.updated_at_unix_ms,
                Some(&stored),
            )?;
            self.append_changefeed_event(
                ChangefeedEventKind::Suppressed,
                Some(&stored.record),
                &request.tenant_id,
                &request.namespace,
                Some(&request.record_id),
                Some("record suppressed".to_string()),
                stored.record.updated_at_unix_ms,
            )?;
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(SuppressReceipt {
            record_id: request.record_id,
            previous_quality_state,
            previous_historical_state,
            quality_state: MemoryQualityState::Suppressed,
            historical_state: previous_historical_state,
            changed,
            dry_run: request.dry_run,
        })
    }

    async fn recover(&self, request: RecoverRequest) -> Result<RecoverReceipt> {
        Self::validate_recover_request(&request)?;

        let Some(mut stored) = self.fetch_record(&request.record_id)? else {
            return Err(Error::InvalidRequest(format!(
                "record {} was not found",
                request.record_id
            )));
        };
        Self::validate_record_scope(&stored, &request.tenant_id, &request.namespace)?;

        let previous_quality_state = stored.record.quality_state;
        let previous_historical_state = stored.record.historical_state;
        let historical_state = request
            .historical_state
            .unwrap_or(MemoryHistoricalState::Current);
        let changed = previous_quality_state != request.quality_state
            || previous_historical_state != historical_state;

        if changed && !request.dry_run {
            stored.record.quality_state = request.quality_state;
            stored.record.historical_state = historical_state;
            stored.record.updated_at_unix_ms = Self::now_unix_ms()?;
            self.records
                .insert(stored.record.id.as_bytes(), Self::encode_record(&stored)?)
                .map_err(|err| Error::Backend(format!("failed to recover record: {err}")))?;
            self.persist_record_version(
                &request.record_id,
                stored.record.updated_at_unix_ms,
                Some(&stored),
            )?;
            self.append_changefeed_event(
                ChangefeedEventKind::Recovered,
                Some(&stored.record),
                &request.tenant_id,
                &request.namespace,
                Some(&request.record_id),
                Some("record recovered".to_string()),
                stored.record.updated_at_unix_ms,
            )?;
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(RecoverReceipt {
            record_id: request.record_id,
            previous_quality_state,
            previous_historical_state,
            quality_state: request.quality_state,
            historical_state,
            changed,
            dry_run: request.dry_run,
        })
    }

    async fn snapshot(&self) -> Result<SnapshotManifest> {
        let records = self.iterate_records()?;
        let namespaces = records
            .iter()
            .map(|stored| stored.record.scope.namespace.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let created_at_unix_ms = Self::now_unix_ms()?;
        let storage_bytes = self
            .db
            .size_on_disk()
            .map_err(|err| Error::Backend(format!("failed to determine db size: {err}")))?;

        Ok(SnapshotManifest {
            snapshot_id: format!("snapshot-{created_at_unix_ms}"),
            created_at_unix_ms,
            namespaces,
            record_count: records.len() as u64,
            storage_bytes,
            engine: self.config.engine_config.tuning_info(),
        })
    }

    async fn stats(&self, request: StoreStatsRequest) -> Result<StoreStatsReport> {
        self.build_stats_report(&request)
    }

    async fn inspect_graph(
        &self,
        request: GraphInspectionRequest,
    ) -> Result<GraphInspectionReport> {
        let records = self
            .iterate_records()?
            .into_iter()
            .map(|stored| stored.record)
            .collect::<Vec<_>>();
        Ok(build_graph_inspection_report(
            &records,
            &request,
            Self::now_unix_ms()?,
        ))
    }

    async fn changefeed(&self, request: ChangefeedRequest) -> Result<ChangefeedReport> {
        let limit = request.limit.unwrap_or(100).max(1);
        let after_sequence = request.after_sequence.unwrap_or(0);
        let mut events = Vec::new();
        let mut truncated = false;

        for item in self.changefeed.iter() {
            let (_, value) =
                item.map_err(|err| Error::Backend(format!("sled iteration failed: {err}")))?;
            let event = serde_json::from_slice::<ChangefeedEvent>(&value).map_err(|err| {
                Error::Backend(format!("failed to decode changefeed event: {err}"))
            })?;
            if event.sequence <= after_sequence
                || request
                    .tenant_id
                    .as_ref()
                    .is_some_and(|tenant_id| &event.tenant_id != tenant_id)
                || request
                    .namespace
                    .as_ref()
                    .is_some_and(|namespace| &event.namespace != namespace)
            {
                continue;
            }
            events.push(event);
        }

        events.sort_by_key(|event| event.sequence);
        if events.len() > limit {
            events.truncate(limit);
            truncated = true;
        }
        let last_sequence = events.last().map(|event| event.sequence);
        Ok(ChangefeedReport {
            events,
            last_sequence,
            truncated,
        })
    }

    async fn integrity_check(
        &self,
        request: IntegrityCheckRequest,
    ) -> Result<IntegrityCheckReport> {
        let summary = self
            .build_integrity_summary(request.tenant_id.as_deref(), request.namespace.as_deref())?;
        Ok(IntegrityCheckReport {
            generated_at_unix_ms: Self::now_unix_ms()?,
            healthy: summary.stale_idempotency_keys == 0
                && summary.missing_idempotency_keys == 0
                && summary.duplicate_active_records == 0,
            scanned_records: summary.scanned_records,
            scanned_idempotency_keys: summary.scanned_idempotency_keys,
            stale_idempotency_keys: summary.stale_idempotency_keys,
            missing_idempotency_keys: summary.missing_idempotency_keys,
            duplicate_active_records: summary.duplicate_active_records,
        })
    }

    async fn repair(&self, request: RepairRequest) -> Result<RepairReport> {
        if request.reason.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "repair reason is required".to_string(),
            ));
        }
        if !request.remove_stale_idempotency_keys && !request.rebuild_missing_idempotency_keys {
            return Err(Error::InvalidRequest(
                "repair requires at least one enabled action".to_string(),
            ));
        }

        let tenant_filter = request.tenant_id.as_deref();
        let namespace_filter = request.namespace.as_deref();
        let summary = self.build_integrity_summary(tenant_filter, namespace_filter)?;
        let records = self.iterate_records()?;
        let mappings = self.iterate_idempotency_mappings()?;
        let mut removed_stale_idempotency_keys = 0u64;
        let mut rebuilt_missing_idempotency_keys = 0u64;

        if request.remove_stale_idempotency_keys {
            for mapping in &mappings {
                let Some((tenant_id, namespace, _, _, _, idempotency_key)) =
                    Self::parse_scope_key(&mapping.scoped_key)
                else {
                    continue;
                };
                if !Self::scope_matches_filters(
                    &tenant_id,
                    &namespace,
                    tenant_filter,
                    namespace_filter,
                ) {
                    continue;
                }
                let stale = match self.fetch_record(&mapping.record_id)? {
                    Some(stored) => {
                        stored.record.scope.tenant_id != tenant_id
                            || stored.record.scope.namespace != namespace
                            || stored.idempotency_key.as_deref() != Some(idempotency_key.as_str())
                            || Self::idempotency_scope_key(&stored.record.scope, &idempotency_key)
                                != mapping.scoped_key
                    }
                    None => true,
                };
                if stale {
                    removed_stale_idempotency_keys += 1;
                    if !request.dry_run {
                        self.idempotency
                            .remove(mapping.scoped_key.as_bytes())
                            .map_err(|err| {
                                Error::Backend(format!(
                                    "failed to remove stale idempotency key: {err}"
                                ))
                            })?;
                    }
                }
            }
        }

        if request.rebuild_missing_idempotency_keys {
            let existing = self.iterate_idempotency_mappings()?;
            let existing_lookup = existing
                .into_iter()
                .map(|mapping| (mapping.scoped_key, mapping.record_id))
                .collect::<HashMap<_, _>>();

            for stored in &records {
                if !Self::scope_matches_filters(
                    &stored.record.scope.tenant_id,
                    &stored.record.scope.namespace,
                    tenant_filter,
                    namespace_filter,
                ) {
                    continue;
                }
                let Some(idempotency_key) = &stored.idempotency_key else {
                    continue;
                };
                let scoped_key = Self::idempotency_scope_key(&stored.record.scope, idempotency_key);
                if existing_lookup.get(&scoped_key) == Some(&stored.record.id) {
                    continue;
                }
                rebuilt_missing_idempotency_keys += 1;
                if !request.dry_run {
                    self.idempotency
                        .insert(scoped_key.as_bytes(), stored.record.id.as_bytes())
                        .map_err(|err| {
                            Error::Backend(format!("failed to rebuild idempotency key: {err}"))
                        })?;
                }
            }
        }

        if !request.dry_run {
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        let stale_after = if request.remove_stale_idempotency_keys {
            0
        } else {
            summary.stale_idempotency_keys
        };
        let missing_after = if request.rebuild_missing_idempotency_keys {
            0
        } else {
            summary.missing_idempotency_keys
        };

        Ok(RepairReport {
            dry_run: request.dry_run,
            scanned_records: summary.scanned_records,
            scanned_idempotency_keys: summary.scanned_idempotency_keys,
            removed_stale_idempotency_keys,
            rebuilt_missing_idempotency_keys,
            healthy_after: stale_after == 0
                && missing_after == 0
                && summary.duplicate_active_records == 0,
        })
    }

    async fn export(&self, request: ExportRequest) -> Result<PortableStorePackage> {
        let exported_at_unix_ms = Self::now_unix_ms()?;
        let mut namespaces = BTreeSet::new();
        let mut records = Vec::new();
        for stored in self.iterate_records()? {
            if request
                .tenant_id
                .as_deref()
                .is_some_and(|tenant_id| stored.record.scope.tenant_id != tenant_id)
            {
                continue;
            }
            if request
                .namespace
                .as_deref()
                .is_some_and(|namespace| stored.record.scope.namespace != namespace)
            {
                continue;
            }
            if !request.include_archived
                && stored.record.quality_state == MemoryQualityState::Archived
            {
                continue;
            }
            namespaces.insert(format!(
                "{}:{}",
                stored.record.scope.tenant_id, stored.record.scope.namespace
            ));
            records.push(PortableRecord {
                record: stored.record,
                idempotency_key: stored.idempotency_key,
            });
        }

        let storage_bytes = records
            .iter()
            .map(|entry| {
                entry.record.content.len()
                    + entry.record.summary.as_deref().map(str::len).unwrap_or(0)
            })
            .sum::<usize>() as u64;

        Ok(PortableStorePackage {
            package_version: PORTABLE_PACKAGE_VERSION,
            exported_at_unix_ms,
            manifest: SnapshotManifest {
                snapshot_id: format!("portable-export-{exported_at_unix_ms}"),
                created_at_unix_ms: exported_at_unix_ms,
                namespaces: namespaces.into_iter().collect(),
                record_count: records.len() as u64,
                storage_bytes,
                engine: self.config.engine_config.tuning_info(),
            },
            records,
        })
    }

    async fn import(&self, request: ImportRequest) -> Result<ImportReport> {
        let snapshot_id = request.package.manifest.snapshot_id.clone();
        let package_version = request.package.package_version;
        let (validated_records, compatible_package, failed_records, entries) =
            self.validate_import_request(&request);
        let apply_changes = compatible_package
            && failed_records.is_empty()
            && !request.dry_run
            && !matches!(request.mode, ImportMode::Validate);
        let mut imported_records = 0u64;
        let mut skipped_records = 0u64;
        if apply_changes && matches!(request.mode, ImportMode::Replace) {
            self.clear_all_records()?;
        }

        for entry in entries {
            if matches!(request.mode, ImportMode::Merge)
                && self
                    .records
                    .contains_key(entry.record.id.as_bytes())
                    .map_err(|err| {
                        Error::Backend(format!("failed to check record presence: {err}"))
                    })?
            {
                skipped_records += 1;
                continue;
            }
            if apply_changes {
                self.persist_imported_record(&StoredRecord {
                    record: entry.record,
                    idempotency_key: entry.idempotency_key,
                })?;
            }
            imported_records += 1;
        }
        if apply_changes {
            self.db
                .flush_async()
                .await
                .map_err(|err| Error::Backend(format!("failed to flush sled db: {err}")))?;
        }

        Ok(ImportReport {
            mode: request.mode,
            dry_run: request.dry_run,
            applied: apply_changes,
            compatible_package,
            package_version,
            validated_records,
            imported_records,
            skipped_records,
            replaced_existing: matches!(request.mode, ImportMode::Replace),
            snapshot_id,
            failed_records,
        })
    }
}
