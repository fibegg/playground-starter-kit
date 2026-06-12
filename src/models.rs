use async_graphql::SimpleObject;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};
use uptime_domain::Role;
use uuid::Uuid;

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
}

impl CurrentUser {
    pub fn parsed_role(&self) -> Role {
        Role::parse(&self.role)
    }
}

#[derive(Clone, Debug)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub password_hash: String,
}

impl<'r> FromRow<'r, PgRow> for UserRow {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            email: row.try_get("email")?,
            name: row.try_get("name")?,
            role: row.try_get("role")?,
            password_hash: row.try_get("password_hash")?,
        })
    }
}

impl From<UserRow> for CurrentUser {
    fn from(value: UserRow) -> Self {
        Self {
            id: value.id,
            email: value.email,
            name: value.name,
            role: value.role,
        }
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct Monitor {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub method: String,
    pub expected_status: i32,
    pub interval_seconds: i32,
    pub enabled: bool,
    pub last_status: String,
    pub last_latency_ms: Option<i32>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for Monitor {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            url: row.try_get("url")?,
            method: row.try_get("method")?,
            expected_status: row.try_get("expected_status")?,
            interval_seconds: row.try_get("interval_seconds")?,
            enabled: row.try_get("enabled")?,
            last_status: row.try_get("last_status")?,
            last_latency_ms: row.try_get("last_latency_ms")?,
            last_checked_at: row.try_get("last_checked_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct CheckEvent {
    pub id: Uuid,
    pub monitor_id: Uuid,
    pub status: String,
    pub latency_ms: Option<i32>,
    pub status_code: Option<i32>,
    pub message: Option<String>,
    pub checked_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for CheckEvent {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            monitor_id: row.try_get("monitor_id")?,
            status: row.try_get("status")?,
            latency_ms: row.try_get("latency_ms")?,
            status_code: row.try_get("status_code")?,
            message: row.try_get("message")?,
            checked_at: row.try_get("checked_at")?,
        })
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct Incident {
    pub id: Uuid,
    pub monitor_id: Option<Uuid>,
    pub title: String,
    pub status: String,
    pub severity: String,
    pub opened_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl<'r> FromRow<'r, PgRow> for Incident {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            monitor_id: row.try_get("monitor_id")?,
            title: row.try_get("title")?,
            status: row.try_get("status")?,
            severity: row.try_get("severity")?,
            opened_at: row.try_get("opened_at")?,
            resolved_at: row.try_get("resolved_at")?,
        })
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct JobRun {
    pub id: Uuid,
    pub job_type: String,
    pub status: String,
    pub detail: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for JobRun {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            job_type: row.try_get("job_type")?,
            status: row.try_get("status")?,
            detail: row.try_get("detail")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct MaintenanceRun {
    pub id: Uuid,
    pub task_name: String,
    pub status: String,
    pub output: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl<'r> FromRow<'r, PgRow> for MaintenanceRun {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            task_name: row.try_get("task_name")?,
            status: row.try_get("status")?,
            output: row.try_get("output")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
        })
    }
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct Dashboard {
    pub monitor_count: i32,
    pub up_count: i32,
    pub down_count: i32,
    pub open_incident_count: i32,
    pub avg_latency_ms: Option<f64>,
}

#[derive(Clone, Debug, SimpleObject, Serialize, Deserialize)]
#[graphql(rename_fields = "camelCase")]
pub struct MaintenanceTask {
    pub name: String,
    pub description: String,
    pub dangerous: bool,
}
