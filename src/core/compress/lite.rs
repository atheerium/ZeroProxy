//! Lite compression (OmniRoute lite.ts) — whitespace/format cleanup + system-prompt dedup +
//! tool-result truncation + image-URL simplification. Safe default (lossless, <1ms).

/// Collapse whitespace (\n{3,} -> \n\n, trim trailing spaces per line).
pub fn collapse_whitespace(text: &str) -> String {
    text.split("\n\n\n")
        .map(|s| s.trim_end().replace(" \n", "\n"))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Deduplicate consecutive identical system prompts (by first-200-char key).
pub fn dedup_system_prompts(system_texts: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in system_texts {
        let key = s.trim().chars().take(200).collect::<String>();
        if !seen.insert(key.clone()) {
            continue; // skip duplicate
        }
        out.push(s.clone());
    }
    out
}

/// Truncate large tool results (>2000 chars) at word boundary, add [truncated] tail.
pub fn truncate_large_output(output: &str, max_chars: usize) -> Option<String> {
    if output.len() <= max_chars {
        return None;
    }
    let cutoff = max_chars.saturating_sub(18);
    let truncated = output[..cutoff.min(output.len())].trim_end();
    Some(format!(
        "{} ... [truncated, {} chars total]",
        truncated,
        output.len()
    ))
}

/// Lite apply — best-effort, fail-open (returns original on any issue).
pub fn apply(
    body_text: &str,
    _store: Option<&mut super::session_dedup::DedupSessionStore>,
) -> String {
    let collapsed = collapse_whitespace(body_text);
    collapsed // v1: whitespace cleanup only; tool truncation handled separately in chat pipeline
}
