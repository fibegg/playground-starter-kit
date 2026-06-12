use crate::{
    config::AppConfig,
    models::{CurrentUser, UserRow},
    web::{AppState, WebState},
};
use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SESSION_COOKIE: &str = "uptime_session";
const SESSION_TTL_SECONDS: u64 = 60 * 60 * 24 * 7;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub user: Option<CurrentUser>,
}

pub async fn login(
    State(state): State<WebState>,
    headers: HeaderMap,
    Json(input): Json<LoginRequest>,
) -> Response {
    match login_inner(&state.app, &headers, input).await {
        Ok((user, cookie)) => {
            let mut response = Json(SessionResponse { user: Some(user) }).into_response();
            response.headers_mut().insert(header::SET_COOKIE, cookie);
            response
        }
        Err(_) => (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid credentials" })),
        )
            .into_response(),
    }
}

pub async fn logout(State(state): State<WebState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        let _ = delete_session(&state.app.redis, &state.app.config, &token).await;
    }
    let cookie = clear_cookie(&state.app.config);
    let mut response = Json(SessionResponse { user: None }).into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
}

pub async fn session(State(state): State<WebState>, headers: HeaderMap) -> Json<SessionResponse> {
    let user = current_user_from_headers(&state.app, &headers)
        .await
        .ok()
        .flatten();
    Json(SessionResponse { user })
}

pub async fn current_user_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>> {
    let Some(token) = session_token(headers) else {
        return Ok(None);
    };
    let Some(user_id) = session_user_id(&state.redis, &state.config, &token).await? else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, name, role, password_hash FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.map(CurrentUser::from))
}

async fn login_inner(
    state: &AppState,
    headers: &HeaderMap,
    input: LoginRequest,
) -> Result<(CurrentUser, HeaderValue)> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, name, role, password_hash FROM users WHERE email = $1",
    )
    .bind(input.email.trim().to_lowercase())
    .fetch_one(&state.pool)
    .await?;

    let parsed_hash = PasswordHash::new(&row.password_hash)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Argon2::default()
        .verify_password(input.password.as_bytes(), &parsed_hash)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let user = CurrentUser::from(row);
    let token = Uuid::new_v4().to_string();
    store_session(&state.redis, &state.config, &token, user.id).await?;
    Ok((user, session_cookie(&state.config, headers, &token)))
}

async fn store_session(
    client: &redis::Client,
    config: &AppConfig,
    token: &str,
    user_id: Uuid,
) -> Result<()> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _: () = conn
        .set_ex(
            session_key(config, token),
            user_id.to_string(),
            SESSION_TTL_SECONDS,
        )
        .await?;
    Ok(())
}

async fn session_user_id(
    client: &redis::Client,
    config: &AppConfig,
    token: &str,
) -> Result<Option<Uuid>> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let value: Option<String> = conn.get(session_key(config, token)).await?;
    Ok(value.and_then(|raw| Uuid::parse_str(&raw).ok()))
}

async fn delete_session(client: &redis::Client, config: &AppConfig, token: &str) -> Result<()> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _: () = conn.del(session_key(config, token)).await?;
    Ok(())
}

fn session_key(config: &AppConfig, token: &str) -> String {
    format!(
        "session:{}:{token}",
        stable_secret_fingerprint(&config.secret)
    )
}

fn stable_secret_fingerprint(secret: &str) -> u64 {
    secret
        .as_bytes()
        .iter()
        .fold(14_695_981_039_346_656_037_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211)
        })
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| (key == SESSION_COOKIE).then(|| value.to_string()))
}

fn session_cookie(config: &AppConfig, headers: &HeaderMap, token: &str) -> HeaderValue {
    let request_is_https = config.trust_proxy_headers
        && headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|proto| proto.eq_ignore_ascii_case("https"));
    let secure = config.cookie_secure_enabled(request_is_https);
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite={}",
        config.cookie_same_site
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn clear_cookie(config: &AppConfig) -> HeaderValue {
    let cookie = format!(
        "{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite={}",
        config.cookie_same_site
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}
