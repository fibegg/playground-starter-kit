use crate::{
    ability::{self, Action, Resource},
    cache,
    jobs::{self, AppJob},
    maintenance,
    models::{
        CheckEvent, CurrentUser, Dashboard, Incident, JobRun, MaintenanceRun, MaintenanceTask,
        Monitor,
    },
    safety,
    web::AppState,
};
use async_graphql::{Context, EmptySubscription, Error, InputObject, Object, Result, Schema};
use sqlx::PgPool;
use std::fmt::Display;
use uptime_domain::{IncidentSeverity, MonitorDraft, clean_required, validate_monitor};
use uuid::Uuid;

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema(state: AppState) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(state)
        .finish()
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn viewer(&self, ctx: &Context<'_>) -> Option<CurrentUser> {
        ctx.data_opt::<CurrentUser>().cloned()
    }

    async fn dashboard(&self, ctx: &Context<'_>) -> Result<Dashboard> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Monitor,
        )?;

        match cache::get_json::<Dashboard>(&state.redis, "cache:dashboard").await {
            Ok(Some(cached)) => return Ok(cached),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "dashboard cache read failed"),
        }

        let row = sqlx::query_as::<_, (i64, i64, i64, i64, Option<f64>)>(
            r#"
            SELECT
              COUNT(*) AS monitor_count,
              COUNT(*) FILTER (WHERE last_status = 'up') AS up_count,
              COUNT(*) FILTER (WHERE last_status = 'down') AS down_count,
              (SELECT COUNT(*) FROM incidents WHERE status = 'open') AS open_incident_count,
              AVG(last_latency_ms)::float8 AS avg_latency_ms
            FROM monitors
            "#,
        )
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;

        let dashboard = Dashboard {
            monitor_count: row.0 as i32,
            up_count: row.1 as i32,
            down_count: row.2 as i32,
            open_incident_count: row.3 as i32,
            avg_latency_ms: row.4,
        };
        cache::set_json(&state.redis, "cache:dashboard", &dashboard, 10)
            .await
            .ok();
        Ok(dashboard)
    }

    async fn monitors(&self, ctx: &Context<'_>) -> Result<Vec<Monitor>> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Monitor,
        )?;
        sqlx::query_as::<_, Monitor>(
            r#"
            SELECT id, name, url, method, expected_status, interval_seconds, enabled, last_status,
                   last_latency_ms, last_checked_at, created_at, updated_at
            FROM monitors
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)
    }

    async fn check_events(&self, ctx: &Context<'_>, monitor_id: Uuid) -> Result<Vec<CheckEvent>> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Monitor,
        )?;
        ensure_monitor_exists(&state.pool, monitor_id).await?;
        sqlx::query_as::<_, CheckEvent>(
            "SELECT id, monitor_id, status, latency_ms, status_code, message, checked_at FROM check_events WHERE monitor_id = $1 ORDER BY checked_at DESC LIMIT 50",
        )
        .bind(monitor_id)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)
    }

    async fn incidents(&self, ctx: &Context<'_>) -> Result<Vec<Incident>> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Incident,
        )?;
        sqlx::query_as::<_, Incident>(
            "SELECT id, monitor_id, title, status, severity, opened_at, resolved_at FROM incidents ORDER BY opened_at DESC LIMIT 50",
        )
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)
    }

    async fn job_runs(&self, ctx: &Context<'_>) -> Result<Vec<JobRun>> {
        let state = app_state(ctx)?;
        ability::require(ctx.data_opt::<CurrentUser>(), Action::Read, Resource::Job)?;
        jobs::recent_job_runs(&state.pool)
            .await
            .map_err(internal_error)
    }

    async fn maintenance_tasks(&self, ctx: &Context<'_>) -> Result<Vec<MaintenanceTask>> {
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Maintenance,
        )?;
        Ok(maintenance::tasks())
    }

    async fn maintenance_runs(&self, ctx: &Context<'_>) -> Result<Vec<MaintenanceRun>> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Read,
            Resource::Maintenance,
        )?;
        maintenance::recent_runs(state)
            .await
            .map_err(internal_error)
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn create_monitor(&self, ctx: &Context<'_>, input: MonitorInput) -> Result<Monitor> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Manage,
            Resource::Monitor,
        )?;
        let input = validate_monitor_input(state, input)?;
        let monitor = sqlx::query_as::<_, Monitor>(
            r#"
            INSERT INTO monitors (name, url, method, expected_status, interval_seconds, enabled)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, name, url, method, expected_status, interval_seconds, enabled, last_status,
                      last_latency_ms, last_checked_at, created_at, updated_at
            "#,
        )
        .bind(input.name)
        .bind(input.url)
        .bind(input.method)
        .bind(input.expected_status)
        .bind(input.interval_seconds)
        .bind(input.enabled)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;
        cache::delete(&state.redis, "cache:dashboard").await.ok();
        jobs::publish_event(state, format!("monitor:{}:created", monitor.id)).await;
        Ok(monitor)
    }

    async fn update_monitor(
        &self,
        ctx: &Context<'_>,
        id: Uuid,
        input: MonitorInput,
    ) -> Result<Monitor> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Manage,
            Resource::Monitor,
        )?;
        ensure_monitor_exists(&state.pool, id).await?;
        let input = validate_monitor_input(state, input)?;
        let monitor = sqlx::query_as::<_, Monitor>(
            r#"
            UPDATE monitors
            SET name = $2,
                url = $3,
                method = $4,
                expected_status = $5,
                interval_seconds = $6,
                enabled = $7,
                updated_at = now()
            WHERE id = $1
            RETURNING id, name, url, method, expected_status, interval_seconds, enabled, last_status,
                      last_latency_ms, last_checked_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(input.name)
        .bind(input.url)
        .bind(input.method)
        .bind(input.expected_status)
        .bind(input.interval_seconds)
        .bind(input.enabled)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;
        cache::delete(&state.redis, "cache:dashboard").await.ok();
        jobs::publish_event(state, format!("monitor:{}:updated", monitor.id)).await;
        Ok(monitor)
    }

    async fn delete_monitor(&self, ctx: &Context<'_>, id: Uuid) -> Result<bool> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Manage,
            Resource::Monitor,
        )?;
        let result = sqlx::query("DELETE FROM monitors WHERE id = $1")
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(internal_error)?;
        cache::delete(&state.redis, "cache:dashboard").await.ok();
        if result.rows_affected() > 0 {
            jobs::publish_event(state, format!("monitor:{id}:deleted")).await;
        }
        Ok(result.rows_affected() > 0)
    }

    async fn enqueue_check(&self, ctx: &Context<'_>, monitor_id: Uuid) -> Result<Uuid> {
        let state = app_state(ctx)?;
        ability::require(ctx.data_opt::<CurrentUser>(), Action::Run, Resource::Job)?;
        ensure_monitor_exists(&state.pool, monitor_id).await?;
        jobs::enqueue(state, AppJob::CheckMonitor { monitor_id })
            .await
            .map_err(internal_error)
    }

    async fn open_incident(&self, ctx: &Context<'_>, input: IncidentInput) -> Result<Incident> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Manage,
            Resource::Incident,
        )?;
        if let Some(monitor_id) = input.monitor_id {
            ensure_monitor_exists(&state.pool, monitor_id).await?;
        }
        let title = clean_required(input.title, "title", 160)
            .map_err(|error| bad_request(error.to_string()))?;
        let severity = IncidentSeverity::parse(input.severity)
            .map_err(|error| bad_request(error.to_string()))?;
        let incident = sqlx::query_as::<_, Incident>(
            r#"
            INSERT INTO incidents (monitor_id, title, severity)
            VALUES ($1, $2, $3)
            RETURNING id, monitor_id, title, status, severity, opened_at, resolved_at
            "#,
        )
        .bind(input.monitor_id)
        .bind(title)
        .bind(severity.as_str())
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;
        cache::delete(&state.redis, "cache:dashboard").await.ok();
        jobs::publish_event(state, format!("incident:{}:open", incident.id)).await;
        Ok(incident)
    }

    async fn resolve_incident(&self, ctx: &Context<'_>, id: Uuid) -> Result<Incident> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Manage,
            Resource::Incident,
        )?;
        let incident = sqlx::query_as::<_, Incident>(
            "UPDATE incidents SET status = 'resolved', resolved_at = now() WHERE id = $1 RETURNING id, monitor_id, title, status, severity, opened_at, resolved_at",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal_error)?
        .ok_or_else(not_found)?;
        cache::delete(&state.redis, "cache:dashboard").await.ok();
        jobs::publish_event(state, format!("incident:{}:resolved", incident.id)).await;
        Ok(incident)
    }

    async fn run_maintenance_task(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<MaintenanceRun> {
        let state = app_state(ctx)?;
        ability::require(
            ctx.data_opt::<CurrentUser>(),
            Action::Run,
            Resource::Maintenance,
        )?;
        let task_name = name.trim();
        if !maintenance::tasks()
            .iter()
            .any(|task| task.name == task_name)
        {
            return Err(bad_request("maintenance task not found"));
        }
        maintenance::run(state, task_name)
            .await
            .map_err(internal_error)
    }
}

#[derive(InputObject)]
pub struct MonitorInput {
    pub name: String,
    pub url: String,
    pub method: Option<String>,
    pub expected_status: Option<i32>,
    pub interval_seconds: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(InputObject)]
pub struct IncidentInput {
    pub monitor_id: Option<Uuid>,
    pub title: String,
    pub severity: Option<String>,
}

fn validate_monitor_input(
    state: &AppState,
    input: MonitorInput,
) -> Result<uptime_domain::ValidatedMonitor> {
    validate_monitor(
        safety::monitor_url_policy(&state.config),
        MonitorDraft {
            name: input.name,
            url: input.url,
            method: input.method,
            expected_status: input.expected_status,
            interval_seconds: input.interval_seconds,
            enabled: input.enabled,
        },
    )
    .map_err(|error| bad_request(error.to_string()))
}

async fn ensure_monitor_exists(pool: &PgPool, id: Uuid) -> Result<()> {
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM monitors WHERE id = $1)")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(internal_error)?;
    if exists { Ok(()) } else { Err(not_found()) }
}

fn app_state<'a>(ctx: &'a Context<'_>) -> Result<&'a AppState> {
    ctx.data::<AppState>()
        .map_err(|_| internal_error("missing app state"))
}

fn bad_request(message: impl Into<String>) -> Error {
    Error::new(message.into())
}

fn not_found() -> Error {
    Error::new("not found")
}

fn internal_error(error: impl Display) -> Error {
    tracing::error!(error = %error, "internal GraphQL error");
    Error::new("internal server error")
}
