use async_graphql::{Request, Value};
use rust_axum_react_starter_kit::{
    config::{AppConfig, CookieSecure, CspMode},
    db, graphql,
    models::CurrentUser,
    web::AppState,
};
use sqlx::postgres::PgPoolOptions;
use std::{env, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use uuid::Uuid;

fn admin_user() -> CurrentUser {
    CurrentUser {
        id: Uuid::new_v4(),
        email: "admin@example.com".to_string(),
        name: "Admin".to_string(),
        role: "admin".to_string(),
    }
}

fn base_config() -> AppConfig {
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

fn lazy_state(config: AppConfig) -> AppState {
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

async fn database_state() -> Option<AppState> {
    if env::var("RUN_DATABASE_TESTS").ok().as_deref() != Some("1") {
        return None;
    }

    let mut config = base_config();
    config.database_url = env::var("DATABASE_URL").ok()?;
    config.redis_url = env::var("REDIS_URL").ok()?;
    db::setup(config.clone()).await.ok()?;
    AppState::new(config).await.ok()
}

#[tokio::test]
async fn graphql_blocks_private_monitor_url_before_database_work() {
    let schema = graphql::build_schema(lazy_state(base_config()));
    let request = Request::new(
        r#"
        mutation CreateMonitor($input: MonitorInput!) {
          createMonitor(input: $input) { id }
        }
        "#,
    )
    .variables(async_graphql::Variables::from_json(serde_json::json!({
        "input": {
            "name": "Internal",
            "url": "http://127.0.0.1:3000/up"
        }
    })))
    .data(admin_user());

    let response = schema.execute(request).await;
    assert_eq!(response.data, Value::Null);
    assert!(response.errors.iter().any(|error| {
        error.message.contains("public host") || error.message.contains("private")
    }));
}

#[tokio::test]
async fn graphql_missing_resource_ids_return_not_found() {
    let Some(state) = database_state().await else {
        return;
    };
    let schema = graphql::build_schema(state);
    let missing_id = Uuid::new_v4();
    let request = Request::new(
        r#"
        mutation ResolveIncident($id: UUID!) {
          resolveIncident(id: $id) { id }
        }
        "#,
    )
    .variables(async_graphql::Variables::from_json(serde_json::json!({
        "id": missing_id
    })))
    .data(admin_user());

    let response = schema.execute(request).await;
    assert_eq!(response.data, Value::Null);
    assert!(
        response
            .errors
            .iter()
            .any(|error| error.message == "not found")
    );
    assert!(
        !response
            .errors
            .iter()
            .any(|error| error.message.contains("sql") || error.message.contains("database"))
    );
}
