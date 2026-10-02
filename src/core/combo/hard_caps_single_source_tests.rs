//! Drift guard for the hard-capability list and its heuristic.
//!
//! The list existed in four independent copies (`combo/mod.rs`,
//! `combo/capabilities.rs`, `combo/capacity_adapter.rs` as `CAPABILITY_KEYS`,
//! and `core/auto`), and `model_has_capability` in three. The copies agreed, so
//! no test failed — but a capability fix applied to one copy would have
//! silently not applied to the others, which is how the combo gate and the
//! auto-candidate drop path could disagree. They are now one definition each.
//!
//! This module lives in its own file so the scan below can exclude it by path:
//! the pinned literals necessarily appear in this file's own source, and
//! `cargo fmt` collapses any multi-line array back onto one line.

use super::{model_has_capability, reorder_by_capabilities, HARD_CAPS};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// This file. Excluded from every scan so the guard never counts itself.
const SELF: &str = "hard_caps_single_source_tests.rs";

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("src/ must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every `.rs` file under `src/` except this one.
fn other_rust_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    files.retain(|p| !p.ends_with(SELF));
    assert!(
        files.len() > 100,
        "source walk found only {} files; the guard is not really scanning src/",
        files.len()
    );
    files
}

fn occurrences(needle: &str) -> Vec<(PathBuf, usize)> {
    other_rust_sources()
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let count = text.matches(needle).count();
            (count > 0).then_some((path, count))
        })
        .collect()
}

fn paths(hits: &[(PathBuf, usize)]) -> Vec<String> {
    hits.iter().map(|(p, _)| p.display().to_string()).collect()
}

#[test]
fn hard_caps_list_literal_appears_exactly_once_in_src() {
    let hits = occurrences(&format!("{:?}", HARD_CAPS));
    let total: usize = hits.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total,
        1,
        "the hard-capability list must be written out in exactly one file, found {total} in {:?}",
        paths(&hits)
    );
}

#[test]
fn no_module_declares_its_own_hard_caps_constant() {
    let mut hits = occurrences("const HARD_CAPS");
    hits.extend(occurrences("const CAPABILITY_KEYS"));
    assert_eq!(
        hits.len(),
        1,
        "only the canonical definition may declare a hard-capability constant, found: {:?}",
        paths(&hits)
    );
}

#[test]
fn model_has_capability_is_defined_exactly_once_in_src() {
    let hits = occurrences("fn model_has_capability(");
    let total: usize = hits.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total,
        1,
        "model_has_capability must be defined exactly once, found {total} in {:?}",
        paths(&hits)
    );
}

#[test]
fn hard_caps_contents_are_pinned() {
    assert_eq!(HARD_CAPS, ["vision", "pdf", "audioInput", "videoInput"]);
}

#[test]
fn capability_heuristic_answers_are_pinned() {
    assert!(model_has_capability("google/gemini-3-pro", "vision"));
    assert!(model_has_capability("anthropic/claude-opus-4.7", "pdf"));
    assert!(model_has_capability("oc/mimo-v2.5-free", "audioInput"));
    assert!(model_has_capability("oc/mimo-v2.5-free", "videoInput"));
    assert!(!model_has_capability("openai/gpt-3.5-turbo", "vision"));
    assert!(!model_has_capability("openai/gpt-4o", "pdf"));
    assert!(!model_has_capability("google/gemini-3-pro", "tools"));
}

/// A hard-cap mismatch must demote a model to last-resort placement, never
/// remove it — `core/auto` applies the same predicate but *drops* candidates,
/// so the two paths are only equivalent because they share this one function.
#[test]
fn hard_cap_mismatch_demotes_rather_than_removes() {
    let required: HashSet<String> = ["vision".to_string()].into_iter().collect();
    let ordered = reorder_by_capabilities(
        &[
            "openai/gpt-3.5-turbo".to_string(),
            "google/gemini-3-pro".to_string(),
        ],
        &required,
    );
    assert_eq!(
        ordered.len(),
        2,
        "a hard-cap mismatch must not remove a model"
    );
    assert_eq!(ordered[0], "google/gemini-3-pro");
    assert_eq!(ordered[1], "openai/gpt-3.5-turbo");
}

#[test]
fn input_order_is_preserved_when_no_capabilities_are_required() {
    let models = vec!["b/model".to_string(), "a/model".to_string()];
    assert_eq!(reorder_by_capabilities(&models, &HashSet::new()), models);
}
