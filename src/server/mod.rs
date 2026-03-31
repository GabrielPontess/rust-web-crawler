use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{StreamExt, stream::Stream};
use serde::Deserialize;
use serde::Serialize;
use serde_json;
use tokio::net::TcpListener;
use tokio_stream;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;
use url::form_urlencoded;

use crate::config::AppConfig;
use crate::db::Database;
use crate::events::EventBus;
use crate::models::{EventLog, PageSummary, QueueItem, SearchResult};

#[derive(Clone)]
struct ApiState {
    db: Database,
    config: Arc<AppConfig>,
    events: Option<EventBus>,
}

pub async fn run_server(
    config: Arc<AppConfig>,
    db: Database,
    events: Option<EventBus>,
    addr: SocketAddr,
) -> Result<()> {
    let state = ApiState {
        db,
        config: config.clone(),
        events,
    };

    let router = Router::new()
        .route("/api/metrics", get(get_metrics))
        .route("/api/queue", get(get_queue))
        .route("/api/events", get(get_events))
        .route("/api/search", get(search))
        .route("/api/page", get(page_detail))
        .route("/stream/events", get(stream_events))
        .with_state(state.clone())
        .layer(from_fn_with_state(state.clone(), auth_guard))
        .layer(CorsLayer::permissive());

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

async fn auth_guard(
    State(state): State<ApiState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(expected) = &state.config.api_token {
        let provided_header = req
            .headers()
            .get("x-api-key")
            .and_then(|value: &HeaderValue| value.to_str().ok());

        let provided_query = req.uri().query().and_then(|query| {
            for (key, value) in form_urlencoded::parse(query.as_bytes()) {
                if key == "token" {
                    return Some(value.into_owned());
                }
            }
            None
        });

        if provided_header.map(|p| p == expected).unwrap_or(false)
            || provided_query.as_deref() == Some(expected)
        {
            return Ok(next.run(req).await);
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(next.run(req).await)
}

async fn get_metrics(State(state): State<ApiState>) -> Result<Json<MetricsResponse>, StatusCode> {
    let total_pages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pages")
        .fetch_one(state.db.pool())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue WHERE status = 'pending'")
        .fetch_one(state.db.pool())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let processing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM queue WHERE status = 'processing'")
            .fetch_one(state.db.pool())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue WHERE status = 'failed'")
        .fetch_one(state.db.pool())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MetricsResponse {
        total_pages,
        queue_pending: pending,
        queue_processing: processing,
        queue_failed: failed,
    }))
}

async fn get_queue(
    State(state): State<ApiState>,
    Query(params): Query<QueueParams>,
) -> Result<Json<Vec<QueueItem>>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200) as i64;
    let offset = params.offset.unwrap_or(0) as i64;
    state
        .db
        .queue_items(
            params.status.as_deref(),
            params.host.as_deref(),
            limit,
            offset,
        )
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_events(
    State(state): State<ApiState>,
    Query(params): Query<EventsParams>,
) -> Result<Json<Vec<EventLog>>, StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500) as i64;
    state
        .db
        .recent_events(limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn search(
    State(state): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let query = params.query.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
    let limit = params.limit.unwrap_or(10);
    state
        .db
        .search(query, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn page_detail(
    State(state): State<ApiState>,
    Query(params): Query<PageParams>,
) -> Result<Json<PageResponse>, StatusCode> {
    let url = params.url.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
    let detail = state
        .db
        .page_detail(url)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(PageResponse::from(detail)))
}

async fn stream_events(
    State(state): State<ApiState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;
    let stream: EventStream = if let Some(bus) = &state.events {
        let recv = BroadcastStream::new(bus.subscribe());
        Box::pin(recv.filter_map(|event| async move {
            match event {
                Ok(ev) => {
                    let json = serde_json::to_string(&ev).ok()?;
                    Some(Ok(Event::default().data(json)))
                }
                Err(_) => None,
            }
        }))
    } else {
        Box::pin(tokio_stream::empty::<Result<Event, Infallible>>())
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Debug, Deserialize)]
struct QueueParams {
    status: Option<String>,
    host: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct EventsParams {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    query: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PageParams {
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct MetricsResponse {
    total_pages: i64,
    queue_pending: i64,
    queue_processing: i64,
    queue_failed: i64,
}

#[derive(Debug, Serialize)]
struct PageResponse {
    url: String,
    title: Option<String>,
    description: Option<String>,
    headings: Vec<String>,
    content: Option<String>,
    summary: Option<String>,
    lang: Option<String>,
    crawled_at: Option<String>,
}

impl From<PageSummary> for PageResponse {
    fn from(summary: PageSummary) -> Self {
        let headings = summary
            .headings
            .map(|h| h.split('\n').map(|s| s.to_string()).collect())
            .unwrap_or_default();
        Self {
            url: summary.url,
            title: summary.title,
            description: summary.description,
            headings,
            content: summary.content,
            summary: summary.summary,
            lang: summary.lang,
            crawled_at: summary.crawled_at.map(|ts| ts.to_rfc3339()),
        }
    }
}
