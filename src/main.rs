mod config;
mod db;
mod http;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct App {
    config: Arc<config::Config>,
    pool: sqlx::PgPool,
    nats: async_nats::Client,
    messages: broadcast::Sender<Arc<str>>,
    subscribed: Arc<AtomicBool>,
    shutdown: CancellationToken,
    nats_epoch: tokio::sync::watch::Receiver<u64>,
}
impl App {
    fn connected(&self) -> bool {
        !self.shutdown.is_cancelled()
            && self.subscribed.load(Ordering::Acquire)
            && self.nats.connection_state() == async_nats::connection::State::Connected
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustalk=info,warn".into()),
        )
        .init();
    if let Err(error) = run().await {
        // Only outer context is displayed: nested connection errors can contain credentials/hosts.
        tracing::error!(reason = %error, "startup or server failed");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    if std::env::args_os().len() != 1 {
        anyhow::bail!(
            "runtime configuration uses environment variables; CLI arguments are not supported"
        );
    }
    let config = Arc::new(config::Config::from_env()?);
    let pool = db::connect(&config).await?;
    let (nats_changes, nats_epoch) = tokio::sync::watch::channel(0u64);
    let nats = tokio::time::timeout(
        config.timeout,
        async_nats::ConnectOptions::new()
            .name("Rustalk")
            .event_callback(move |event| {
                let changes = nats_changes.clone();
                async move {
                    if matches!(
                        event,
                        async_nats::Event::Disconnected | async_nats::Event::Connected
                    ) {
                        changes.send_modify(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
            })
            .max_reconnects(None)
            .connection_timeout(config.timeout)
            .connect(config.nats_url.clone()),
    )
    .await
    .context("NATS connection timed out")?
    .map_err(|_| anyhow::anyhow!("NATS connection failed"))?;
    let mut subscription = nats
        .subscribe(config.nats_subject.clone())
        .await
        .context("NATS subscription failed")?;
    tokio::time::timeout(config.timeout, nats.flush())
        .await
        .context("NATS subscription flush timed out")?
        .context("NATS subscription flush failed")?;
    let (messages, _) = broadcast::channel(config.sse_capacity);
    let app = App {
        config: config.clone(),
        pool,
        nats,
        messages,
        subscribed: Arc::new(AtomicBool::new(true)),
        shutdown: CancellationToken::new(),
        nats_epoch,
    };
    let subscriber_app = app.clone();
    let subscriber = tokio::spawn(async move {
        while let Some(message) = subscription.next().await {
            if let Ok(data) = std::str::from_utf8(&message.payload) {
                let _ = subscriber_app.messages.send(Arc::from(data));
            }
        }
        subscriber_app.subscribed.store(false, Ordering::Release);
        tracing::error!("NATS subscription ended; shutting down");
        subscriber_app.shutdown.cancel();
    });
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .context("HTTP bind failed")?;
    tracing::info!(listen = %config.listen, "Rustalk ready");
    let shutdown = app.shutdown.clone();
    let signal_shutdown = shutdown.clone();
    let signal = tokio::spawn(async move {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        signal_shutdown.cancel();
    });
    let server = axum::serve(listener, http::router(app.clone()))
        .with_graceful_shutdown(shutdown.clone().cancelled_owned());
    // Serve implements IntoFuture, not Future.
    let serving = std::future::IntoFuture::into_future(server);
    tokio::pin!(serving);
    tokio::select! {
        result = &mut serving => result.context("HTTP server failed")?,
        _ = shutdown.cancelled() => {
            tracing::info!("draining HTTP connections");
            match tokio::time::timeout(config.shutdown_timeout, &mut serving).await {
                Ok(result) => result.context("HTTP drain failed")?,
                Err(_) => tracing::warn!("HTTP drain deadline reached"),
            }
        }
    }
    subscriber.abort();
    signal.abort();
    Ok(())
}
