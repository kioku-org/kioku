//! SSRF-safe webhook URL validation. Faithful port of webhook_url.py.
//! Reference: OWASP SSRF Prevention Cheat Sheet.

use std::net::IpAddr;
use url::Url;

const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    "metadata.google.internal",
    "metadata.amazonaws.com",
    "metadata",
    "api-gateway",
    "admin-api",
    "meeting-api",
    "runtime-api",
    "transcription-collector",
    "redis",
    "postgres",
    "mcp",
];

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // covers 169.254.169.254 cloud metadata
                || v4.is_multicast()
                || v4.octets()[0] == 0 // 0.0.0.0/8
        }
        IpAddr::V6(v6) => v6.is_loopback() || is_unique_local_v6(v6) || v6.is_multicast() || is_link_local_v6(v6),
    }
}

fn is_unique_local_v6(v6: std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7
}

fn is_link_local_v6(v6: std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10
}

fn is_blocked_hostname(hostname: &str) -> bool {
    BLOCKED_HOSTNAMES.contains(&hostname.to_lowercase().as_str())
}

/// Validates a webhook URL is not SSRF-vulnerable: http(s) only, no private/loopback/
/// link-local/metadata IPs, no internal Docker service hostnames, and — since DNS
/// rebinding could otherwise bypass a hostname-only check — every resolved IP is checked
/// too, not just the literal host.
pub async fn validate_webhook_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|_| "Webhook URL is not a well-formed URL".to_string())?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Webhook URL must use http or https scheme".to_string());
    }

    let hostname = parsed.host_str().ok_or("Webhook URL must have a valid hostname")?;

    if is_blocked_hostname(hostname) {
        return Err("Webhook URL cannot target internal or private networks".to_string());
    }

    if let Ok(ip) = hostname.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err("Webhook URL cannot target internal or private networks".to_string());
        }
        return Ok(());
    }

    // Not a literal IP — resolve and validate every returned address (DNS rebinding guard).
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((hostname, port))
        .await
        .map(|it| it.collect::<Vec<_>>())
        .unwrap_or_default();
    if addrs.is_empty() {
        return Err("Webhook URL hostname could not be resolved".to_string());
    }
    for addr in addrs {
        if is_blocked_ip(addr.ip()) {
            return Err("Webhook URL cannot target internal or private networks".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        assert!(validate_webhook_url("ftp://example.com/hook").await.is_err());
    }

    #[tokio::test]
    async fn rejects_localhost() {
        assert!(validate_webhook_url("http://localhost:8080/hook").await.is_err());
    }

    #[tokio::test]
    async fn rejects_loopback_ip() {
        assert!(validate_webhook_url("http://127.0.0.1/hook").await.is_err());
    }

    #[tokio::test]
    async fn rejects_cloud_metadata_ip() {
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data").await.is_err());
    }

    #[tokio::test]
    async fn rejects_private_network_ip() {
        assert!(validate_webhook_url("http://10.0.0.5/hook").await.is_err());
        assert!(validate_webhook_url("http://192.168.1.1/hook").await.is_err());
    }

    #[tokio::test]
    async fn rejects_internal_docker_hostname() {
        assert!(validate_webhook_url("http://admin-api:8001/hook").await.is_err());
    }

    #[tokio::test]
    async fn accepts_public_looking_ip() {
        // 8.8.8.8 is a real public IP (Google DNS) — not in any blocked range.
        assert!(validate_webhook_url("https://8.8.8.8/hook").await.is_ok());
    }
}
