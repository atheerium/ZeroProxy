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
                bytesBefore, bytesAfter, bytesSaved, imagePrompts,
                success, latency_ms, ttft_ms
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
                bytesBefore, bytesAfter, bytesSaved, imagePrompts,
                success, latency_ms, ttft_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
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
            entry.success.map(|s| s as i32).unwrap_or(1),
            entry.latency_ms,
            entry.ttft_ms,
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
    let success: Option<bool> = row.get::<_, Option<i32>>(16)?.map(|v| v != 0);
    let latency_ms: Option<i64> = row.get(17)?;
    let ttft_ms: Option<i64> = row.get(18)?;

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
        success,
        bytes_before,
        bytes_after,
        bytes_saved,
        image_prompts,
        extra,
        latency_ms,
        ttft_ms,
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

    #[test]
    fn roundtrip_with_success_latency_ttft_columns() {
        let db = SqliteDb::open_in_memory().unwrap();
        let entry = UsageEntry {
            model: "test-model".into(),
            provider: Some("test-provider".into()),
            timestamp: Some("2026-09-26T12:00:00Z".into()),
            status: Some("success".into()),
            success: Some(true),
            latency_ms: Some(5000),
            ttft_ms: Some(1200),
            ..Default::default()
        };
        db.with_transaction(|tx| insert(tx, &entry)).unwrap();
        let history = db.with_conn(|c| get_history(c, 10, 0)).unwrap();
        assert_eq!(history.len(), 1);
        let loaded = &history[0];
        assert_eq!(loaded.model, "test-model");
        assert_eq!(loaded.provider.as_deref(), Some("test-provider"));
        assert_eq!(loaded.status.as_deref(), Some("success"));
        assert_eq!(loaded.success, Some(true));
        assert_eq!(loaded.latency_ms, Some(5000));
        assert_eq!(loaded.ttft_ms, Some(1200));
    }

    #[test]
    fn roundtrip_with_failure_success_false() {
        let db = SqliteDb::open_in_memory().unwrap();
        let entry = UsageEntry {
            model: "failed-model".into(),
            provider: Some("failed-provider".into()),
            timestamp: Some("2026-09-26T12:00:00Z".into()),
            status: Some("error".into()),
            success: Some(false),
            latency_ms: None,
            ttft_ms: None,
            ..Default::default()
        };
        db.with_transaction(|tx| insert(tx, &entry)).unwrap();
        let history = db.with_conn(|c| get_history(c, 10, 0)).unwrap();
        assert_eq!(history.len(), 1);
        let loaded = &history[0];
        assert_eq!(loaded.success, Some(false));
        assert_eq!(loaded.latency_ms, None);
        assert_eq!(loaded.ttft_ms, None);
    }

    #[test]
    fn migration_backfill_populates_columns_from_meta() {
        // Build the table at its PRE-migration shape. Using `open_in_memory()`
        // would create it from the current schema, which already has the three
        // columns, so the migration's ALTER steps would be skipped and the
        // backfill would have nothing to prove.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE usageHistory (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL,
                 provider TEXT, model TEXT, connectionId TEXT, apiKey TEXT,
                 endpoint TEXT, promptTokens INTEGER DEFAULT 0,
                 completionTokens INTEGER DEFAULT 0, cost REAL DEFAULT 0,
                 status TEXT, tokens TEXT, meta TEXT, bytesBefore INTEGER DEFAULT 0,
                 bytesAfter INTEGER DEFAULT 0, bytesSaved INTEGER DEFAULT 0,
                 imagePrompts INTEGER DEFAULT 0
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO usageHistory(timestamp, provider, model, status, meta)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "2026-09-01T00:00:00Z",
                "openrouter",
                "test/model",
                "success",
                serde_json::json!({"latency": {"total": 36655, "ttft": 9052}}).to_string(),
            ],
        )
        .unwrap();

        crate::db::sqlite::migrations::add_usage_history_reliability_columns(&conn).unwrap();

        let history = get_history(&conn, 10, 0).unwrap();
        assert_eq!(history.len(), 1);
        let loaded = &history[0];
        assert_eq!(loaded.success, Some(true));
        assert_eq!(loaded.latency_ms, Some(36655));
        assert_eq!(loaded.ttft_ms, Some(9052));
    }

    #[test]
    fn migration_backfill_runs_once_and_is_not_repeated() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE usageHistory (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL,
                 provider TEXT, model TEXT, connectionId TEXT, apiKey TEXT,
                 endpoint TEXT, promptTokens INTEGER DEFAULT 0,
                 completionTokens INTEGER DEFAULT 0, cost REAL DEFAULT 0,
                 status TEXT, tokens TEXT, meta TEXT, bytesBefore INTEGER DEFAULT 0,
                 bytesAfter INTEGER DEFAULT 0, bytesSaved INTEGER DEFAULT 0,
                 imagePrompts INTEGER DEFAULT 0
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO usageHistory(timestamp, provider, model, status, meta)
             VALUES('2026-09-01T00:00:00Z', 'openrouter', 'test/model', 'success', '{}')",
            [],
        )
        .unwrap();

        crate::db::sqlite::migrations::add_usage_history_reliability_columns(&conn).unwrap();
        let marker: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _meta WHERE key = 'usage_history_reliability_backfilled'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(marker, 1, "backfill must record its completion marker");

        // A row inserted afterwards has no latency in `meta`, so its latency_ms
        // stays NULL. Re-running the migration must NOT re-scan the table, which
        // is what the marker is for.
        conn.execute(
            "INSERT INTO usageHistory(timestamp, provider, model, status, meta)
             VALUES('2026-09-02T00:00:00Z', 'openrouter', 'test/model2', 'success', '{}')",
            [],
        )
        .unwrap();
        crate::db::sqlite::migrations::add_usage_history_reliability_columns(&conn).unwrap();
    }
}
