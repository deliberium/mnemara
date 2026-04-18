use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionClass {
    Read,
    Write,
    Admin,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeAdmissionStatus {
    pub max_inflight_reads: u64,
    pub max_inflight_writes: u64,
    pub max_inflight_admin: u64,
    pub max_queued_requests: u64,
    pub max_tenant_inflight: u64,
    pub queued_total: u64,
    pub inflight_total: u64,
    pub queued_reads: u64,
    pub queued_writes: u64,
    pub queued_admin: u64,
    pub inflight_reads: u64,
    pub inflight_writes: u64,
    pub inflight_admin: u64,
    pub tenant_inflight: BTreeMap<String, u64>,
    pub queue_wait_timeout_ms: u64,
    pub average_queue_wait_ms: u64,
    pub max_queue_wait_ms: u64,
    pub oldest_queue_wait_ms: u64,
    pub oldest_read_queue_wait_ms: u64,
    pub oldest_write_queue_wait_ms: u64,
    pub oldest_admin_queue_wait_ms: u64,
    pub fair_queue_policy: String,
}

#[derive(Debug, Clone)]
pub struct AdmissionConfig {
    pub max_inflight_reads: usize,
    pub max_inflight_writes: usize,
    pub max_inflight_admin: usize,
    pub max_queued_requests: usize,
    pub max_tenant_inflight: usize,
    pub queue_wait_timeout_ms: u64,
}

#[derive(Debug)]
pub struct AdmissionController {
    config: AdmissionConfig,
    reads: Arc<Semaphore>,
    writes: Arc<Semaphore>,
    admin: Arc<Semaphore>,
    state: Mutex<AdmissionState>,
}

#[derive(Debug, Default)]
struct AdmissionState {
    queued_total: u64,
    inflight_total: u64,
    queued_reads: u64,
    queued_writes: u64,
    queued_admin: u64,
    inflight_reads: u64,
    inflight_writes: u64,
    inflight_admin: u64,
    tenant_inflight: BTreeMap<String, u64>,
    queued_read_since: VecDeque<u64>,
    queued_write_since: VecDeque<u64>,
    queued_admin_since: VecDeque<u64>,
    total_queue_wait_ms: u64,
    completed_queue_waits: u64,
    max_queue_wait_ms: u64,
}

#[derive(Debug)]
pub enum AdmissionError {
    QueueFull,
    TimedOut,
    TenantBusy,
}

#[derive(Debug)]
pub struct AdmissionGuard {
    controller: Arc<AdmissionController>,
    class: AdmissionClass,
    tenant_id: Option<String>,
    _permit: OwnedSemaphorePermit,
}

impl AdmissionController {
    pub fn new(config: AdmissionConfig) -> Self {
        Self {
            reads: Arc::new(Semaphore::new(config.max_inflight_reads.max(1))),
            writes: Arc::new(Semaphore::new(config.max_inflight_writes.max(1))),
            admin: Arc::new(Semaphore::new(config.max_inflight_admin.max(1))),
            config,
            state: Mutex::new(AdmissionState::default()),
        }
    }

    pub async fn acquire(
        self: &Arc<Self>,
        class: AdmissionClass,
        tenant_id: Option<&str>,
    ) -> Result<AdmissionGuard, AdmissionError> {
        {
            let mut state = self.state.lock().expect("admission state poisoned");
            if state.queued_total as usize >= self.config.max_queued_requests {
                return Err(AdmissionError::QueueFull);
            }
            let enqueued_at = now_unix_ms();
            state.queued_total += 1;
            match class {
                AdmissionClass::Read => {
                    state.queued_reads += 1;
                    state.queued_read_since.push_back(enqueued_at);
                }
                AdmissionClass::Write => {
                    state.queued_writes += 1;
                    state.queued_write_since.push_back(enqueued_at);
                }
                AdmissionClass::Admin => {
                    state.queued_admin += 1;
                    state.queued_admin_since.push_back(enqueued_at);
                }
            }
        }

        let permit_result = tokio::time::timeout(
            Duration::from_millis(self.config.queue_wait_timeout_ms.max(1)),
            match class {
                AdmissionClass::Read => self.reads.clone().acquire_owned(),
                AdmissionClass::Write => self.writes.clone().acquire_owned(),
                AdmissionClass::Admin => self.admin.clone().acquire_owned(),
            },
        )
        .await;

        let permit = match permit_result {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) | Err(_) => {
                self.release_queue(class);
                return Err(AdmissionError::TimedOut);
            }
        };

        let tenant_id = tenant_id.map(ToOwned::to_owned);
        self.release_queue(class);
        {
            let mut state = self.state.lock().expect("admission state poisoned");
            if let Some(tenant_id) = tenant_id.as_ref() {
                let current = state.tenant_inflight.get(tenant_id).copied().unwrap_or(0);
                if current as usize >= self.config.max_tenant_inflight {
                    return Err(AdmissionError::TenantBusy);
                }
                state.tenant_inflight.insert(tenant_id.clone(), current + 1);
            }

            state.inflight_total += 1;
            match class {
                AdmissionClass::Read => state.inflight_reads += 1,
                AdmissionClass::Write => state.inflight_writes += 1,
                AdmissionClass::Admin => state.inflight_admin += 1,
            }
        }

        Ok(AdmissionGuard {
            controller: Arc::clone(self),
            class,
            tenant_id,
            _permit: permit,
        })
    }

    pub fn snapshot(&self) -> RuntimeAdmissionStatus {
        let state = self.state.lock().expect("admission state poisoned");
        let now = now_unix_ms();
        RuntimeAdmissionStatus {
            max_inflight_reads: self.config.max_inflight_reads as u64,
            max_inflight_writes: self.config.max_inflight_writes as u64,
            max_inflight_admin: self.config.max_inflight_admin as u64,
            max_queued_requests: self.config.max_queued_requests as u64,
            max_tenant_inflight: self.config.max_tenant_inflight as u64,
            queued_total: state.queued_total,
            inflight_total: state.inflight_total,
            queued_reads: state.queued_reads,
            queued_writes: state.queued_writes,
            queued_admin: state.queued_admin,
            inflight_reads: state.inflight_reads,
            inflight_writes: state.inflight_writes,
            inflight_admin: state.inflight_admin,
            tenant_inflight: state.tenant_inflight.clone(),
            queue_wait_timeout_ms: self.config.queue_wait_timeout_ms,
            average_queue_wait_ms: if state.completed_queue_waits == 0 {
                0
            } else {
                state.total_queue_wait_ms / state.completed_queue_waits
            },
            max_queue_wait_ms: state.max_queue_wait_ms,
            oldest_queue_wait_ms: [
                state
                    .queued_read_since
                    .front()
                    .copied()
                    .map(|started| now.saturating_sub(started)),
                state
                    .queued_write_since
                    .front()
                    .copied()
                    .map(|started| now.saturating_sub(started)),
                state
                    .queued_admin_since
                    .front()
                    .copied()
                    .map(|started| now.saturating_sub(started)),
            ]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0),
            oldest_read_queue_wait_ms: state
                .queued_read_since
                .front()
                .copied()
                .map(|started| now.saturating_sub(started))
                .unwrap_or(0),
            oldest_write_queue_wait_ms: state
                .queued_write_since
                .front()
                .copied()
                .map(|started| now.saturating_sub(started))
                .unwrap_or(0),
            oldest_admin_queue_wait_ms: state
                .queued_admin_since
                .front()
                .copied()
                .map(|started| now.saturating_sub(started))
                .unwrap_or(0),
            fair_queue_policy: "fifo-per-class".to_string(),
        }
    }

    fn release_queue(&self, class: AdmissionClass) {
        let mut state = self.state.lock().expect("admission state poisoned");
        let started_at = match class {
            AdmissionClass::Read => {
                state.queued_reads = state.queued_reads.saturating_sub(1);
                state.queued_read_since.pop_front()
            }
            AdmissionClass::Write => {
                state.queued_writes = state.queued_writes.saturating_sub(1);
                state.queued_write_since.pop_front()
            }
            AdmissionClass::Admin => {
                state.queued_admin = state.queued_admin.saturating_sub(1);
                state.queued_admin_since.pop_front()
            }
        };
        state.queued_total = state.queued_total.saturating_sub(1);
        if let Some(started_at) = started_at {
            let waited = now_unix_ms().saturating_sub(started_at);
            state.total_queue_wait_ms = state.total_queue_wait_ms.saturating_add(waited);
            state.completed_queue_waits = state.completed_queue_waits.saturating_add(1);
            state.max_queue_wait_ms = state.max_queue_wait_ms.max(waited);
        }
    }
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        let mut state = self
            .controller
            .state
            .lock()
            .expect("admission state poisoned");
        state.inflight_total = state.inflight_total.saturating_sub(1);
        match self.class {
            AdmissionClass::Read => state.inflight_reads = state.inflight_reads.saturating_sub(1),
            AdmissionClass::Write => {
                state.inflight_writes = state.inflight_writes.saturating_sub(1)
            }
            AdmissionClass::Admin => state.inflight_admin = state.inflight_admin.saturating_sub(1),
        }
        if let Some(tenant_id) = self.tenant_id.as_ref() {
            let current = state.tenant_inflight.get(tenant_id).copied().unwrap_or(1);
            if current <= 1 {
                state.tenant_inflight.remove(tenant_id);
            } else {
                state.tenant_inflight.insert(tenant_id.clone(), current - 1);
            }
        }
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
