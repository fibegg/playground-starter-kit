use crate::{
    jobs::{self, AppJob},
    models::{MaintenanceRun, MaintenanceTask},
    web::AppState,
};
use anyhow::Result;

pub fn tasks() -> Vec<MaintenanceTask> {
    vec![
        MaintenanceTask {
            name: "rebuild_rollups".to_string(),
            description:
                "Clear dashboard rollups and force the next request to rebuild cached summaries."
                    .to_string(),
            dangerous: false,
        },
        MaintenanceTask {
            name: "clear_cache".to_string(),
            description: "Remove Redis-backed application caches.".to_string(),
            dangerous: false,
        },
        MaintenanceTask {
            name: "prune_events".to_string(),
            description: "Delete check events older than 14 days.".to_string(),
            dangerous: true,
        },
        MaintenanceTask {
            name: "reseed_demo".to_string(),
            description: "Ensure demo users and monitors exist.".to_string(),
            dangerous: false,
        },
    ]
}

pub async fn run(state: &AppState, name: &str) -> Result<MaintenanceRun> {
    let task = match name {
        "rebuild_rollups" => AppJob::RebuildRollups,
        "clear_cache" => AppJob::ClearCache,
        "prune_events" => AppJob::PruneEvents,
        "reseed_demo" => AppJob::ReseedDemo,
        _ => anyhow::bail!("unknown maintenance task"),
    };

    let run = sqlx::query_as::<_, MaintenanceRun>(
        "INSERT INTO maintenance_runs (task_name, status) VALUES ($1, 'running') RETURNING id, task_name, status, output, started_at, finished_at",
    )
    .bind(name)
    .fetch_one(&state.pool)
    .await?;

    let enqueue_result = jobs::enqueue(state, task).await;
    let (status, output) = match enqueue_result {
        Ok(job_id) => ("succeeded", Some(format!("queued background job {job_id}"))),
        Err(error) => {
            tracing::warn!(%error, task_name = name, "failed to queue maintenance task");
            ("failed", Some("failed to queue background job".to_string()))
        }
    };

    Ok(sqlx::query_as::<_, MaintenanceRun>(
        "UPDATE maintenance_runs SET status = $2, output = $3, finished_at = now() WHERE id = $1 RETURNING id, task_name, status, output, started_at, finished_at",
    )
    .bind(run.id)
    .bind(status)
    .bind(output)
    .fetch_one(&state.pool)
    .await?)
}

pub async fn recent_runs(state: &AppState) -> Result<Vec<MaintenanceRun>> {
    Ok(sqlx::query_as::<_, MaintenanceRun>(
        "SELECT id, task_name, status, output, started_at, finished_at FROM maintenance_runs ORDER BY started_at DESC LIMIT 20",
    )
    .fetch_all(&state.pool)
    .await?)
}
