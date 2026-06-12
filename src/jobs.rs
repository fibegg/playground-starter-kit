use crate::{
    cache,
    config::AppConfig,
    db,
    models::{JobRun, Monitor},
    safety,
    web::AppState,
};
use anyhow::{Context, Result};
use apalis::layers::WorkerBuilderExt;
use apalis::prelude::{Data, Monitor as ApalisMonitor, Storage, WorkerBuilder, WorkerFactoryFn};
use apalis_redis::RedisStorage;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio::time;
use uuid::Uuid;

const APP_EVENTS_CHANNEL: &str = "uptime_console:events";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AppJob {
    CheckMonitor { monitor_id: Uuid },
    RebuildRollups,
    ClearCache,
    PruneEvents,
    ReseedDemo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct QueuedJob {
    run_id: Uuid,
    job: AppJob,
}

pub async fn run_worker(config: AppConfig) -> Result<()> {
    let state = AppState::new(config).await?;
    let conn = apalis_redis::connect(state.config.redis_url.clone()).await?;
    let storage = RedisStorage::new(conn);

    tokio::spawn(schedule_due_monitor_checks(state.clone()));

    ApalisMonitor::new()
        .register(
            WorkerBuilder::new("uptime-console-worker")
                .concurrency(2)
                .data(state)
                .backend(storage)
                .build_fn(perform_job),
        )
        .run()
        .await
        .context("worker stopped unexpectedly")
}

pub async fn enqueue(state: &AppState, job: AppJob) -> Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO job_runs (job_type, status, detail) VALUES ($1, 'queued', $2) RETURNING id",
    )
    .bind(job.kind())
    .bind(job.public_detail())
    .fetch_one(&state.pool)
    .await?;

    let conn = apalis_redis::connect(state.config.redis_url.clone()).await?;
    let mut storage = RedisStorage::new(conn);
    storage.push(QueuedJob { run_id: id, job }).await?;
    publish_event(state, format!("job:{id}:queued")).await;
    Ok(id)
}

pub async fn forward_events_from_redis(state: AppState) {
    loop {
        if let Err(error) = forward_events_once(&state).await {
            tracing::warn!(%error, "redis event bridge disconnected");
        }
        time::sleep(Duration::from_secs(1)).await;
    }
}

async fn forward_events_once(state: &AppState) -> Result<()> {
    let mut pubsub = state.redis.get_async_pubsub().await?;
    pubsub.subscribe(APP_EVENTS_CHANNEL).await?;
    let mut messages = pubsub.on_message();

    while let Some(message) = messages.next().await {
        let payload: String = message
            .get_payload()
            .context("failed to decode app event payload")?;
        let _ = state.events.send(payload);
    }

    Ok(())
}

pub async fn recent_job_runs(pool: &PgPool) -> Result<Vec<JobRun>> {
    Ok(sqlx::query_as::<_, JobRun>(
        "SELECT id, job_type, status, detail, started_at, finished_at, created_at FROM job_runs ORDER BY created_at DESC LIMIT 20",
    )
    .fetch_all(pool)
    .await?)
}

async fn perform_job(queued: QueuedJob, data: Data<AppState>) -> Result<()> {
    let state: &AppState = &data;
    mark_running(&state.pool, queued.run_id).await?;
    publish_event(state, format!("job:{}:running", queued.run_id)).await;

    let result = match queued.job.clone() {
        AppJob::CheckMonitor { monitor_id } => check_monitor(state, monitor_id).await,
        AppJob::RebuildRollups => rebuild_rollups(state).await,
        AppJob::ClearCache => clear_cache(state).await,
        AppJob::PruneEvents => prune_events(state).await,
        AppJob::ReseedDemo => db::seed(&state.pool, &state.config).await,
    };

    match result {
        Ok(()) => {
            mark_finished(&state.pool, queued.run_id, "succeeded", None).await?;
            publish_event(state, format!("job:{}:succeeded", queued.run_id)).await;
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                job_type = queued.job.kind(),
                run_id = %queued.run_id,
                "background job failed"
            );
            mark_finished(
                &state.pool,
                queued.run_id,
                "failed",
                Some("job failed; check server logs".to_string()),
            )
            .await?;
            publish_event(state, format!("job:{}:failed", queued.run_id)).await;
        }
    }
    Ok(())
}

async fn schedule_due_monitor_checks(state: AppState) {
    let mut tick = time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        if let Err(error) = enqueue_due_monitor_checks(&state).await {
            tracing::warn!(%error, "failed to enqueue due monitor checks");
        }
    }
}

async fn enqueue_due_monitor_checks(state: &AppState) -> Result<()> {
    let monitors = sqlx::query_as::<_, Monitor>(
        r#"
        SELECT id, name, url, method, expected_status, interval_seconds, enabled, last_status,
               last_latency_ms, last_checked_at, created_at, updated_at
        FROM monitors
        WHERE enabled = true
          AND (last_checked_at IS NULL OR last_checked_at < now() - (interval_seconds || ' seconds')::interval)
        ORDER BY COALESCE(last_checked_at, 'epoch'::timestamptz)
        LIMIT 10
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    for monitor in monitors {
        enqueue(
            state,
            AppJob::CheckMonitor {
                monitor_id: monitor.id,
            },
        )
        .await?;
    }
    Ok(())
}

async fn check_monitor(state: &AppState, monitor_id: Uuid) -> Result<()> {
    let monitor = sqlx::query_as::<_, Monitor>(
        r#"
        SELECT id, name, url, method, expected_status, interval_seconds, enabled, last_status,
               last_latency_ms, last_checked_at, created_at, updated_at
        FROM monitors
        WHERE id = $1
        "#,
    )
    .bind(monitor_id)
    .fetch_one(&state.pool)
    .await?;

    if let Err(error) = safety::ensure_monitor_url_allowed(&state.config, &monitor.url).await {
        tracing::warn!(%error, %monitor_id, "blocked unsafe monitor request");
        record_check_result(
            state,
            &monitor,
            "down",
            None,
            None,
            Some("request blocked by URL safety policy".to_string()),
        )
        .await?;
        return Ok(());
    }

    let method = match monitor.method.parse::<reqwest::Method>() {
        Ok(method) if matches!(method, reqwest::Method::GET | reqwest::Method::HEAD) => method,
        _ => {
            record_check_result(
                state,
                &monitor,
                "down",
                None,
                None,
                Some("unsupported monitor method".to_string()),
            )
            .await?;
            return Ok(());
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("uptime-console/0.1")
        .build()?;
    let started = Instant::now();
    let response = client.request(method, &monitor.url).send().await;
    let latency_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;

    let (status, status_code, message) = match response {
        Ok(resp) => {
            let code = resp.status().as_u16() as i32;
            if code == monitor.expected_status {
                ("up", Some(code), Some(format!("expected HTTP {code}")))
            } else {
                (
                    "down",
                    Some(code),
                    Some(format!("expected {}, got {code}", monitor.expected_status)),
                )
            }
        }
        Err(error) => {
            tracing::warn!(%error, %monitor_id, "monitor request failed");
            ("down", None, Some("request failed".to_string()))
        }
    };

    record_check_result(
        state,
        &monitor,
        status,
        Some(latency_ms),
        status_code,
        message,
    )
    .await
}

async fn record_check_result(
    state: &AppState,
    monitor: &Monitor,
    status: &str,
    latency_ms: Option<i32>,
    status_code: Option<i32>,
    message: Option<String>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO check_events (monitor_id, status, latency_ms, status_code, message)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(monitor.id)
    .bind(status)
    .bind(latency_ms)
    .bind(status_code)
    .bind(message.clone())
    .execute(&state.pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE monitors
        SET last_status = $2, last_latency_ms = $3, last_checked_at = now(), updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(monitor.id)
    .bind(status)
    .bind(latency_ms)
    .execute(&state.pool)
    .await?;

    let incidents_changed =
        sync_incident_for_check(state, monitor, status, message.as_deref()).await?;
    cache::delete(&state.redis, "cache:dashboard").await.ok();
    publish_event(state, format!("monitor:{}:{status}", monitor.id)).await;
    if incidents_changed {
        publish_event(state, format!("incident:{}:changed", monitor.id)).await;
    }
    Ok(())
}

async fn sync_incident_for_check(
    state: &AppState,
    monitor: &Monitor,
    status: &str,
    message: Option<&str>,
) -> Result<bool> {
    match status {
        "down" | "degraded" => {
            let severity = if status == "down" { "major" } else { "minor" };
            let reason = message.unwrap_or("monitor check failed");
            let title = format!("{} is {status}: {reason}", monitor.name);
            let result = sqlx::query(
                r#"
                INSERT INTO incidents (monitor_id, title, severity)
                VALUES ($1, $2, $3)
                ON CONFLICT (monitor_id) WHERE status = 'open' AND monitor_id IS NOT NULL DO NOTHING
                "#,
            )
            .bind(monitor.id)
            .bind(title)
            .bind(severity)
            .execute(&state.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        }
        "up" => {
            let result = sqlx::query(
                r#"
                UPDATE incidents
                SET status = 'resolved', resolved_at = now()
                WHERE monitor_id = $1 AND status = 'open'
                "#,
            )
            .bind(monitor.id)
            .execute(&state.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        }
        _ => Ok(false),
    }
}

async fn rebuild_rollups(state: &AppState) -> Result<()> {
    cache::delete(&state.redis, "cache:dashboard").await?;
    publish_event(state, "maintenance:rebuild_rollups").await;
    Ok(())
}

async fn clear_cache(state: &AppState) -> Result<()> {
    cache::delete(&state.redis, "cache:dashboard").await?;
    publish_event(state, "maintenance:clear_cache").await;
    Ok(())
}

async fn prune_events(state: &AppState) -> Result<()> {
    sqlx::query("DELETE FROM check_events WHERE checked_at < now() - interval '14 days'")
        .execute(&state.pool)
        .await?;
    publish_event(state, "maintenance:prune_events").await;
    Ok(())
}

pub async fn publish_event(state: &AppState, payload: impl Into<String>) {
    let payload = payload.into();

    if let Err(error) = publish_event_to_redis(state, &payload).await {
        tracing::debug!(%error, "failed to publish app event to redis");
        let _ = state.events.send(payload);
    }
}

async fn publish_event_to_redis(state: &AppState, payload: &str) -> Result<()> {
    let mut connection = state.redis.get_multiplexed_async_connection().await?;
    let _: i64 = redis::cmd("PUBLISH")
        .arg(APP_EVENTS_CHANNEL)
        .arg(payload)
        .query_async(&mut connection)
        .await?;
    Ok(())
}

async fn mark_running(pool: &PgPool, run_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE job_runs SET status = 'running', started_at = now() WHERE id = $1")
        .bind(run_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn mark_finished(
    pool: &PgPool,
    run_id: Uuid,
    status: &str,
    detail: Option<String>,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_runs SET status = $2, detail = COALESCE($3, detail), finished_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

impl AppJob {
    fn kind(&self) -> &'static str {
        match self {
            Self::CheckMonitor { .. } => "check_monitor",
            Self::RebuildRollups => "rebuild_rollups",
            Self::ClearCache => "clear_cache",
            Self::PruneEvents => "prune_events",
            Self::ReseedDemo => "reseed_demo",
        }
    }

    fn public_detail(&self) -> &'static str {
        match self {
            Self::CheckMonitor { .. } => "check monitor",
            Self::RebuildRollups => "rebuild dashboard rollups",
            Self::ClearCache => "clear application cache",
            Self::PruneEvents => "prune old check events",
            Self::ReseedDemo => "reseed demo data",
        }
    }
}
