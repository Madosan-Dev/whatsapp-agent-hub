use crate::models::{Agent, ChatMessage};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::str::FromStr;

/// Creates the SQLite connection pool with WAL mode and auto-creation enabled.
pub async fn create_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
}

/// Automatically creates required multi-tenant tables and indexes on boot.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_slug TEXT UNIQUE NOT NULL,
            evolution_instance TEXT UNIQUE NOT NULL,
            system_prompt TEXT NOT NULL,
            allowed_tools TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tenant_slug TEXT NOT NULL,
            customer_phone TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS paused_chats (
            tenant_slug TEXT NOT NULL,
            customer_phone TEXT NOT NULL,
            paused_until DATETIME NOT NULL,
            PRIMARY KEY (tenant_slug, customer_phone)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_agents_instance
        ON agents (evolution_instance);
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_chat_history_tenant_phone
        ON chat_history (tenant_slug, customer_phone, id DESC);
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("SQLite schema initialized and migrated successfully");
    Ok(())
}

pub async fn find_active_agent_by_instance(
    pool: &SqlitePool,
    instance: &str,
) -> Result<Option<Agent>, sqlx::Error> {
    sqlx::query_as::<_, Agent>(
        r#"
        SELECT id, tenant_slug, evolution_instance, system_prompt, allowed_tools, active, CAST(created_at AS TEXT) AS created_at
        FROM agents
        WHERE evolution_instance = ? AND active = 1
        LIMIT 1
        "#,
    )
    .bind(instance)
    .fetch_optional(pool)
    .await
}

pub async fn is_chat_paused(
    pool: &SqlitePool,
    tenant_slug: &str,
    customer_phone: &str,
) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM paused_chats
        WHERE tenant_slug = ?
          AND customer_phone = ?
          AND paused_until > CURRENT_TIMESTAMP
        "#,
    )
    .bind(tenant_slug)
    .bind(customer_phone)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

pub async fn pause_chat_for_two_hours(
    pool: &SqlitePool,
    tenant_slug: &str,
    customer_phone: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO paused_chats (tenant_slug, customer_phone, paused_until)
        VALUES (?, ?, datetime('now', '+2 hours'))
        ON CONFLICT(tenant_slug, customer_phone)
        DO UPDATE SET paused_until = datetime('now', '+2 hours')
        "#,
    )
    .bind(tenant_slug)
    .bind(customer_phone)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn save_chat_message(
    pool: &SqlitePool,
    tenant_slug: &str,
    customer_phone: &str,
    role: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO chat_history (tenant_slug, customer_phone, role, content)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(tenant_slug)
    .bind(customer_phone)
    .bind(role)
    .bind(content)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetches the last `limit` messages strictly isolated by `(tenant_slug, customer_phone)`
/// and returns them in chronological order (oldest -> newest).
pub async fn get_recent_history(
    pool: &SqlitePool,
    tenant_slug: &str,
    customer_phone: &str,
    limit: i64,
) -> Result<Vec<ChatMessage>, sqlx::Error> {
    let mut rows = sqlx::query_as::<_, ChatMessage>(
        r#"
        SELECT id, tenant_slug, customer_phone, role, content, CAST(created_at AS TEXT) AS created_at
        FROM chat_history
        WHERE tenant_slug = ? AND customer_phone = ?
        ORDER BY id DESC
        LIMIT ?
        "#,
    )
    .bind(tenant_slug)
    .bind(customer_phone)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.reverse();
    Ok(rows)
}

pub async fn list_agents(pool: &SqlitePool) -> Result<Vec<Agent>, sqlx::Error> {
    sqlx::query_as::<_, Agent>(
        r#"
        SELECT id, tenant_slug, evolution_instance, system_prompt, allowed_tools, active, CAST(created_at AS TEXT) AS created_at
        FROM agents
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn upsert_agent(
    pool: &SqlitePool,
    tenant_slug: &str,
    evolution_instance: &str,
    system_prompt: &str,
    allowed_tools_json: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO agents (tenant_slug, evolution_instance, system_prompt, allowed_tools, active)
        VALUES (?, ?, ?, ?, 1)
        ON CONFLICT(tenant_slug) DO UPDATE SET
            evolution_instance = excluded.evolution_instance,
            system_prompt = excluded.system_prompt,
            allowed_tools = excluded.allowed_tools,
            active = 1
        "#,
    )
    .bind(tenant_slug)
    .bind(evolution_instance)
    .bind(system_prompt)
    .bind(allowed_tools_json)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn toggle_agent_status(pool: &SqlitePool, id: i64) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        r#"
        UPDATE agents
        SET active = CASE WHEN active = 1 THEN 0 ELSE 1 END
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(res.rows_affected())
}

pub async fn delete_agent(pool: &SqlitePool, id: i64) -> Result<u64, sqlx::Error> {
    let res = sqlx::query("DELETE FROM agents WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(res.rows_affected())
}
