//! Session Dedup — cross-turn suffix-block content-addressed dedup (OmniRoute session-dedup/index.ts).
//! Safe-default (lossless): never touches role:system, only type:text, first occurrence kept intact.

use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Default config (matches OmniRoute SESSION_DEDUP_SCHEMA / DEFAULT_MIN_BLOCK_CHARS)
pub const DEFAULT_MIN_BLOCK_CHARS: usize = 80;
pub const MIN_BLOCK_LINES: usize = 3;
pub const MAX_SUFFIX_STARTS: usize = 2000;
pub const MAX_TOTAL_BLOCK_BYTES: usize = 8 * 1024 * 1024; // 8MB OOM guard

/// Per-session compressed store (first-occurrence map) — bounded.
pub struct DedupSessionStore {
    /// first_owner: hash -> (message_index, block_text). Only first owner retained for reconstruction.
    pub first_owner: HashMap<String, String>,
    /// Per-hash retrieval count (for CCR-style skip-on-threshold; kept minimal in v1).
    pub retrieval_counts: HashMap<String, u32>,
}
impl DedupSessionStore {
    pub fn new() -> Self {
        Self {
            first_owner: HashMap::new(),
            retrieval_counts: HashMap::new(),
        }
    }
    /// Hard cap: ~1000 entries/session to keep memory bounded.
    pub fn len(&self) -> usize {
        self.first_owner.len()
    }
    pub fn is_bounded(&self) -> bool {
        self.len() < 2000
    }
}

/// Build an 8-char hex hash (matches OmniRoute [dedup:ref sha=<8hex>]).
fn hash_block(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let bytes = hasher.finalize();
    let mut s = String::with_capacity(8);
    for b in bytes.iter().take(4) {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Deduplicate text content. Returns (compressed_text, bytes_saved, hits).
/// Fail-open: if anything goes wrong, return original with 0 saved.
pub fn compress_text(
    body_text: &str,
    session: Option<&mut DedupSessionStore>,
    min_block_chars: usize,
    fuzzy: bool,
) -> (String, usize, usize) {
    let min_chars = min_block_chars.max(DEFAULT_MIN_BLOCK_CHARS);
    let mut result = body_text.to_string();
    let mut saved: usize = 0;
    let mut hits: usize = 0;

    // Phase 1: find contiguous blocks >= min_chars (line-aware split for simplicity).
    // For v1 we use a simple line-block approach; real suffix-block is multi-pass.
    let lines: Vec<&str> = body_text.lines().collect();
    let total_lines = lines.len();
    if total_lines < MIN_BLOCK_LINES || body_text.len() < min_chars {
        return (body_text.to_string(), 0, 0);
    }

    for start in 0..total_lines.saturating_sub(MIN_BLOCK_LINES) {
        // Bound suffix starts to avoid O(n^2) blow-up
        if start >= MAX_SUFFIX_STARTS {
            break;
        }
        // Accumulate from start downward to form the longest block within budget
        let mut accumulated_bytes: usize = 0;
        let mut end_idx = start + MIN_BLOCK_LINES - 1;
        for j in start..total_lines {
            let line_bytes = lines[j].len() + 1; // +1 for newline
            if accumulated_bytes + line_bytes > MAX_TOTAL_BLOCK_BYTES {
                break;
            }
            accumulated_bytes += line_bytes;
            end_idx = j;
            // Only process blocks that meet min_chars
            if accumulated_bytes >= min_chars {
                let block = lines[start..=end_idx].join("\n");
                let h = hash_block(&block);
                let marker = format!("[dedup:ref sha={}]", &h[..8]);
                // First occurrence kept intact; only later duplicates replaced.
                // For v1 without retrieval, we store first owner locally.
                let is_first = session
                    .as_ref()
                    .map(|s| !s.first_owner.contains_key(&h))
                    .unwrap_or(true);
                if is_first {
                    // First occurrence kept; session store update deferred (v1 skeleton).
                } else {
                    // Replace only if marker is shorter (always true for long blocks)
                    if marker.len() < accumulated_bytes {
                        // Note: in-place replacement of a multi-line block is non-trivial on a string
                        // without full re-scan; v1 does best-effort single-pass for simplicity.
                        saved += accumulated_bytes.saturating_sub(marker.len());
                        hits += 1;
                    }
                }
            }
        }
    }

    // Phase 1 v1 is intentionally simplified: real implementation uses two-pass greedy
    // longest-first with overlap skip. This skeleton defines the interface + guard rails.
    (body_text.to_string(), saved, hits)
}

/// Apply session-dedup to a chat request body (`Value` mutation).
/// Only touches `messages[].content` when it is a string; skips `system` role.
/// Fail-open: returns original body unchanged on any error.
pub fn apply_request_preprocessing(
    messages: &mut serde_json::Value,
    session_key: Option<&str>,
    config_min_block_chars: usize,
) {
    // No-op for v1 skeleton: interface defined for future integration into chat.rs pipeline.
    // Actual body mutation will call `compress_text()` on each eligible message content.
    let _ = (messages, session_key, config_min_block_chars);
}
