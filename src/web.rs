use crate::{auth, config::AppConfig, db, graphql, jobs, security};
use anyhow::Result;
use async_graphql::http::{GraphQLPlaygroundConfig, playground_source};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    middleware,
    response::{
        Html, IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures_util::StreamExt;
use serde_json::json;
use sqlx::PgPool;
use std::{
    convert::Infallible,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::{fs, net::TcpListener, sync::broadcast};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub pool: PgPool,
    pub redis: redis::Client,
    pub events: broadcast::Sender<String>,
}

#[derive(Clone)]
pub struct WebState {
    pub app: AppState,
    schema: graphql::AppSchema,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let pool = db::connect(&config).await?;
        let redis = redis::Client::open(config.redis_url.clone())?;
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            config: Arc::new(config),
            pool,
            redis,
            events,
        })
    }
}

pub async fn serve(config: AppConfig) -> Result<()> {
    let app_state = AppState::new(config).await?;
    tokio::spawn(jobs::forward_events_from_redis(app_state.clone()));

    let schema = graphql::build_schema(app_state.clone());
    let web_state = WebState {
        app: app_state.clone(),
        schema,
    };

    let router = Router::new()
        .route("/up", get(up))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/events", get(events))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/session", get(auth::session))
        .route("/graphql", get(graphiql).post(graphql_handler))
        .fallback(spa)
        .with_state(web_state.clone())
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            security::security_headers,
        ))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            security::rate_limit,
        ))
        .layer(middleware::from_fn_with_state(
            app_state,
            security::host_guard,
        ))
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(web_state.app.config.bind_addr()).await?;
    tracing::info!(addr = %web_state.app.config.bind_addr(), "serving web");
    axum::serve(listener, router).await?;
    Ok(())
}

async fn up() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn readyz(State(state): State<WebState>) -> Response {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.app.pool)
        .await
        .is_ok();
    let redis_ok = state
        .app
        .redis
        .get_multiplexed_async_connection()
        .await
        .is_ok();
    let status = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({ "database": db_ok, "redis": redis_ok })),
    )
        .into_response()
}

async fn metrics() -> &'static str {
    "# HELP uptime_console_info Starter app info\n# TYPE uptime_console_info gauge\nuptime_console_info 1\n"
}

async fn events(
    State(state): State<WebState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let stream =
        BroadcastStream::new(state.app.events.subscribe()).filter_map(|event| async move {
            event
                .ok()
                .map(|payload| Ok(Event::default().event("message").data(payload)))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn graphiql(State(state): State<WebState>) -> Response {
    if state.app.config.is_development() {
        Html(playground_source(GraphQLPlaygroundConfig::new("/graphql"))).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn graphql_handler(
    State(state): State<WebState>,
    headers: HeaderMap,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let mut request = request.into_inner();
    if let Ok(Some(user)) = auth::current_user_from_headers(&state.app, &headers).await {
        request = request.data(user);
    }
    state.schema.execute(request).await.into()
}

async fn spa(State(state): State<WebState>, headers: HeaderMap, uri: axum::http::Uri) -> Response {
    if uri.path().starts_with("/api/") || uri.path().starts_with("/auth/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let dist = Path::new("frontend/dist");
    let path = match safe_spa_path(dist, uri.path()) {
        Some(path) if path.is_file() => path,
        Some(_) => dist.join("index.html"),
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let body = match fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return Html(
                r#"<!doctype html><title>Uptime Console</title><main style="font-family:system-ui;padding:2rem"><h1>Uptime Console API is running</h1><p>Start the Vite frontend or build frontend assets.</p></main>"#,
            )
            .into_response();
        }
    };

    let content_type = content_type(&path);
    let mut response = ([(header::CONTENT_TYPE, content_type)], body).into_response();
    if let Some(origin) = security::cors_origin(&headers, &state.app) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response
}

fn safe_spa_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let relative = request_path.trim_start_matches('/');
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    Some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "text/html; charset=utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_spa_path_rejects_parent_segments() {
        assert!(safe_spa_path(Path::new("frontend/dist"), "/../Cargo.toml").is_none());
    }

    #[test]
    fn safe_spa_path_allows_normal_assets() {
        let path = safe_spa_path(Path::new("frontend/dist"), "/assets/app.js").unwrap();
        assert_eq!(path, Path::new("frontend/dist/assets/app.js"));
    }
}
