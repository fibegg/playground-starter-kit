use crate::{config::CspMode, web::AppState};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
};
use redis::AsyncCommands;

pub async fn host_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.allows_all_hosts() {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(':').next())
            .unwrap_or_default();
        if !state
            .config
            .allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
        {
            return Err(StatusCode::MISDIRECTED_REQUEST);
        }
    }
    Ok(next.run(request).await)
}

pub async fn rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();
    if path == "/up" || path == "/readyz" || path.starts_with("/assets/") {
        return Ok(next.run(request).await);
    }

    let key = rate_limit_key(&request, state.config.trust_proxy_headers);
    let limit = if path.starts_with("/auth/") { 30 } else { 240 };
    let redis_key = format!("rate:{key}:{}", chrono::Utc::now().timestamp() / 60);
    let mut conn = state
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let count: i64 = conn
        .incr(&redis_key, 1)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if count == 1 {
        let _: () = conn
            .expire(&redis_key, 75)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    }
    if count > limit {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

pub async fn security_headers(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );

    if state.config.hsts {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    if let Some(value) = &state.config.x_frame_options
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        headers.insert("x-frame-options", header_value);
    }

    if let Some(csp) = csp_value(&state) {
        let header_name = match state.config.csp_mode {
            CspMode::Off => return response,
            CspMode::ReportOnly => "content-security-policy-report-only",
            CspMode::Enforce => "content-security-policy",
        };
        if let Ok(header_value) = HeaderValue::from_str(&csp) {
            headers.insert(header_name, header_value);
        }
    }

    if state.config.allows_all_origins() {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
    }

    response
}

pub fn cors_origin(headers: &HeaderMap, state: &AppState) -> Option<HeaderValue> {
    if state.config.allows_all_origins() {
        return Some(HeaderValue::from_static("*"));
    }
    let origin = headers.get(header::ORIGIN)?.to_str().ok()?;
    state
        .config
        .cors_allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
        .then(|| HeaderValue::from_str(origin).ok())
        .flatten()
}

fn csp_value(state: &AppState) -> Option<String> {
    if state.config.csp_mode == CspMode::Off {
        return None;
    }
    let frame_ancestors = state
        .config
        .frame_ancestors
        .clone()
        .unwrap_or_else(|| "*".to_string());
    let connect_src = state
        .config
        .csp_connect_src
        .clone()
        .or_else(|| {
            state
                .config
                .public_origin
                .as_ref()
                .map(|origin| format!("'self' {origin}"))
        })
        .unwrap_or_else(|| "'self' http: https: ws: wss:".to_string());
    Some(format!(
        "default-src 'self'; base-uri 'self'; frame-ancestors {frame_ancestors}; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src {connect_src}"
    ))
}

fn rate_limit_key(request: &Request<Body>, trust_proxy_headers: bool) -> String {
    let forwarded_for = trust_proxy_headers
        .then(|| {
            request
                .headers()
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .flatten();

    forwarded_for.unwrap_or("local").replace(':', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AppConfig, CookieSecure, CspMode},
        web::AppState,
    };
    use axum::http::Request;
    use sqlx::postgres::PgPoolOptions;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::broadcast;

    fn config() -> AppConfig {
        AppConfig {
            env: "test".to_string(),
            port: 3000,
            secret: "test-secret".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            db_max_connections: 1,
            db_min_connections: 0,
            db_acquire_timeout: Duration::from_millis(100),
            redis_url: "redis://localhost:6379/0".to_string(),
            allowed_hosts: vec!["*".to_string()],
            cors_allowed_origins: vec!["*".to_string()],
            public_origin: None,
            trust_proxy_headers: false,
            force_https: false,
            assume_https: false,
            hsts: false,
            frame_ancestors: None,
            x_frame_options: None,
            csp_mode: CspMode::Off,
            csp_connect_src: None,
            cookie_secure: CookieSecure::Never,
            cookie_same_site: "lax".to_string(),
            demo_password: "password".to_string(),
            allow_private_monitor_urls: false,
        }
    }

    fn state(config: AppConfig) -> AppState {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/test")
            .unwrap();
        let redis = redis::Client::open("redis://localhost:6379/0").unwrap();
        let (events, _) = broadcast::channel(1);
        AppState {
            config: Arc::new(config),
            pool,
            redis,
            events,
        }
    }

    #[tokio::test]
    async fn csp_uses_configurable_frame_ancestors_and_connect_src() {
        let mut config = config();
        config.csp_mode = CspMode::Enforce;
        config.frame_ancestors = Some("'self' https://*.example.com".to_string());
        config.csp_connect_src = Some("'self' https://api.example.com".to_string());
        let value = csp_value(&state(config)).unwrap();
        assert!(value.contains("frame-ancestors 'self' https://*.example.com"));
        assert!(value.contains("connect-src 'self' https://api.example.com"));
    }

    #[test]
    fn rate_limit_ignores_forwarded_for_when_proxy_headers_are_untrusted() {
        let request = Request::builder()
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        assert_eq!(rate_limit_key(&request, false), "local");
        assert_eq!(rate_limit_key(&request, true), "203.0.113.10");
    }
}
