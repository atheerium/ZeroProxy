/// Minimal combo decision trace for visibility (Phase 3).
/// Records which members were tried, which were skipped, and why.
/// In-memory only, bounded size, fail-open.
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    AdaptiveTtftSkip,
    Quarantined,
    Degraded,
    PredictiveSlow,
    QueueExpired,
    Availability,
    Unknown,
}

impl SkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkipReason::AdaptiveTtftSkip => "adaptive_ttft_skip",
            SkipReason::Quarantined => "quarantined",
            SkipReason::Degraded => "degraded",
            SkipReason::PredictiveSlow => "predictive_slow",
            SkipReason::QueueExpired => "queue_expired",
            SkipReason::Availability => "availability",
            SkipReason::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraceDecision {
    pub step: String,
    pub target: String,
    pub decision: String, // "dispatched", "skipped_before_dispatch", "not_reached"
    pub reason: Option<SkipReason>,
    pub ts: u64,
}

#[derive(Debug, Clone)]
pub struct ComboTrace {
    pub invocation_id: String,
    pub created_at: u64,
    pub strategy: Option<String>,
    pub combo_name: Option<String>,
    pub decisions: Vec<TraceDecision>,
    pub terminal_status: Option<u16>,
    pub terminal_error_class: Option<String>,
}

/// In-memory bounded trace store.
pub struct DecisionTraceStore {
    traces: HashMap<String, ComboTrace>,
    max_traces: usize,
    ttl_ms: u64,
}

impl DecisionTraceStore {
    pub fn new(max_traces: usize, ttl_ms: u64) -> Self {
        Self {
            traces: HashMap::new(),
            max_traces,
            ttl_ms,
        }
    }

    pub fn start_trace(&mut self, id: String, meta: Option<(String, String)>) {
        // Evict expired first
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.traces
            .retain(|_, trace| now.saturating_sub(trace.created_at) < self.ttl_ms as u64);

        // Evict oldest if at capacity
        while self.traces.len() >= self.max_traces {
            let oldest = self.traces.keys().cloned().next();
            if let Some(k) = oldest {
                self.traces.remove(&k);
            } else {
                break;
            }
        }

        self.traces.insert(
            id.clone(),
            ComboTrace {
                invocation_id: id,
                created_at: now,
                strategy: meta.as_ref().map(|(_, s)| s.clone()),
                combo_name: meta.as_ref().map(|(c, _)| c.clone()),
                decisions: Vec::new(),
                terminal_status: None,
                terminal_error_class: None,
            },
        );
    }

    pub fn record_decision(&mut self, id: &str, entry: (String, String, Option<SkipReason>)) {
        if let Some(trace) = self.traces.get_mut(id) {
            let (step, target, reason) = entry;
            trace.decisions.push(TraceDecision {
                step,
                target,
                decision: if reason.is_some() {
                    "skipped_before_dispatch".to_string()
                } else {
                    "dispatched".to_string()
                },
                reason,
                ts: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            });
        }
    }

    pub fn finish_trace(&mut self, id: &str, status: Option<u16>, error_class: Option<String>) {
        if let Some(trace) = self.traces.get_mut(id) {
            trace.terminal_status = status;
            trace.terminal_error_class = error_class;
        }
    }

    pub fn get_trace(&self, id: &str) -> Option<ComboTrace> {
        self.traces.get(id).cloned()
    }
}

use once_cell::sync::Lazy;
/// Global instance.
use std::sync::Mutex;

pub static TRACE_STORE: Lazy<Mutex<DecisionTraceStore>> =
    Lazy::new(|| Mutex::new(DecisionTraceStore::new(2000, 30 * 60 * 1000)));
