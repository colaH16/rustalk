use crate::config::Config;
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

#[derive(Serialize, sqlx::FromRow)]
pub struct Message {
    pub id: i32,
    pub content: String,
    pub sender_pod: String,
    pub sender_nick: String,
    pub sender_color: String,
    pub time: String,
}

pub async fn connect(c: &Config) -> Result<PgPool> {
    let options = PgConnectOptions::new_without_pgpass()
        .host(&c.db_host)
        .port(c.db_port)
        .username(&c.db_user)
        .password(&c.db_password)
        .database(&c.db_name)
        .ssl_mode(c.db_sslmode)
        .application_name("rustalk")
        .disable_statement_logging();
    let statement_timeout = c.timeout.as_millis().to_string();
    let timezone = c.db_timezone.clone();
    let pool = PgPoolOptions::new()
        .max_connections(c.db_max_connections)
        .acquire_timeout(c.timeout)
        .after_connect(move |conn, _| {
            let timeout = statement_timeout.clone();
            let timezone = timezone.clone();
            Box::pin(async move {
                // Bounds queries even when a database lock outlives the HTTP client.
                sqlx::query("SELECT set_config('statement_timeout', $1, false), set_config('TimeZone', $2, false)")
                    .bind(timeout)
                    .bind(timezone)
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .context("PostgreSQL connection failed")?;
    if c.db_init_schema {
        sqlx::raw_sql(include_str!("../schema.sql"))
            .execute(&pool)
            .await
            .context("schema initialization failed")?;
    }
    // Check the full expected projection before advertising readiness; never alter existing data.
    sqlx::query("SELECT m.id, m.content, m.sender_pod, m.sender_nick, m.created_at, u.nickname, u.color_code FROM messages m LEFT JOIN users u ON m.sender_nick=u.nickname LIMIT 0")
        .execute(&pool).await.context("existing GoTalk schema is required (or set DB_INIT_SCHEMA=true for a new database)")?;
    Ok(pool)
}

pub async fn history(pool: &PgPool, before: Option<i64>) -> Result<Vec<Message>, sqlx::Error> {
    sqlx::query_as("SELECT m.id, COALESCE(m.content, '') AS content, COALESCE(m.sender_pod, '') AS sender_pod, COALESCE(m.sender_nick, '') AS sender_nick, COALESCE(u.color_code, '#ffffff') AS sender_color, COALESCE(to_char(m.created_at, 'HH24:MI:SS'), '') AS time FROM messages m LEFT JOIN users u ON m.sender_nick=u.nickname WHERE ($1::bigint IS NULL OR m.id < $1) ORDER BY m.id DESC LIMIT 30")
        .bind(before).fetch_all(pool).await
}

pub const UPSERT: &str = "INSERT INTO users (nickname, color_code) VALUES ($1, $2) ON CONFLICT (nickname) DO UPDATE SET color_code = $2";
