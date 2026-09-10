//! Repository for `usageHistory` and `usageDaily` tables.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::types::{DailySummary, UsageEntry};

pub fn get_history(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<UsageEntry>> {
    let mut stmt = conn.prepare(
        "SELECT timestamp, provider, model, connectionId, apiKey, endpoint,
                promptTokens, completionTokens, cost, status, tokens, meta,
                bytesBefore, bytesAfter, bytesSaved, imagePrompts
         FROM usageHistory ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], row_to_usage)?;
    rows.collect()
}

pub fn insert(conn: &Connection, entry: &UsageEntry) -> rusqlite::Result<()> {
    let tokens_json = entry
        .tokens
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_default());
    let meta_json = if entry.extra.is_empty() {
        None::<String>
    } else {
        Some(serde_json::to_string(&entry.extra).unwrap_or_default())
    };
    conn.execute(
        "INSERT INTO usageHistory(timestamp, provider, model, connectionId, apiKey, endpoint,
                promptTokens, completionTokens, cost, status, tokens, meta,
                bytesBefore, bytesAfter, bytesSaved, imagePrompts)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            entry.timestamp.as_deref().unwrap_or(""),
            entry.provider.as_deref(),
            entry.model,
            entry.connection_id.as_deref(),
            entry.api_key.as_deref(),
            entry.endpoint.as_deref(),
            entry
                .tokens
                .as_ref()
                .and_then(|t| t.prompt_tokens.or(t.input_tokens))
                .unwrap_or(0) as i64,
            entry
                .tokens
                .as_ref()
                .and_then(|t| t.completion_tokens.or(t.output_tokens))
                .unwrap_or(0) as i64,
            entry.cost,
            entry.status.as_deref(),
            tokens_json,
            meta_json,
            entry.bytes_before as i64,
            entry.bytes_after as i64,
            entry.bytes_saved as i64,
            entry.image_prompts as i64,
        ],
    )?;
    Ok(())
}

pub fn get_daily(conn: &Connection, date_key: &str) -> rusqlite::Result<Option<DailySummary>> {
    let mut stmt = conn.prepare("SELECT data FROM usageDaily WHERE dateKey = ?1")?;
    let mut rows = stmt.query_map(params![date_key], |row| {
        let s: String = row.get(0)?;
        serde_json::from_str(&s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    })?;
    rows.next().transpose()
}

pub fn upsert_daily(
    conn: &Connection,
    date_key: &str,
    summary: &DailySummary,
) -> rusqlite::Result<()> {
    let json_str = serde_json::to_string(summary).unwrap_or_else(|_| "{}".into());
    conn.execute(
        "INSERT INTO usageDaily(dateKey, data) VALUES(?1,?2) ON CONFLICT(dateKey) DO UPDATE SET data = excluded.data",
        params![date_key, json_str],
    )?;
    Ok(())
}

fn row_to_usage(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageEntry> {
    let timestamp: Option<String> = row.get(0)?;
    let provider: Option<String> = row.get(1)?;
    let model: String = row.get(2)?;
    let connection_id: Option<String> = row.get(3)?;
    let api_key: Option<String> = row.get(4)?;
    let endpoint: Option<String> = row.get(5)?;
    let prompt_tokens: Option<i64> = row.get(6)?;
    let completion_tokens: Option<i64> = row.get(7)?;
    let cost: Option<f64> = row.get(8)?;
    let status: Option<String> = row.get(9)?;
    let tokens_str: Option<String> = row.get(10)?;
    let meta_str: Option<String> = row.get(11)?;
    let bytes_before: u64 = row.get::<_, i64>(12).unwrap_or(0) as u64;
    let bytes_after: u64 = row.get::<_, i64>(13).unwrap_or(0) as u64;
    let bytes_saved: u64 = row.get::<_, i64>(14).unwrap_or(0) as u64;
    let image_prompts: u64 = row.get::<_, i64>(15).unwrap_or(0) as u64;

    let tokens = tokens_str.and_then(|s| serde_json::from_str(&s).ok());
    let extra: std::collections::BTreeMap<String, Value> = meta_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    Ok(UsageEntry {
        timestamp,
        provider,
        model,
        tokens,
        connection_id,
        api_key,
        endpoint,
        cost,
        status,
        bytes_before,
        bytes_after,
        bytes_saved,
        image_prompts,
        extra,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::SqliteDb;
    use serde_json::json;

    #[test]
    fn roundtrip() {
        let db = SqliteDb::open_in_memory().unwrap();
        let entry = UsageEntry {
            model: "gpt-4o".into(),
            provider: Some("openai".into()),
            timestamp: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        db.with_transaction(|tx| insert(tx, &entry)).unwrap();
        let history = db.with_conn(|c| get_history(c, 10, 0)).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].model, "gpt-4o");
    }

    #[test]
    fn roundtrip_with_latency_and_connection() {
        let db = SqliteDb::open_in_memory().unwrap();
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("latency".to_string(), json!({"total": 1234, "ttft": 567}));
        extra.insert("error_class".to_string(), json!("timeout"));
        let entry = UsageEntry {
            model: "nvidia/nemotron-3-super-120b-a12b".into(),
            provider: Some("nvidia".into()),
            timestamp: Some("2026-09-09T00:12:24Z".into()),
            connection_id: Some("conn-abc".into()),
            api_key: Some("sk-nvidia-key".into()),
            endpoint: Some("/v1/chat/completions".into()),
            status: Some("success".into()),
            cost: Some(0.05),
            latency_ms: Some(1234),
            ttft_ms: Some(567),
            extra,
            ..Default::default()
        };
        db.with_transaction(|tx| insert(tx, &entry)).unwrap();
        let history = db.with_conn(|c| get_history(c, 10, 0)).unwrap();
        assert_eq!(history.len(), 1);
        let loaded = &history[0];
        assert_eq!(loaded.model, "nvidia/nemotron-3-super-120b-a12b");
        assert_eq!(loaded.provider.as_deref(), Some("nvidia"));
        assert_eq!(loaded.connection_id.as_deref(), Some("conn-abc"));
        assert_eq!(loaded.api_key.as_deref(), Some("sk-nvidia-key"));
        assert_eq!(loaded.endpoint.as_deref(), Some("/v1/chat/completions"));
        assert_eq!(loaded.status.as_deref(), Some("success"));
        assert_eq!(loaded.cost, Some(0.05));
        // Verify latency survived via extra
        let latency = loaded.extra.get("latency").unwrap();
        assert_eq!(latency["total"], 1234);
        assert_eq!(latency["ttft"], 567);
        assert_eq!(loaded.extra["error_class"], "timeout");
    }
}
