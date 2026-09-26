//! Versioned migration runner for the CipherRoute SQLite schema.
//!
//! Migration files live under `src/db/sqlite/migrations/` and follow the
//! naming convention `NNNN_description.sql`. Each file is wrapped in a
//! transaction by [`apply_pending_migrations`].
//!
//! Currently the schema is initialized via [`crate::db::sqlite::schema::TABLES_SQL`]
//! (DDL is idempotent thanks to `IF NOT EXISTS`), and this module just
//! records the schema version into `_meta`. Future migrations append here.

use rusqlite::{params, Connection, OptionalExtension};

use super::schema::SCHEMA_VERSION;

/// Get the current schema version stored in `_meta`. Returns 0 if the row
/// is missing (fresh DB).
pub fn get_schema_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.query_row(
        "SELECT value FROM _meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|opt| opt.and_then(|s| s.parse::<i32>().ok()).unwrap_or(0))
}

/// Stamp the active schema version into `_meta`. Idempotent.
pub fn set_schema_version(conn: &Connection, version: i32) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO _meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![version.to_string()],
    )?;
    Ok(())
}

/// Run any pending migrations between `current` and [`SCHEMA_VERSION`].
///
/// This module currently has no version-gated migrations (the schema is fully
/// expressed by `TABLES_SQL`). The function exists as the extension point
/// for future schema-evolution scripts; for now it only stamps the version
/// so callers can distinguish "fresh DB" from "DB at current version".
///
/// Idempotent column additions (e.g. `apiKeys.monthly_budget_usd`) run on
/// every open so databases created before a field was added pick it up
/// without a version bump.
pub fn apply_pending_migrations(conn: &Connection) -> rusqlite::Result<()> {
    // Ensure the apiKeys table carries the monthly_budget_usd column
    // (free-tier Feature 3). Safe to run on every open — no-op when present.
    add_api_keys_budget_column(conn)?;
    rekey_custom_models(conn)?;

    let current = get_schema_version(conn)?;
    if current < SCHEMA_VERSION {
        set_schema_version(conn, SCHEMA_VERSION)?;
    }
    Ok(())
}

/// Add `monthly_budget_usd REAL` to `apiKeys` if it is missing. SQLite has no
/// `ADD COLUMN IF NOT EXISTS`, so we probe `pragma_table_info` first.
fn add_api_keys_budget_column(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='apiKeys'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if !table_exists {
        return Ok(());
    }
    let has_column: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('apiKeys') WHERE name = 'monthly_budget_usd'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);
    if !has_column {
        conn.execute("ALTER TABLE apiKeys ADD COLUMN monthly_budget_usd REAL", [])?;
    }
    Ok(())
}

/// Re-key `customModels` rows from the bare model id to `alias/id`.
///
/// Rows were originally keyed by model id alone, but a model id is only unique
/// within a provider, so two providers offering the same id collapsed onto one
/// row and one provider's model was silently lost. This cannot be left to
/// `diff_kv_scope`: that derives the "old" side from the in-memory `Vec` rather
/// than the table, so a bare-id row is never seen and never deleted, while
/// `export_all` reads by scope alone — the orphan would come back as a
/// duplicate `CustomModel` on the next load.
///
/// Idempotent: rows already keyed `alias/id` are skipped, and a row whose
/// composite key already exists keeps that row (it was written by the current
/// process and is therefore the authoritative copy).
fn rekey_custom_models(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if !table_exists {
        return Ok(());
    }

    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT key, value FROM kv WHERE scope = 'customModels'")?;
        let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut collected = Vec::new();
        for row in mapped {
            collected.push(row?);
        }
        collected
    };

    for (key, value) in rows {
        let parsed: serde_json::Value = match serde_json::from_str(&value) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let (Some(alias), Some(id)) = (
            parsed.get("providerAlias").and_then(|v| v.as_str()),
            parsed.get("id").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let desired = format!("{alias}/{id}");
        if desired == key {
            continue;
        }
        conn.execute(
            "INSERT INTO kv(scope, key, value) VALUES('customModels', ?1, ?2)
             ON CONFLICT(scope, key) DO NOTHING",
            rusqlite::params![desired, value],
        )?;
        conn.execute(
            "DELETE FROM kv WHERE scope = 'customModels' AND key = ?1",
            rusqlite::params![key],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        Connection::open_in_memory().expect("in-memory sqlite")
    }

    #[test]
    fn fresh_db_has_version_zero() {
        let conn = fresh();
        // Pre-create _meta since apply_pending_migrations needs it.
        conn.execute_batch("CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 0);
    }

    #[test]
    fn stamping_sets_version() {
        let conn = fresh();
        conn.execute_batch("CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        set_schema_version(&conn, 5).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 5);
    }

    #[test]
    fn apply_pending_brings_to_target() {
        let conn = fresh();
        conn.execute_batch("CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        set_schema_version(&conn, SCHEMA_VERSION - 1).unwrap();
        apply_pending_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    fn custom_models_keys(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT key FROM kv WHERE scope='customModels' ORDER BY key")
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    #[test]
    fn rekey_migrates_legacy_bare_id_rows_exactly_once() {
        let conn = fresh();
        conn.execute_batch(
            "CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE kv(scope TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
                              PRIMARY KEY(scope, key));",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO kv(scope,key,value) VALUES('customModels','deepseek-v4-flash',
             '{\"providerAlias\":\"cerebras\",\"id\":\"deepseek-v4-flash\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO kv(scope,key,value) VALUES('customModels','grok-4',
             '{\"providerAlias\":\"groq\",\"id\":\"grok-4\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO kv(scope,key,value) VALUES('customModels','idx9','not json')",
            [],
        )
        .unwrap();

        apply_pending_migrations(&conn).unwrap();
        assert_eq!(
            custom_models_keys(&conn),
            vec![
                "cerebras/deepseek-v4-flash".to_string(),
                "groq/grok-4".to_string(),
                "idx9".to_string(),
            ],
            "both re-keyable rows migrate; the unparseable one is left alone"
        );

        apply_pending_migrations(&conn).unwrap();
        assert_eq!(
            custom_models_keys(&conn),
            vec![
                "cerebras/deepseek-v4-flash".to_string(),
                "groq/grok-4".to_string(),
                "idx9".to_string(),
            ],
            "second run must be a no-op"
        );
    }

    #[test]
    fn rekey_keeps_existing_composite_row_over_legacy_one() {
        let conn = fresh();
        conn.execute_batch(
            "CREATE TABLE _meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE kv(scope TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
                              PRIMARY KEY(scope, key));",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO kv(scope,key,value) VALUES('customModels','groq/grok-4',
             '{\"providerAlias\":\"groq\",\"id\":\"grok-4\",\"name\":\"current\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO kv(scope,key,value) VALUES('customModels','grok-4',
             '{\"providerAlias\":\"groq\",\"id\":\"grok-4\",\"name\":\"stale\"}')",
            [],
        )
        .unwrap();

        rekey_custom_models(&conn).unwrap();
        assert_eq!(custom_models_keys(&conn), vec!["groq/grok-4".to_string()]);
        let value: String = conn
            .query_row(
                "SELECT value FROM kv WHERE scope='customModels' AND key='groq/grok-4'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            value.contains("current"),
            "the already-migrated row is authoritative and must not be clobbered"
        );
    }

    #[test]
    fn rekey_is_a_noop_without_a_kv_table() {
        let conn = fresh();
        assert!(rekey_custom_models(&conn).is_ok());
    }
}
