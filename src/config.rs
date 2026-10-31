use anyhow::{Context, Result, bail};
use std::{env, net::SocketAddr, str::FromStr, time::Duration};

// Deliberately not Debug: this struct contains credentials.
pub struct Config {
    pub listen: SocketAddr,
    pub static_dir: String,
    pub db_host: String,
    pub db_port: u16,
    pub db_user: String,
    pub db_password: String,
    pub db_name: String,
    pub db_timezone: String,
    pub db_sslmode: sqlx::postgres::PgSslMode,
    pub db_max_connections: u32,
    pub db_init_schema: bool,
    pub nats_url: String,
    pub nats_subject: String,
    pub pod_name: String,
    pub sse_capacity: usize,
    pub keepalive: Duration,
    pub timeout: Duration,
    pub shutdown_timeout: Duration,
    pub drain_delay: Duration,
    pub max_body_bytes: usize,
}

fn value(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}
fn parse<T: FromStr>(key: &str, default: &str) -> Result<T> {
    value(key, default)
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid {key}"))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let c = Self {
            listen: parse("LISTEN_ADDR", "0.0.0.0:8080")?,
            static_dir: value("STATIC_DIR", "frontend"),
            db_host: value("DB_HOST", "127.0.0.1"),
            db_port: parse("DB_PORT", "5432")?,
            db_user: value("DB_USER", "postgres"),
            db_password: value("DB_PASSWORD", ""),
            db_name: value("DB_NAME", "cotalk"),
            db_timezone: value("DB_TIMEZONE", "UTC"),
            db_sslmode: parse("DB_SSLMODE", "disable")?,
            db_max_connections: parse("DB_MAX_CONNECTIONS", "10")?,
            db_init_schema: parse("DB_INIT_SCHEMA", "false")?,
            nats_url: value("NATS_URL", "nats://127.0.0.1:4222"),
            nats_subject: value("NATS_SUBJECT", "chat.global"),
            pod_name: value("POD_NAME", &value("HOSTNAME", "rustalk")),
            sse_capacity: parse("SSE_BUFFER_CAPACITY", "100")?,
            keepalive: Duration::from_secs(parse("SSE_KEEPALIVE_SECONDS", "15")?),
            timeout: Duration::from_secs(parse("DEPENDENCY_TIMEOUT_SECONDS", "5")?),
            shutdown_timeout: Duration::from_secs(parse("SHUTDOWN_TIMEOUT_SECONDS", "25")?),
            drain_delay: Duration::from_secs(parse("DRAIN_DELAY_SECONDS", "5")?),
            max_body_bytes: parse("MAX_BODY_BYTES", "1048576")?,
        };
        if c.db_max_connections == 0
            || c.db_max_connections > 1000
            || c.sse_capacity == 0
            || c.sse_capacity > 1_000_000
            || c.keepalive.is_zero()
            || c.timeout.is_zero()
            || c.shutdown_timeout.is_zero()
            || c.drain_delay >= c.shutdown_timeout
            || c.max_body_bytes == 0
            || c.max_body_bytes > 16 * 1024 * 1024
        {
            bail!("numeric configuration is outside supported bounds");
        }
        if c.nats_subject
            .split('.')
            .any(|s| s.is_empty() || s.chars().any(|c| c.is_whitespace() || c == '*' || c == '>'))
        {
            bail!("NATS_SUBJECT must be a concrete publish subject");
        }
        std::fs::metadata(format!("{}/index.html", c.static_dir))
            .context("STATIC_DIR/index.html missing")?;
        Ok(c)
    }
}
