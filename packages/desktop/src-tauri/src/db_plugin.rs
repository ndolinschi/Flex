//! Desktop Database UI plugin — connection + schema/table browse + query.
//!
//! Not part of the agent engine: Tauri IPC only, consumed by the right-panel
//! Database tab registered through the frontend UI plugin registry.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

use crate::error::{DesktopError, DesktopResult};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DbEngine {
    Sqlite,
    Postgres,
    Mysql,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbConnectionSpec {
    pub id: String,
    pub name: String,
    pub engine: DbEngine,
    /// SQLite file path, or postgres/mysql URL (`postgres://…`, `mysql://…`).
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbSchemaInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTableInfo {
    pub schema: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub truncated: bool,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbMentionHit {
    pub name: String,
    pub path: String,
    pub insert_text: String,
}

enum LiveConn {
    Sqlite(PathBuf),
    Postgres(String),
    Mysql(String),
}

#[derive(Default)]
pub struct DbPluginState {
    specs: HashMap<String, DbConnectionSpec>,
    active: Option<String>,
    live: HashMap<String, LiveConn>,
}

impl DbPluginState {
    fn persist_path() -> Option<PathBuf> {
        let dir = dirs::data_dir()?.join("agentloop").join("desktop");
        let _ = std::fs::create_dir_all(&dir);
        Some(dir.join("db_connections.json"))
    }

    pub fn load() -> Self {
        let mut state = Self::default();
        let Some(path) = Self::persist_path() else {
            return state;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return state;
        };
        let Ok(specs): Result<Vec<DbConnectionSpec>, _> = serde_json::from_str(&raw) else {
            return state;
        };
        for spec in specs {
            state.specs.insert(spec.id.clone(), spec);
        }
        state
    }

    fn save(&self) {
        let Some(path) = Self::persist_path() else {
            return;
        };
        let list: Vec<&DbConnectionSpec> = self.specs.values().collect();
        if let Ok(raw) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(path, raw);
        }
    }
}

fn db_state(state: &AppState) -> &Mutex<DbPluginState> {
    &state.db_plugin
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_list_connections(
    state: State<'_, AppState>,
) -> DesktopResult<Vec<DbConnectionSpec>> {
    let guard = db_state(&state).lock().await;
    let mut list: Vec<_> = guard.specs.values().cloned().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(list)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_upsert_connection(
    state: State<'_, AppState>,
    spec: DbConnectionSpec,
) -> DesktopResult<DbConnectionSpec> {
    let name = spec.name.trim().to_string();
    let target = spec.target.trim().to_string();
    if name.is_empty() {
        return Err(DesktopError::Message("connection name is required".into()));
    }
    if target.is_empty() {
        return Err(DesktopError::Message("connection target is required".into()));
    }
    let target = normalize_db_target(spec.engine, &target)?;
    let id = if spec.id.trim().is_empty() {
        new_id()
    } else {
        spec.id.trim().to_string()
    };
    let saved = DbConnectionSpec {
        id: id.clone(),
        name,
        engine: spec.engine,
        target,
    };
    let mut guard = db_state(&state).lock().await;
    guard.specs.insert(id, saved.clone());
    guard.save();
    Ok(saved)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_remove_connection(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let mut guard = db_state(&state).lock().await;
    guard.specs.remove(&id);
    guard.live.remove(&id);
    if guard.active.as_deref() == Some(id.as_str()) {
        guard.active = None;
    }
    guard.save();
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_connect(
    state: State<'_, AppState>,
    id: String,
) -> DesktopResult<DbConnectionSpec> {
    let spec = {
        let guard = db_state(&state).lock().await;
        guard
            .specs
            .get(&id)
            .cloned()
            .ok_or_else(|| DesktopError::Message("connection not found".into()))?
    };
    let target = normalize_db_target(spec.engine, &spec.target)?;
    let live = match spec.engine {
        DbEngine::Sqlite => {
            let path = PathBuf::from(&target);
            let conn = rusqlite::Connection::open(&path)
                .map_err(|e| DesktopError::Message(format!("sqlite open failed: {e}")))?;
            drop(conn);
            LiveConn::Sqlite(path)
        }
        DbEngine::Postgres => {
            probe_postgres(&target).await?;
            LiveConn::Postgres(target)
        }
        DbEngine::Mysql => {
            probe_mysql(&target).await?;
            LiveConn::Mysql(target)
        }
    };
    let mut guard = db_state(&state).lock().await;
    guard.live.insert(id.clone(), live);
    guard.active = Some(id);
    Ok(spec)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_disconnect(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let mut guard = db_state(&state).lock().await;
    guard.live.remove(&id);
    if guard.active.as_deref() == Some(id.as_str()) {
        guard.active = None;
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_active_connection(
    state: State<'_, AppState>,
) -> DesktopResult<Option<DbConnectionSpec>> {
    let guard = db_state(&state).lock().await;
    Ok(guard
        .active
        .as_ref()
        .and_then(|id| guard.specs.get(id).cloned()))
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_list_schemas(
    state: State<'_, AppState>,
    id: String,
) -> DesktopResult<Vec<DbSchemaInfo>> {
    match snapshot_live(&state, &id).await? {
        LiveSnap::Sqlite(_) => Ok(vec![DbSchemaInfo {
            name: "main".into(),
        }]),
        LiveSnap::Postgres(url) => list_schemas_postgres(&url).await,
        LiveSnap::Mysql(url) => list_schemas_mysql(&url).await,
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_list_tables(
    state: State<'_, AppState>,
    id: String,
    schema: Option<String>,
) -> DesktopResult<Vec<DbTableInfo>> {
    match snapshot_live(&state, &id).await? {
        LiveSnap::Sqlite(path) => tokio::task::spawn_blocking(move || list_tables_sqlite(&path))
            .await
            .map_err(|e| DesktopError::Message(format!("sqlite join: {e}")))?,
        LiveSnap::Postgres(url) => {
            list_tables_postgres(&url, schema.as_deref().unwrap_or("public")).await
        }
        LiveSnap::Mysql(url) => list_tables_mysql(&url, schema.as_deref()).await,
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_preview_table(
    state: State<'_, AppState>,
    id: String,
    schema: String,
    table: String,
    limit: Option<u32>,
) -> DesktopResult<DbQueryResult> {
    let lim = limit.unwrap_or(100).clamp(1, 500);
    match snapshot_live(&state, &id).await? {
        LiveSnap::Sqlite(path) => {
            let sql = format!("SELECT * FROM {} LIMIT {lim}", quote_ident_sqlite(&table));
            tokio::task::spawn_blocking(move || run_sqlite_query(&path, &sql))
                .await
                .map_err(|e| DesktopError::Message(format!("sqlite join: {e}")))?
        }
        LiveSnap::Postgres(url) => {
            let sql = format!(
                "SELECT * FROM {}.{} LIMIT {lim}",
                quote_ident_pg(&schema),
                quote_ident_pg(&table)
            );
            run_postgres_query(&url, &sql).await
        }
        LiveSnap::Mysql(url) => {
            let qualified = if schema.is_empty() {
                quote_ident_mysql(&table)
            } else {
                format!(
                    "{}.{}",
                    quote_ident_mysql(&schema),
                    quote_ident_mysql(&table)
                )
            };
            let sql = format!("SELECT * FROM {qualified} LIMIT {lim}");
            run_mysql_query(&url, &sql).await
        }
    }
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_query(
    state: State<'_, AppState>,
    id: String,
    sql: String,
) -> DesktopResult<DbQueryResult> {
    let sql = sql.trim().to_string();
    if sql.is_empty() {
        return Err(DesktopError::Message("SQL is empty".into()));
    }
    match snapshot_live(&state, &id).await? {
        LiveSnap::Sqlite(path) => {
            tokio::task::spawn_blocking(move || run_sqlite_query(&path, &sql))
                .await
                .map_err(|e| DesktopError::Message(format!("sqlite join: {e}")))?
        }
        LiveSnap::Postgres(url) => run_postgres_query(&url, &sql).await,
        LiveSnap::Mysql(url) => run_mysql_query(&url, &sql).await,
    }
}

/// Table names from every live connection — for composer `@` suggestions.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn db_mention_tables(
    state: State<'_, AppState>,
    query: String,
) -> DesktopResult<Vec<DbMentionHit>> {
    collect_mentions(&state, query.trim()).await
}

enum LiveSnap {
    Sqlite(PathBuf),
    Postgres(String),
    Mysql(String),
}

impl From<&LiveConn> for LiveSnap {
    fn from(value: &LiveConn) -> Self {
        match value {
            LiveConn::Sqlite(p) => Self::Sqlite(p.clone()),
            LiveConn::Postgres(u) => Self::Postgres(u.clone()),
            LiveConn::Mysql(u) => Self::Mysql(u.clone()),
        }
    }
}

async fn snapshot_live(state: &AppState, id: &str) -> DesktopResult<LiveSnap> {
    let guard = db_state(state).lock().await;
    guard
        .live
        .get(id)
        .map(LiveSnap::from)
        .ok_or_else(|| DesktopError::Message("not connected — open the connection first".into()))
}

async fn collect_mentions(state: &AppState, needle: &str) -> DesktopResult<Vec<DbMentionHit>> {
    let needle = needle.to_lowercase();
    let guard = db_state(state).lock().await;
    let live_ids: Vec<(String, LiveSnap)> = guard
        .live
        .iter()
        .map(|(id, live)| {
            let name = guard
                .specs
                .get(id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.clone());
            (name, LiveSnap::from(live))
        })
        .collect();
    drop(guard);

    let mut hits = Vec::new();
    for (conn_name, live) in live_ids {
        let tables = match live {
            LiveSnap::Sqlite(path) => {
                tokio::task::spawn_blocking(move || list_tables_sqlite(&path))
                    .await
                    .map_err(|e| DesktopError::Message(format!("sqlite join: {e}")))??
            }
            LiveSnap::Postgres(url) => list_tables_postgres(&url, "public").await?,
            LiveSnap::Mysql(url) => list_tables_mysql(&url, None).await?,
        };
        for t in tables {
            let label = if t.schema.is_empty() || t.schema == "main" {
                t.name.clone()
            } else {
                format!("{}.{}", t.schema, t.name)
            };
            if !needle.is_empty()
                && !label.to_lowercase().contains(&needle)
                && !conn_name.to_lowercase().contains(&needle)
            {
                continue;
            }
            hits.push(DbMentionHit {
                name: label.clone(),
                path: format!("{conn_name} · table"),
                insert_text: label,
            });
            if hits.len() >= 30 {
                return Ok(hits);
            }
        }
    }
    Ok(hits)
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("db-{nanos:x}")
}

/// Validate + normalize connection targets so a leftover SQLite path can't be
/// saved under MySQL/Postgres. `mysql2://` is rewritten to `mysql://`.
fn normalize_db_target(engine: DbEngine, target: &str) -> DesktopResult<String> {
    let t = target.trim();
    match engine {
        DbEngine::Sqlite => Ok(t.to_string()),
        DbEngine::Postgres => {
            let lower = t.to_ascii_lowercase();
            if !(lower.starts_with("postgres://") || lower.starts_with("postgresql://")) {
                return Err(DesktopError::Message(
                    "PostgreSQL target must be a URL like \
                     postgres://user:pass@127.0.0.1:5432/dbname"
                        .into(),
                ));
            }
            Ok(t.to_string())
        }
        DbEngine::Mysql => {
            let lower = t.to_ascii_lowercase();
            if lower.starts_with("mysql2://") {
                return Ok(format!("mysql://{}", &t[8..]));
            }
            if !lower.starts_with("mysql://") {
                return Err(DesktopError::Message(
                    "MySQL target must be a URL like \
                     mysql://user:pass@127.0.0.1:3306/dbname \
                     (for Docker Compose use 127.0.0.1 and the published host port, \
                     not the compose service name)"
                        .into(),
                ));
            }
            Ok(t.to_string())
        }
    }
}

fn quote_ident_sqlite(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn quote_ident_pg(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn quote_ident_mysql(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

fn list_tables_sqlite(path: &PathBuf) -> DesktopResult<Vec<DbTableInfo>> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| DesktopError::Message(format!("sqlite open failed: {e}")))?;
    let mut stmt = conn
        .prepare(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|e| DesktopError::Message(format!("sqlite prepare failed: {e}")))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DbTableInfo {
                schema: "main".into(),
                name: row.get::<_, String>(0)?,
                kind: row.get::<_, String>(1)?,
            })
        })
        .map_err(|e| DesktopError::Message(format!("sqlite query failed: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| DesktopError::Message(format!("sqlite row: {e}")))?);
    }
    Ok(out)
}

fn run_sqlite_query(path: &PathBuf, sql: &str) -> DesktopResult<DbQueryResult> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| DesktopError::Message(format!("sqlite open failed: {e}")))?;
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| DesktopError::Message(format!("sqlite prepare failed: {e}")))?;
    let columns: Vec<String> = stmt.column_names().iter().map(|s| (*s).to_string()).collect();
    let col_count = columns.len();
    let mut rows_out: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut truncated = false;
    const MAX_ROWS: usize = 500;
    {
        let mut rows = stmt
            .query([])
            .map_err(|e| DesktopError::Message(format!("sqlite query failed: {e}")))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| DesktopError::Message(format!("sqlite next: {e}")))?
        {
            if rows_out.len() >= MAX_ROWS {
                truncated = true;
                break;
            }
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                vals.push(sqlite_value_to_json(row, i)?);
            }
            rows_out.push(vals);
        }
    }
    let row_count = rows_out.len() as u64;
    Ok(DbQueryResult {
        columns,
        rows: rows_out,
        truncated,
        row_count,
    })
}

fn sqlite_value_to_json(row: &rusqlite::Row<'_>, idx: usize) -> DesktopResult<serde_json::Value> {
    let v = row
        .get_ref(idx)
        .map_err(|e| DesktopError::Message(format!("sqlite get_ref: {e}")))?;
    Ok(match v {
        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
        rusqlite::types::ValueRef::Integer(i) => serde_json::json!(i),
        rusqlite::types::ValueRef::Real(f) => serde_json::json!(f),
        rusqlite::types::ValueRef::Text(t) => {
            serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
        }
        rusqlite::types::ValueRef::Blob(b) => {
            serde_json::Value::String(format!("blob({} bytes)", b.len()))
        }
    })
}

async fn probe_postgres(url: &str) -> DesktopResult<()> {
    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .map_err(|e| DesktopError::Message(format!("postgres connect failed: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .simple_query("SELECT 1")
        .await
        .map_err(|e| DesktopError::Message(format!("postgres probe failed: {e}")))?;
    Ok(())
}

async fn list_schemas_postgres(url: &str) -> DesktopResult<Vec<DbSchemaInfo>> {
    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .map_err(|e| DesktopError::Message(format!("postgres connect failed: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let rows = client
        .query(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog','information_schema','pg_toast') \
             ORDER BY schema_name",
            &[],
        )
        .await
        .map_err(|e| DesktopError::Message(format!("postgres schemas failed: {e}")))?;
    Ok(rows
        .iter()
        .map(|r| DbSchemaInfo {
            name: r.get::<_, String>(0),
        })
        .collect())
}

async fn list_tables_postgres(url: &str, schema: &str) -> DesktopResult<Vec<DbTableInfo>> {
    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .map_err(|e| DesktopError::Message(format!("postgres connect failed: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let rows = client
        .query(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = $1 ORDER BY table_name",
            &[&schema],
        )
        .await
        .map_err(|e| DesktopError::Message(format!("postgres tables failed: {e}")))?;
    Ok(rows
        .iter()
        .map(|r| {
            let kind: String = r.get(1);
            DbTableInfo {
                schema: schema.to_string(),
                name: r.get(0),
                kind: if kind.contains("VIEW") {
                    "view".into()
                } else {
                    "table".into()
                },
            }
        })
        .collect())
}

async fn run_postgres_query(url: &str, sql: &str) -> DesktopResult<DbQueryResult> {
    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .map_err(|e| DesktopError::Message(format!("postgres connect failed: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let rows = client
        .query(sql, &[])
        .await
        .map_err(|e| DesktopError::Message(format!("postgres query failed: {e}")))?;
    if rows.is_empty() {
        return Ok(DbQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: false,
            row_count: 0,
        });
    }
    let columns: Vec<String> = rows[0]
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let mut out = Vec::new();
    let mut truncated = false;
    const MAX_ROWS: usize = 500;
    for row in &rows {
        if out.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let mut vals = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            vals.push(pg_cell_to_json(row, i));
        }
        out.push(vals);
    }
    let row_count = out.len() as u64;
    Ok(DbQueryResult {
        columns,
        rows: out,
        truncated,
        row_count,
    })
}

fn pg_cell_to_json(row: &tokio_postgres::Row, idx: usize) -> serde_json::Value {
    if let Ok(v) = row.try_get::<_, Option<String>>(idx) {
        return match v {
            Some(s) => serde_json::Value::String(s),
            None => serde_json::Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<_, Option<i64>>(idx) {
        return match v {
            Some(n) => serde_json::json!(n),
            None => serde_json::Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<_, Option<f64>>(idx) {
        return match v {
            Some(n) => serde_json::json!(n),
            None => serde_json::Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<_, Option<bool>>(idx) {
        return match v {
            Some(b) => serde_json::json!(b),
            None => serde_json::Value::Null,
        };
    }
    serde_json::Value::String(format!("<{idx}>"))
}

async fn mysql_conn(url: &str) -> DesktopResult<(mysql_async::Pool, mysql_async::Conn)> {
    let opts = mysql_async::Opts::from_url(url)
        .map_err(|e| DesktopError::Message(format!("mysql url invalid: {e}")))?;
    let pool = mysql_async::Pool::new(opts);
    let conn = pool
        .get_conn()
        .await
        .map_err(|e| DesktopError::Message(format!("mysql connect failed: {e}")))?;
    Ok((pool, conn))
}

async fn probe_mysql(url: &str) -> DesktopResult<()> {
    use mysql_async::prelude::*;
    let (pool, mut conn) = mysql_conn(url).await?;
    let _: Vec<mysql_async::Row> = conn
        .query("SELECT 1")
        .await
        .map_err(|e| DesktopError::Message(format!("mysql probe failed: {e}")))?;
    drop(conn);
    let _ = pool.disconnect().await;
    Ok(())
}

async fn list_schemas_mysql(url: &str) -> DesktopResult<Vec<DbSchemaInfo>> {
    use mysql_async::prelude::*;
    let (pool, mut conn) = mysql_conn(url).await?;
    let names: Vec<String> = conn
        .query_map(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema','mysql','performance_schema','sys') \
             ORDER BY schema_name",
            |name: String| name,
        )
        .await
        .map_err(|e| DesktopError::Message(format!("mysql schemas failed: {e}")))?;
    drop(conn);
    let _ = pool.disconnect().await;
    Ok(names
        .into_iter()
        .map(|name| DbSchemaInfo { name })
        .collect())
}

async fn list_tables_mysql(url: &str, schema: Option<&str>) -> DesktopResult<Vec<DbTableInfo>> {
    use mysql_async::prelude::*;
    let (pool, mut conn) = mysql_conn(url).await?;
    let sql = if let Some(schema) = schema {
        format!(
            "SELECT table_schema, table_name, table_type FROM information_schema.tables \
             WHERE table_schema = '{}' ORDER BY table_name",
            schema.replace('\'', "''")
        )
    } else {
        "SELECT table_schema, table_name, table_type FROM information_schema.tables \
         WHERE table_schema = DATABASE() ORDER BY table_name"
            .to_string()
    };
    let rows: Vec<DbTableInfo> = conn
        .query_map(sql, |(schema, name, kind): (String, String, String)| {
            DbTableInfo {
                schema,
                name,
                kind: if kind.to_uppercase().contains("VIEW") {
                    "view".into()
                } else {
                    "table".into()
                },
            }
        })
        .await
        .map_err(|e| DesktopError::Message(format!("mysql tables failed: {e}")))?;
    drop(conn);
    let _ = pool.disconnect().await;
    Ok(rows)
}

async fn run_mysql_query(url: &str, sql: &str) -> DesktopResult<DbQueryResult> {
    use mysql_async::prelude::*;
    let (pool, mut conn) = mysql_conn(url).await?;
    let result = conn
        .query_iter(sql)
        .await
        .map_err(|e| DesktopError::Message(format!("mysql query failed: {e}")))?;
    let columns: Vec<String> = result
        .columns_ref()
        .iter()
        .map(|c| c.name_str().to_string())
        .collect();
    let col_count = columns.len();
    let mut rows_out = Vec::new();
    let mut truncated = false;
    const MAX_ROWS: usize = 500;
    let mut result = result;
    while let Some(row) = result
        .next()
        .await
        .map_err(|e| DesktopError::Message(format!("mysql next: {e}")))?
    {
        if rows_out.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let mut vals = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let v: Option<mysql_async::Value> = row.get(i);
            vals.push(mysql_value_to_json(v));
        }
        rows_out.push(vals);
    }
    drop(conn);
    let _ = pool.disconnect().await;
    let row_count = rows_out.len() as u64;
    Ok(DbQueryResult {
        columns,
        rows: rows_out,
        truncated,
        row_count,
    })
}

fn mysql_value_to_json(v: Option<mysql_async::Value>) -> serde_json::Value {
    match v {
        None | Some(mysql_async::Value::NULL) => serde_json::Value::Null,
        Some(mysql_async::Value::Bytes(b)) => {
            serde_json::Value::String(String::from_utf8_lossy(&b).into_owned())
        }
        Some(mysql_async::Value::Int(i)) => serde_json::json!(i),
        Some(mysql_async::Value::UInt(u)) => serde_json::json!(u),
        Some(mysql_async::Value::Float(f)) => serde_json::json!(f),
        Some(mysql_async::Value::Double(f)) => serde_json::json!(f),
        Some(other) => serde_json::Value::String(format!("{other:?}")),
    }
}
