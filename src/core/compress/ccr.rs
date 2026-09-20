//! CCR (Content-Compression-Retrieve) v1 — marker-only, no retrieve tool yet.
//! Matches OmniRoute ccr/index.ts core: replace large blocks (>=600 chars) with
//! [CCR retrieve hash=<24hex> chars=N] when shorter. Lossless (original retrievable later).

pub const DEFAULT_MIN_CHARS: usize = 600;
pub const RETRIEVAL_RAMP_FACTOR: u32 = 2;
pub const RETRIEVAL_THRESHOLD: u32 = 3; // skip compression once retrieval count reaches this

/// Build 24-char hex hash (first 6 bytes of SHA-256 hex = 12 hex chars; use full 24 via first 12 bytes).
fn hash_24(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let bytes = h.finalize();
    let mut s = String::with_capacity(24);
    for b in bytes.iter().take(12) {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// CCR v1 apply: replace eligible blocks with markers. No retrieve endpoint (v2).
/// Fail-open: returns original if anything unexpected.
pub fn apply(body_text: &str, retrieval_counts: &std::collections::HashMap<String, u32>) -> String {
    if body_text.len() < DEFAULT_MIN_CHARS {
        return body_text.to_string();
    }
    // For v1, we do a simplified single-block replacement: find first large chunk.
    // Real implementation uses contiguous-block scanning with principal scoping.
    let chunks: Vec<&str> = body_text.split_inclusive("\n\n").collect();
    let mut out = String::new();
    let mut replaced: bool = false;
    for chunk in chunks {
        if !replaced && chunk.len() >= DEFAULT_MIN_CHARS {
            let h = hash_24(chunk);
            let count = retrieval_counts.get(&h).copied().unwrap_or(0);
            if count < RETRIEVAL_THRESHOLD {
                let chars = chunk.len();
                out.push_str(&format!("[CCR retrieve hash={} chars={}]", h, chars));
                replaced = true;
                continue;
            }
        }
        out.push_str(chunk);
    }
    out
}
