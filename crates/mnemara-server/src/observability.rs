use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mnemara_core::{OperationTrace, TraceListRequest};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceRegistrySnapshot {
    pub stored_traces: u64,
    pub trace_capacity: u64,
    pub evicted_traces: u64,
    pub oldest_started_at_unix_ms: Option<u64>,
    pub newest_started_at_unix_ms: Option<u64>,
}

#[derive(Debug)]
pub struct TraceRegistry {
    capacity: usize,
    sequence: AtomicU64,
    evicted: AtomicU64,
    traces: Mutex<VecDeque<OperationTrace>>,
}

impl TraceRegistry {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            sequence: AtomicU64::new(1),
            evicted: AtomicU64::new(0),
            traces: Mutex::new(VecDeque::new()),
        }
    }

    pub fn next_id(&self, prefix: &str) -> String {
        let now = now_unix_ms();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{now}-{sequence}")
    }

    pub fn record(&self, trace: OperationTrace) -> bool {
        let mut traces = self.traces.lock().expect("trace registry poisoned");
        traces.push_front(trace);
        let mut evicted = false;
        while traces.len() > self.capacity {
            traces.pop_back();
            self.evicted.fetch_add(1, Ordering::Relaxed);
            evicted = true;
        }
        evicted
    }

    pub fn list(&self, request: &TraceListRequest) -> Vec<OperationTrace> {
        let limit = request.limit.unwrap_or(self.capacity).min(self.capacity);
        let operation = request.operation.clone();
        let status = request.status.clone();
        let before_started_at_unix_ms = request.before_started_at_unix_ms;
        self.traces
            .lock()
            .expect("trace registry poisoned")
            .iter()
            .filter(|trace| {
                request
                    .tenant_id
                    .as_deref()
                    .is_none_or(|tenant_id| trace.tenant_id.as_deref() == Some(tenant_id))
            })
            .filter(|trace| {
                request
                    .namespace
                    .as_deref()
                    .is_none_or(|namespace| trace.namespace.as_deref() == Some(namespace))
            })
            .filter(|trace| {
                operation
                    .as_ref()
                    .is_none_or(|value| &trace.operation == value)
            })
            .filter(|trace| status.as_ref().is_none_or(|value| &trace.status == value))
            .filter(|trace| {
                before_started_at_unix_ms.is_none_or(|before| trace.started_at_unix_ms <= before)
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get(&self, trace_id: &str) -> Option<OperationTrace> {
        self.traces
            .lock()
            .expect("trace registry poisoned")
            .iter()
            .find(|trace| trace.trace_id == trace_id)
            .cloned()
    }

    pub fn snapshot(&self) -> TraceRegistrySnapshot {
        let traces = self.traces.lock().expect("trace registry poisoned");
        TraceRegistrySnapshot {
            stored_traces: traces.len() as u64,
            trace_capacity: self.capacity as u64,
            evicted_traces: self.evicted.load(Ordering::Relaxed),
            oldest_started_at_unix_ms: traces.back().map(|trace| trace.started_at_unix_ms),
            newest_started_at_unix_ms: traces.front().map(|trace| trace.started_at_unix_ms),
        }
    }
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
