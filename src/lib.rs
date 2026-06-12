pub mod ability;
pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod graphql;
pub mod jobs;
pub mod maintenance;
pub mod models;
pub mod safety;
pub mod security;
pub mod web;

use tracing_subscriber::{EnvFilter, fmt};

pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).compact().init();
}
