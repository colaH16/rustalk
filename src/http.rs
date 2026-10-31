use crate::{App, db};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{Method, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{any, get},
};
use std::{collections::HashMap, convert::Infallible, sync::atomic::Ordering};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir};

type ApiResult = Result<Response, StatusCode>;

pub fn router(app: App) -> Router {
    Router::new()
        .route("/login", any(login))
        .route("/update", any(update))
        .route("/history", any(history))
        .route("/send", any(send))
        .route("/stream", get(stream))
        .route("/livez", get(|| async { "ok\n" }))
        .route("/readyz", get(ready))
        .route("/dependencies", get(dependencies))
        .fallback_service(ServeDir::new(&app.config.static_dir))
        .layer(RequestBodyLimitLayer::new(app.config.max_body_bytes))
        .with_state(app)
}

fn fields(encoded: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (key, value) in url::form_urlencoded::parse(encoded.as_bytes()) {
        // Go FormValue uses the first value, not the last duplicate.
        result.entry(key.into_owned()).or_insert(value.into_owned());
    }
    result
}

async fn form(request: Request, limit: usize) -> Result<HashMap<String, String>, StatusCode> {
    let query = fields(request.uri().query().unwrap_or_default());
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if content_type.starts_with("multipart/") {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let mut body_fields = if content_type == "application/x-www-form-urlencoded" {
        let bytes = to_bytes(request.into_body(), limit)
            .await
            .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
        let body = std::str::from_utf8(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
        fields(body)
    } else {
        HashMap::new()
    };
    for (key, value) in query {
        body_fields.entry(key).or_insert(value);
    }
    Ok(body_fields)
}

fn field<'a>(f: &'a HashMap<String, String>, name: &str) -> &'a str {
    f.get(name).map(String::as_str).unwrap_or_default()
}
fn color(f: &HashMap<String, String>) -> &str {
    match field(f, "color") {
        c if c.len() == 7
            && c.starts_with('#')
            && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) =>
        {
            c
        }
        _ => "#ffffff",
    }
}
fn database_error(error: sqlx::Error) -> StatusCode {
    // Do not log database error text: constraint errors can contain private values.
    let kind = match &error {
        sqlx::Error::Database(_) => "database",
        sqlx::Error::PoolTimedOut => "pool_timeout",
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::Io(_) => "io",
        _ => "other",
    };
    let code = error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|v| v.into_owned());
    tracing::error!(
        kind,
        sqlstate = code.as_deref().unwrap_or(""),
        "database operation failed"
    );
    StatusCode::SERVICE_UNAVAILABLE
}

async fn login(State(app): State<App>, request: Request) -> ApiResult {
    let f = fields(request.uri().query().unwrap_or_default());
    let nick = field(&f, "nick");
    let color: Option<Option<String>> =
        sqlx::query_scalar("SELECT color_code FROM users WHERE nickname=$1")
            .bind(nick)
            .fetch_optional(&app.pool)
            .await
            .map_err(database_error)?;
    Ok(Json(
        serde_json::json!({"nickname": nick, "color_code": color.flatten().unwrap_or_default()}),
    )
    .into_response())
}

async fn update(State(app): State<App>, request: Request) -> ApiResult {
    if request.method() != Method::POST {
        return Ok(StatusCode::OK.into_response());
    }
    let f = form(request, app.config.max_body_bytes).await?;
    let nick = field(&f, "nick");
    if nick.is_empty() {
        return Ok(StatusCode::OK.into_response());
    }
    sqlx::query(db::UPSERT)
        .bind(nick)
        .bind(color(&f))
        .execute(&app.pool)
        .await
        .map_err(database_error)?;
    Ok(StatusCode::OK.into_response())
}

async fn history(State(app): State<App>, request: Request) -> ApiResult {
    let f = fields(request.uri().query().unwrap_or_default());
    let before = match field(&f, "before_id") {
        "" => None,
        v => Some(v.parse::<i64>().unwrap_or(0)),
    };
    let messages = db::history(&app.pool, before)
        .await
        .map_err(database_error)?;
    // Preserve Go's nil-slice response (null) when there are no messages.
    Ok(Json(if messages.is_empty() {
        None
    } else {
        Some(messages)
    })
    .into_response())
}

async fn send(State(app): State<App>, request: Request) -> ApiResult {
    if request.method() != Method::POST {
        return Ok(StatusCode::OK.into_response());
    }
    let f = form(request, app.config.max_body_bytes).await?;
    let nick = field(&f, "nick");
    let content = field(&f, "msg");
    if nick.is_empty() || content.is_empty() {
        return Ok(StatusCode::OK.into_response());
    }
    if app.shutdown.is_cancelled() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let mut tx = app.pool.begin().await.map_err(database_error)?;
    sqlx::query(db::UPSERT)
        .bind(nick)
        .bind(color(&f))
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    let (id, time): (i32, String) = sqlx::query_as(
        "INSERT INTO messages (content, sender_pod, sender_nick) VALUES ($1, $2, $3) RETURNING id, to_char(created_at, 'HH24:MI:SS')",
    )
    .bind(content)
    .bind(&app.config.pod_name)
    .bind(nick)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    let message = db::Message {
        id,
        content: content.into(),
        sender_pod: app.config.pod_name.clone(),
        sender_nick: nick.into(),
        sender_color: color(&f).into(),
        time,
    };
    if !app.connected() {
        tracing::warn!(
            message_id = id,
            "message stored while live delivery is disconnected"
        );
        return Ok(StatusCode::OK.into_response());
    }
    let data = serde_json::to_vec(&message).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let published = tokio::time::timeout(app.config.timeout, async {
        app.nats
            .publish(app.config.nats_subject.clone(), data.into())
            .await
            .map_err(|_| ())?;
        app.nats.flush().await.map_err(|_| ())
    })
    .await;
    if !matches!(published, Ok(Ok(()))) {
        // The DB has committed. Returning a retryable error would encourage duplicates.
        // Core NATS is best effort; history remains the durable source of truth.
        tracing::warn!(
            message_id = id,
            "message stored; live delivery not confirmed"
        );
    }
    Ok(StatusCode::OK.into_response())
}

async fn ready(State(app): State<App>) -> StatusCode {
    // Startup already checks schema and subscriptions. A shared dependency outage
    // must not remove every replica and make static/history routes unreachable.
    if app.shutdown.is_cancelled() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}

async fn dependencies(State(app): State<App>) -> StatusCode {
    if !app.connected() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    match tokio::time::timeout(
        app.config.timeout,
        sqlx::query("SELECT 1").execute(&app.pool),
    )
    .await
    {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn stream(State(app): State<App>) -> ApiResult {
    if !app.connected() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let mut receiver = app.messages.subscribe();
    let keepalive = app.config.keepalive;
    let mut nats_epoch = app.nats_epoch.clone();
    nats_epoch.borrow_and_update();
    let events = async_stream::stream! {
        // An immediate comment flushes headers so clients can begin a race-free history catch-up.
        yield Ok::<_, Infallible>(Event::default().comment("connected"));
        loop {
            tokio::select! {
                biased;
                _ = app.shutdown.cancelled() => break,
                _ = nats_epoch.changed() => break,
                message = receiver.recv() => match message {
                    Ok(data) => yield Ok(Event::default().data(data.as_ref())),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!(count, "slow SSE consumer reconnecting");
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = tokio::time::sleep(keepalive) => {
                    if !app.subscribed.load(Ordering::Acquire) || !app.connected() { break; }
                }
            }
        }
    };
    Ok(Sse::new(events)
        .keep_alive(KeepAlive::new().interval(keepalive).text("keepalive"))
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn form_keeps_first_value_and_decodes_unicode() {
        let f = fields("nick=%ED%95%9C%EA%B8%80&nick=second&msg=a%2Bb+c");
        assert_eq!(field(&f, "nick"), "한글");
        assert_eq!(field(&f, "msg"), "a+b c");
        assert_eq!(color(&f), "#ffffff");
    }
    #[test]
    fn color_rejects_extra_css_and_database_nul() {
        assert_eq!(color(&fields("color=%23aBcD09")), "#aBcD09");
        assert_eq!(color(&fields("color=red%3Bposition%3Afixed")), "#ffffff");
        assert_eq!(color(&fields("color=%00")), "#ffffff");
    }
    #[tokio::test]
    async fn post_body_precedes_query_like_go_formvalue() {
        let request = Request::builder()
            .uri("/send?nick=query&color=%23123456")
            .header(
                "content-type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .body(axum::body::Body::from("nick=body&nick=ignored&msg=hi"))
            .unwrap();
        let f = form(request, 1024).await.unwrap();
        assert_eq!(field(&f, "nick"), "body");
        assert_eq!(color(&f), "#123456");
    }
}
