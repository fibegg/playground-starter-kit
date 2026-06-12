use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

#[derive(Clone, Copy, Debug)]
pub struct MonitorUrlPolicy {
    pub allow_private_monitor_urls: bool,
}

#[derive(Clone, Debug)]
pub struct MonitorDraft {
    pub name: String,
    pub url: String,
    pub method: Option<String>,
    pub expected_status: Option<i32>,
    pub interval_seconds: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct ValidatedMonitor {
    pub name: String,
    pub url: String,
    pub method: String,
    pub expected_status: i32,
    pub interval_seconds: i32,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IncidentSeverity {
    Minor,
    Major,
    Critical,
}

impl IncidentSeverity {
    pub fn parse(value: Option<String>) -> Result<Self> {
        let severity = value.unwrap_or_else(|| "minor".to_string());
        match severity.trim().to_ascii_lowercase().as_str() {
            "minor" => Ok(Self::Minor),
            "major" => Ok(Self::Major),
            "critical" => Ok(Self::Critical),
            _ => anyhow::bail!("severity must be minor, major, or critical"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minor => "minor",
            Self::Major => "major",
            Self::Critical => "critical",
        }
    }
}

pub fn validate_monitor(policy: MonitorUrlPolicy, input: MonitorDraft) -> Result<ValidatedMonitor> {
    let name = clean_required(input.name, "name", 120)?;
    let url = normalize_monitor_url(policy, &input.url)?;
    let method = input
        .method
        .unwrap_or_else(|| "GET".to_string())
        .trim()
        .to_ascii_uppercase();
    anyhow::ensure!(
        matches!(method.as_str(), "GET" | "HEAD"),
        "method must be GET or HEAD"
    );

    let expected_status = input.expected_status.unwrap_or(200);
    anyhow::ensure!(
        (100..=599).contains(&expected_status),
        "expectedStatus must be between 100 and 599"
    );

    let interval_seconds = input.interval_seconds.unwrap_or(60);
    anyhow::ensure!(
        (15..=86_400).contains(&interval_seconds),
        "intervalSeconds must be between 15 and 86400"
    );

    Ok(ValidatedMonitor {
        name,
        url,
        method,
        expected_status,
        interval_seconds,
        enabled: input.enabled.unwrap_or(true),
    })
}

pub fn clean_required(value: String, field: &str, max_len: usize) -> Result<String> {
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "{field} is required");
    anyhow::ensure!(value.len() <= max_len, "{field} is too long");
    Ok(value.to_string())
}

pub fn normalize_monitor_url(policy: MonitorUrlPolicy, raw: &str) -> Result<String> {
    let value = raw.trim();
    anyhow::ensure!(!value.is_empty(), "monitor URL is required");
    anyhow::ensure!(value.len() <= 2048, "monitor URL is too long");

    let url = Url::parse(value).context("monitor URL must be absolute")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https"),
        "monitor URL must use http or https"
    );
    anyhow::ensure!(url.host_str().is_some(), "monitor URL must include a host");
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "monitor URL must not include credentials"
    );
    anyhow::ensure!(
        url.fragment().is_none(),
        "monitor URL must not include a fragment"
    );

    if !policy.allow_private_monitor_urls && host_is_private_or_local(&url) {
        anyhow::bail!(
            "monitor URL must use a public host unless APP_ALLOW_PRIVATE_MONITOR_URLS=true"
        );
    }

    Ok(url.to_string())
}

pub fn host_is_private_or_local(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return true;
    };

    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized == "localhost" || normalized.ends_with(".localhost") {
        return true;
    }

    host.parse::<IpAddr>().is_ok_and(ip_is_private_or_local)
}

pub fn ip_is_private_or_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_private_or_local(ip),
        IpAddr::V6(ip) => ipv6_is_private_or_local(ip),
    }
}

fn ipv4_is_private_or_local(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
}

fn ipv6_is_private_or_local(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.is_multicast()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(allow_private_monitor_urls: bool) -> MonitorUrlPolicy {
        MonitorUrlPolicy {
            allow_private_monitor_urls,
        }
    }

    #[test]
    fn rejects_private_monitor_urls_by_default() {
        let error = normalize_monitor_url(policy(false), "http://127.0.0.1:3000/up")
            .expect_err("loopback URLs should be blocked");
        assert!(error.to_string().contains("public host"));
    }

    #[test]
    fn allows_private_monitor_urls_when_configured() {
        assert!(normalize_monitor_url(policy(true), "http://127.0.0.1:3000/up").is_ok());
    }

    #[test]
    fn rejects_monitor_urls_with_credentials() {
        let error = normalize_monitor_url(policy(false), "https://user:pass@example.com")
            .expect_err("credentialed URLs should be blocked");
        assert!(error.to_string().contains("credentials"));
    }

    #[test]
    fn validates_monitor_input() {
        let monitor = validate_monitor(
            policy(false),
            MonitorDraft {
                name: "Docs".to_string(),
                url: "https://docs.rs".to_string(),
                method: None,
                expected_status: None,
                interval_seconds: None,
                enabled: None,
            },
        )
        .unwrap();
        assert_eq!(monitor.method, "GET");
        assert_eq!(monitor.expected_status, 200);
        assert!(monitor.enabled);
    }
}
