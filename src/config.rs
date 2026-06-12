use anyhow::{Context, Result};
use std::{env, net::SocketAddr, time::Duration};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub env: String,
    pub port: u16,
    pub secret: String,
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub db_acquire_timeout: Duration,
    pub redis_url: String,
    pub allowed_hosts: Vec<String>,
    pub cors_allowed_origins: Vec<String>,
    pub public_origin: Option<String>,
    pub trust_proxy_headers: bool,
    pub force_https: bool,
    pub assume_https: bool,
    pub hsts: bool,
    pub frame_ancestors: Option<String>,
    pub x_frame_options: Option<String>,
    pub csp_mode: CspMode,
    pub csp_connect_src: Option<String>,
    pub cookie_secure: CookieSecure,
    pub cookie_same_site: String,
    pub demo_password: String,
    pub allow_private_monitor_urls: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CspMode {
    Off,
    ReportOnly,
    Enforce,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CookieSecure {
    Auto,
    Always,
    Never,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let env_name = env_or("APP_ENV", "development");
        Ok(Self {
            env: env_name.clone(),
            port: env_or("APP_PORT", "3000")
                .parse()
                .context("APP_PORT must be a port")?,
            secret: env_or("APP_SECRET", "dev-secret-change-me"),
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            db_max_connections: env_or("DB_MAX_CONNECTIONS", "5")
                .parse()
                .context("DB_MAX_CONNECTIONS must be a number")?,
            db_min_connections: env_or("DB_MIN_CONNECTIONS", "0")
                .parse()
                .context("DB_MIN_CONNECTIONS must be a number")?,
            db_acquire_timeout: Duration::from_millis(
                env_or("DB_ACQUIRE_TIMEOUT_MS", "1500")
                    .parse()
                    .context("DB_ACQUIRE_TIMEOUT_MS must be a number")?,
            ),
            redis_url: env::var("REDIS_URL").context("REDIS_URL is required")?,
            allowed_hosts: list_env("APP_ALLOWED_HOSTS", "*"),
            cors_allowed_origins: list_env("APP_CORS_ALLOWED_ORIGINS", "*"),
            public_origin: blank_none(env::var("APP_PUBLIC_ORIGIN").unwrap_or_default()),
            trust_proxy_headers: bool_env("APP_TRUST_PROXY_HEADERS", true),
            force_https: bool_env("APP_FORCE_HTTPS", false),
            assume_https: bool_env("APP_ASSUME_HTTPS", false),
            hsts: bool_env("APP_HSTS", false),
            frame_ancestors: blank_none(env::var("APP_FRAME_ANCESTORS").unwrap_or_default()),
            x_frame_options: blank_none(env::var("APP_X_FRAME_OPTIONS").unwrap_or_default()),
            csp_mode: match env_or(
                "APP_CSP_MODE",
                if env_name == "production" {
                    "report-only"
                } else {
                    "off"
                },
            )
            .as_str()
            {
                "off" => CspMode::Off,
                "report-only" => CspMode::ReportOnly,
                "enforce" => CspMode::Enforce,
                other => {
                    anyhow::bail!("APP_CSP_MODE must be off, report-only, or enforce; got {other}")
                }
            },
            csp_connect_src: blank_none(env::var("APP_CSP_CONNECT_SRC").unwrap_or_default()),
            cookie_secure: match env_or("APP_COOKIE_SECURE", "auto").as_str() {
                "auto" => CookieSecure::Auto,
                "true" => CookieSecure::Always,
                "false" => CookieSecure::Never,
                other => {
                    anyhow::bail!("APP_COOKIE_SECURE must be auto, true, or false; got {other}")
                }
            },
            cookie_same_site: env_or("APP_COOKIE_SAMESITE", "lax"),
            demo_password: env_or("PUBLIC_DEMO_PASSWORD", "password"),
            allow_private_monitor_urls: bool_env("APP_ALLOW_PRIVATE_MONITOR_URLS", false),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }

    pub fn is_development(&self) -> bool {
        self.env != "production"
    }

    pub fn allows_all_hosts(&self) -> bool {
        self.allowed_hosts.iter().any(|host| host == "*")
    }

    pub fn allows_all_origins(&self) -> bool {
        self.cors_allowed_origins.iter().any(|origin| origin == "*")
    }

    pub fn cookie_secure_enabled(&self, request_is_https: bool) -> bool {
        match self.cookie_secure {
            CookieSecure::Always => true,
            CookieSecure::Never => false,
            CookieSecure::Auto => request_is_https || self.assume_https || self.force_https,
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn bool_env(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|value| match value.as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn list_env(key: &str, default: &str) -> Vec<String> {
    env_or(key, default)
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn blank_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
