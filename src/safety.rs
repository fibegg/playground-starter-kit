use crate::config::AppConfig;
use anyhow::{Context, Result};
use tokio::net::lookup_host;
use uptime_domain::{MonitorUrlPolicy, host_is_private_or_local, ip_is_private_or_local};
use url::Url;

pub fn normalize_monitor_url(config: &AppConfig, raw: &str) -> Result<String> {
    uptime_domain::normalize_monitor_url(monitor_url_policy(config), raw)
}

pub async fn ensure_monitor_url_allowed(config: &AppConfig, raw: &str) -> Result<()> {
    let url = Url::parse(raw).context("monitor URL must be absolute")?;
    let policy = monitor_url_policy(config);
    if policy.allow_private_monitor_urls {
        return Ok(());
    }

    if host_is_private_or_local(&url) {
        anyhow::bail!("monitor URL targets a private or local host");
    }

    let host = url.host_str().context("monitor URL must include a host")?;
    let port = url
        .port_or_known_default()
        .context("monitor URL must include a known port")?;
    let mut resolved_any = false;
    for address in lookup_host((host, port)).await? {
        resolved_any = true;
        if ip_is_private_or_local(address.ip()) {
            anyhow::bail!("monitor URL resolves to a private or local address");
        }
    }
    anyhow::ensure!(resolved_any, "monitor URL did not resolve");
    Ok(())
}

pub fn monitor_url_policy(config: &AppConfig) -> MonitorUrlPolicy {
    MonitorUrlPolicy {
        allow_private_monitor_urls: config.allow_private_monitor_urls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(allow_private_monitor_urls: bool) -> AppConfig {
        AppConfig {
            env: "test".to_string(),
            port: 3000,
            secret: "test-secret".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            db_max_connections: 1,
            db_min_connections: 0,
            db_acquire_timeout: std::time::Duration::from_millis(100),
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
            csp_mode: crate::config::CspMode::Off,
            cookie_secure: crate::config::CookieSecure::Never,
            cookie_same_site: "lax".to_string(),
            demo_password: "password".to_string(),
            allow_private_monitor_urls,
            csp_connect_src: None,
        }
    }

    #[test]
    fn rejects_private_monitor_urls_by_default() {
        let error = normalize_monitor_url(&config(false), "http://127.0.0.1:3000/up")
            .expect_err("loopback URLs should be blocked");
        assert!(error.to_string().contains("public host"));
    }

    #[test]
    fn allows_private_monitor_urls_when_configured() {
        assert!(normalize_monitor_url(&config(true), "http://127.0.0.1:3000/up").is_ok());
    }

    #[test]
    fn rejects_monitor_urls_with_credentials() {
        let error = normalize_monitor_url(&config(false), "https://user:pass@example.com")
            .expect_err("credentialed URLs should be blocked");
        assert!(error.to_string().contains("credentials"));
    }
}
