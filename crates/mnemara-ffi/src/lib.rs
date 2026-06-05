use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr;

use mnemara_core::{
    ExportRequest, ImportRequest, MaintenanceRunRequest, MemoryStore, RecallQuery, UpsertRequest,
};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::runtime::{Builder, Runtime};

pub struct MnemaraFfiStore {
    store: SledMemoryStore,
    runtime: Runtime,
}

#[repr(C)]
pub struct MnemaraFfiResult {
    pub ok: bool,
    pub data: *mut c_char,
    pub error: *mut c_char,
}

fn into_c_string(value: String) -> *mut c_char {
    let sanitized = value.replace('\0', "\\u0000");
    CString::new(sanitized)
        .expect("interior nul bytes were escaped")
        .into_raw()
}

fn success<T: Serialize>(value: &T) -> MnemaraFfiResult {
    match serde_json::to_string(value) {
        Ok(data) => MnemaraFfiResult {
            ok: true,
            data: into_c_string(data),
            error: ptr::null_mut(),
        },
        Err(err) => failure(format!("failed to encode result: {err}")),
    }
}

fn failure(error: String) -> MnemaraFfiResult {
    MnemaraFfiResult {
        ok: false,
        data: ptr::null_mut(),
        error: into_c_string(error),
    }
}

unsafe fn ptr_to_str<'a>(ptr: *const c_char, name: &str) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err(format!("{name} pointer is null"));
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|err| format!("{name} must be valid UTF-8: {err}"))
}

fn parse_request<T: DeserializeOwned>(json: *const c_char) -> Result<T, String> {
    let raw = unsafe { ptr_to_str(json, "json") }?;
    serde_json::from_str(raw).map_err(|err| format!("invalid JSON request: {err}"))
}

fn with_store<T, F>(store: *mut MnemaraFfiStore, operation: F) -> MnemaraFfiResult
where
    T: Serialize,
    F: FnOnce(&mut MnemaraFfiStore) -> Result<T, String>,
{
    let result = catch_unwind(AssertUnwindSafe(|| {
        if store.is_null() {
            return Err("store pointer is null".to_string());
        }
        operation(unsafe { &mut *store })
    }));
    match result {
        Ok(Ok(value)) => success(&value),
        Ok(Err(err)) => failure(err),
        Err(_) => failure("Mnemara FFI operation panicked".to_string()),
    }
}

#[unsafe(no_mangle)]
/// Open a sled-backed Mnemara store.
///
/// # Safety
///
/// `path` must be a non-null pointer to a valid NUL-terminated UTF-8 string.
/// The returned pointer must be closed exactly once with `mnemara_ffi_close`.
pub unsafe extern "C" fn mnemara_ffi_open_sled(path: *const c_char) -> *mut MnemaraFfiStore {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path = unsafe { ptr_to_str(path, "path") }?;
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| format!("failed to create Tokio runtime: {err}"))?;
        let store = SledMemoryStore::open(SledStoreConfig::new(PathBuf::from(path)))
            .map_err(|err| format!("failed to open sled store: {err}"))?;
        Ok::<_, String>(Box::into_raw(Box::new(MnemaraFfiStore { store, runtime })))
    }));
    match result {
        Ok(Ok(store)) => store,
        Ok(Err(_)) | Err(_) => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
/// Close a store handle returned by `mnemara_ffi_open_sled`.
///
/// # Safety
///
/// `store` must be null or a pointer returned by `mnemara_ffi_open_sled` that
/// has not already been closed.
pub unsafe extern "C" fn mnemara_ffi_close(store: *mut MnemaraFfiStore) {
    if !store.is_null() {
        unsafe {
            drop(Box::from_raw(store));
        }
    }
}

#[unsafe(no_mangle)]
/// Free a string returned in `MnemaraFfiResult.data` or `.error`.
///
/// # Safety
///
/// `value` must be null or a pointer returned by this crate through
/// `CString::into_raw`, and it must not have been freed before.
pub unsafe extern "C" fn mnemara_ffi_free_string(value: *mut c_char) {
    if !value.is_null() {
        unsafe {
            drop(CString::from_raw(value));
        }
    }
}

#[unsafe(no_mangle)]
/// Upsert a memory record using a JSON `UpsertRequest`.
///
/// # Safety
///
/// `store` must be a valid open store handle. `json` must be a non-null pointer
/// to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn mnemara_ffi_upsert_json(
    store: *mut MnemaraFfiStore,
    json: *const c_char,
) -> MnemaraFfiResult {
    with_store(store, |handle| {
        let request = parse_request::<UpsertRequest>(json)?;
        handle
            .runtime
            .block_on(handle.store.upsert(request))
            .map_err(|err| err.to_string())
    })
}

#[unsafe(no_mangle)]
/// Recall memory records using a JSON `RecallQuery`.
///
/// # Safety
///
/// `store` must be a valid open store handle. `json` must be a non-null pointer
/// to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn mnemara_ffi_recall_json(
    store: *mut MnemaraFfiStore,
    json: *const c_char,
) -> MnemaraFfiResult {
    with_store(store, |handle| {
        let request = parse_request::<RecallQuery>(json)?;
        handle
            .runtime
            .block_on(handle.store.recall(request))
            .map_err(|err| err.to_string())
    })
}

#[unsafe(no_mangle)]
/// Export a portable package using a JSON `ExportRequest`.
///
/// # Safety
///
/// `store` must be a valid open store handle. `json` must be a non-null pointer
/// to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn mnemara_ffi_export_json(
    store: *mut MnemaraFfiStore,
    json: *const c_char,
) -> MnemaraFfiResult {
    with_store(store, |handle| {
        let request = parse_request::<ExportRequest>(json)?;
        handle
            .runtime
            .block_on(handle.store.export(request))
            .map_err(|err| err.to_string())
    })
}

#[unsafe(no_mangle)]
/// Import a portable package using a JSON `ImportRequest`.
///
/// # Safety
///
/// `store` must be a valid open store handle. `json` must be a non-null pointer
/// to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn mnemara_ffi_import_json(
    store: *mut MnemaraFfiStore,
    json: *const c_char,
) -> MnemaraFfiResult {
    with_store(store, |handle| {
        let request = parse_request::<ImportRequest>(json)?;
        handle
            .runtime
            .block_on(handle.store.import(request))
            .map_err(|err| err.to_string())
    })
}

#[unsafe(no_mangle)]
/// Run integrity, repair, compaction, and opt-in synthesis phases using a JSON maintenance request.
///
/// # Safety
///
/// `store` must be a valid open store handle. `json` must be a non-null pointer
/// to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn mnemara_ffi_run_maintenance_json(
    store: *mut MnemaraFfiStore,
    json: *const c_char,
) -> MnemaraFfiResult {
    with_store(store, |handle| {
        let request = parse_request::<MaintenanceRunRequest>(json)?;
        handle
            .runtime
            .block_on(handle.store.run_maintenance(request))
            .map_err(|err| err.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn cstring(value: serde_json::Value) -> CString {
        CString::new(value.to_string()).unwrap()
    }

    unsafe fn result_string(result: MnemaraFfiResult) -> String {
        assert!(result.ok, "unexpected ffi error");
        let data = unsafe { CStr::from_ptr(result.data) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe {
            mnemara_ffi_free_string(result.data);
        }
        data
    }

    #[test]
    fn ffi_upsert_recall_and_maintenance_round_trip() {
        let path = std::env::temp_dir().join(format!("mnemara-ffi-{}", Uuid::new_v4()));
        let path = CString::new(path.to_string_lossy().to_string()).unwrap();
        let store = unsafe { mnemara_ffi_open_sled(path.as_ptr()) };
        assert!(!store.is_null());

        let scope = json!({
            "tenant_id": "ffi",
            "namespace": "default",
            "actor_id": "tester",
            "conversation_id": null,
            "session_id": null,
            "source": "ffi-test",
            "labels": [],
            "trust_level": "Observed"
        });
        let upsert = cstring(json!({
            "record": {
                "id": "ffi-record",
                "scope": scope,
                "kind": "Fact",
                "content": "FFI clients can upsert and recall Mnemara records.",
                "summary": null,
                "source_id": null,
                "metadata": {},
                "quality_state": "Active",
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 1,
                "expires_at_unix_ms": null,
                "importance_score": 0.7,
                "artifact": null,
                "episode": null,
                "historical_state": "Current",
                "lineage": [],
                "conflict": null
            },
            "idempotency_key": "ffi-record"
        }));
        let upsert_result = unsafe { mnemara_ffi_upsert_json(store, upsert.as_ptr()) };
        let upsert_json: serde_json::Value =
            serde_json::from_str(&unsafe { result_string(upsert_result) }).unwrap();
        assert_eq!(upsert_json["record_id"], "ffi-record");

        let recall = cstring(json!({
            "scope": scope,
            "query_text": "FFI recall",
            "max_items": 3,
            "token_budget": null,
            "filters": {
                "kinds": [],
                "required_labels": [],
                "source": null,
                "from_unix_ms": null,
                "to_unix_ms": null,
                "min_importance_score": null,
                "trust_levels": [],
                "states": [],
                "include_archived": false,
                "episode_id": null,
                "continuity_states": [],
                "unresolved_only": false,
                "temporal_order": "Relevance",
                "historical_mode": "CurrentOnly",
                "lineage_record_id": null,
                "before_record_id": null,
                "after_record_id": null,
                "boundary_labels": [],
                "recurrence_key": null,
                "conflict_states": [],
                "resolution_kinds": [],
                "unresolved_conflicts_only": false
            },
            "include_explanation": true
        }));
        let recall_result = unsafe { mnemara_ffi_recall_json(store, recall.as_ptr()) };
        let recall_json: serde_json::Value =
            serde_json::from_str(&unsafe { result_string(recall_result) }).unwrap();
        assert_eq!(recall_json["hits"].as_array().unwrap().len(), 1);

        let maintenance = cstring(json!({
            "tenant_id": "ffi",
            "dry_run": true,
            "run_integrity_check": true,
            "run_repair": true,
            "run_compaction": true
        }));
        let maintenance_result =
            unsafe { mnemara_ffi_run_maintenance_json(store, maintenance.as_ptr()) };
        let maintenance_json: serde_json::Value =
            serde_json::from_str(&unsafe { result_string(maintenance_result) }).unwrap();
        assert_eq!(maintenance_json["dry_run"], true);

        unsafe {
            mnemara_ffi_close(store);
        }
    }
}
