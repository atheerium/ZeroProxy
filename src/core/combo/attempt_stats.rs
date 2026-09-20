// src/core/combo/attempt_stats.rs
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-member statistics for adaptive TTFT and provider patterns.
#[derive(Debug, Clone)]
pub struct MemberStats {
    pub provider: String,
    pub model: String,
    pub attempts: u64,
    pub successes: u64,
    pub ttft_samples: Vec<u64>,
    pub latency_samples: Vec<u64>,
    pub statuses: HashMap<u16, u64>,
    pub error_classes: HashMap<String, u64>,
    pub timeouts: u64,
    pub first_seen: Instant,
    pub last_attempt: Instant,
}

impl MemberStats {
    fn new(provider: String, model: String) -> Self {
        Self {
            provider,
            model,
            attempts: 0,
            successes: 0,
            ttft_samples: Vec::with_capacity(64),
            latency_samples: Vec::with_capacity(64),
            statuses: HashMap::new(),
            error_classes: HashMap::new(),
            timeouts: 0,
            first_seen: Instant::now(),
            last_attempt: Instant::now(),
        }
    }

    fn record_attempt(
        &mut self,
        status: u16,
        ttft_ms: Option<u64>,
        latency_ms: u64,
        error_class: Option<String>,
        is_success: bool,
        is_timeout: bool,
    ) {
        self.attempts += 1;
        self.last_attempt = Instant::now();
        *self.statuses.entry(status).or_insert(0) += 1;
        if let Some(ec) = error_class {
            *self.error_classes.entry(ec).or_insert(0) += 1;
        }
        if is_success {
            self.successes += 1;
        }
        if let Some(ttft) = ttft_ms {
            self.ttft_samples.push(ttft);
            if self.ttft_samples.len() > 64 {
                self.ttft_samples.drain(0..8);
            }
        }
        self.latency_samples.push(latency_ms);
        if self.latency_samples.len() > 64 {
            self.latency_samples.drain(0..8);
        }
        if is_timeout {
            self.timeouts += 1;
        }
    }
}

/// In-memory per-member stats cache.
pub struct ComboAttemptStats {
    stats: Mutex<HashMap<(String, String), MemberStats>>, // (combo_name, model)
    max_entries: usize,
    entry_ttl_ms: u64,
}

impl ComboAttemptStats {
    /// Create new stats cache with TTL-based eviction.
    pub fn new(max_entries: usize, entry_ttl_ms: u64) -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            max_entries,
            entry_ttl_ms,
        }
    }

    /// Record an attempt for a specific combo member.
    pub fn record_attempt(
        &self,
        combo_name: &str,
        model: &str,
        provider: &str,
        status: u16,
        ttft_ms: Option<u64>,
        latency_ms: u64,
        error_class: Option<String>,
        is_success: bool,
        is_timeout: bool,
    ) {
        let key = (combo_name.to_string(), model.to_string());
        let mut guard = self.stats.lock().unwrap();

        // Evict stale entries before accessing.
        let now = Instant::now();
        guard.retain(|_, stats| {
            now.duration_since(stats.last_attempt) < Duration::from_millis(self.entry_ttl_ms)
        });

        // Enforce max entries cap (evict oldest).
        while guard.len() >= self.max_entries {
            let oldest = guard
                .iter()
                .min_by_key(|(_, stats)| stats.first_seen)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                guard.remove(&k);
            } else {
                break;
            }
        }

        // Update or insert stats.
        let entry = guard
            .entry(key)
            .or_insert_with(|| MemberStats::new(provider.to_string(), model.to_string()));

        entry.record_attempt(
            status,
            ttft_ms,
            latency_ms,
            error_class,
            is_success,
            is_timeout,
        );
    }

    /// Get stats for a specific combo member.
    pub fn get_stats(&self, combo_name: &str, model: &str) -> Option<MemberStats> {
        let guard = self.stats.lock().unwrap();
        guard
            .get(&(combo_name.to_string(), model.to_string()))
            .cloned()
    }

    /// Get status histogram for a specific combo member.
    pub fn get_status_histogram(&self, combo_name: &str, model: &str) -> HashMap<u16, u64> {
        let guard = self.stats.lock().unwrap();
        guard
            .get(&(combo_name.to_string(), model.to_string()))
            .map(|stats| stats.statuses.clone())
            .unwrap_or_default()
    }

    /// Get TTFT percentile (p90) for a combo member.
    pub fn get_ttft_p90(&self, combo_name: &str, model: &str) -> Option<u64> {
        let guard = self.stats.lock().unwrap();
        guard
            .get(&(combo_name.to_string(), model.to_string()))
            .and_then(|stats| {
                if stats.ttft_samples.is_empty() {
                    None
                } else {
                    let mut sorted = stats.ttft_samples.clone();
                    sorted.sort_unstable();
                    let idx = (sorted.len() as f64 * 0.9) as usize;
                    Some(sorted[idx])
                }
            })
    }

    /// Get average latency for a combo member.
    pub fn get_avg_latency(&self, combo_name: &str, model: &str) -> Option<u64> {
        let guard = self.stats.lock().unwrap();
        guard
            .get(&(combo_name.to_string(), model.to_string()))
            .and_then(|stats| {
                if stats.latency_samples.is_empty() {
                    None
                } else {
                    let sum: u64 = stats.latency_samples.iter().sum();
                    Some(sum / stats.latency_samples.len() as u64)
                }
            })
    }

    /// Get attempt count for a combo member.
    pub fn get_attempt_count(&self, combo_name: &str, model: &str) -> u64 {
        let guard = self.stats.lock().unwrap();
        guard
            .get(&(combo_name.to_string(), model.to_string()))
            .map(|stats| stats.attempts)
            .unwrap_or(0)
    }
}

/// Global instance of attempt stats cache.
pub fn attempt_stats() -> &'static ComboAttemptStats {
    static INSTANCE: std::sync::OnceLock<ComboAttemptStats> = std::sync::OnceLock::new();
    INSTANCE.get_or_init(|| ComboAttemptStats::new(2000, 5 * 60 * 1000))
}

/// Convenience function to record an attempt.
pub fn record_combo_attempt(
    combo_name: &str,
    model: &str,
    provider: &str,
    status: u16,
    ttft_ms: Option<u64>,
    latency_ms: u64,
    error_class: Option<String>,
    is_success: bool,
    is_timeout: bool,
) {
    attempt_stats().record_attempt(
        combo_name,
        model,
        provider,
        status,
        ttft_ms,
        latency_ms,
        error_class,
        is_success,
        is_timeout,
    );
}

/// Get status histogram for analysis.
pub fn get_combo_status_histogram(combo_name: &str, model: &str) -> HashMap<u16, u64> {
    attempt_stats().get_status_histogram(combo_name, model)
}

/// Get TTFT percentile for adaptive timeouts.
pub fn get_combo_ttft_p90(combo_name: &str, model: &str) -> Option<u64> {
    attempt_stats().get_ttft_p90(combo_name, model)
}

/// Get average latency for performance analysis.
pub fn get_combo_avg_latency(combo_name: &str, model: &str) -> Option<u64> {
    attempt_stats().get_avg_latency(combo_name, model)
}

/// Get attempt count for provider analysis.
pub fn get_combo_attempt_count(combo_name: &str, model: &str) -> u64 {
    attempt_stats().get_attempt_count(combo_name, model)
}
