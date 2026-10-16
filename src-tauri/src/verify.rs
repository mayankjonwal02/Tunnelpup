//! Confirms a new route works end to end before TunnelPup shows it: the hostname
//! resolves on public DNS and an HTTPS request through Cloudflare reaches the
//! local app. Lookups go over DNS-over-HTTPS so they never touch the system
//! resolver; a "not found" cached by the router would hide the new record for
//! up to 30 minutes.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::doh;

/// Cloudflare takes a few seconds to publish a new record. A lookup before then would make
/// the public resolvers cache "not found" for 30 minutes, so the first check waits.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: Duration = Duration::from_secs(60);
const RETRY_EVERY: Duration = Duration::from_secs(3);

/// Retry until `https://{hostname}/` answers through the tunnel, or return why it didn't.
pub async fn wait_until_live(hostname: &str, port: u16) -> Result<(), String> {
    let deadline = Instant::now() + GIVE_UP_AFTER;
    tokio::time::sleep(FIRST_CHECK_AFTER).await;
    loop {
        let why = match check(hostname, port).await {
            Ok(()) => return Ok(()),
            Err(why) => why,
        };
        if Instant::now() + RETRY_EVERY > deadline {
            return Err(why);
        }
        tokio::time::sleep(RETRY_EVERY).await;
    }
}

async fn check(hostname: &str, port: u16) -> Result<(), String> {
    let ip = doh::first_ipv4(hostname).await.ok_or("its DNS record isn't visible on public DNS yet")?;
    let client = reqwest::Client::builder()
        .resolve(hostname, SocketAddr::new(ip, 443))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client.get(format!("https://{hostname}/")).send().await.map_err(|e| {
        if is_missing_certificate(&e) {
            "Cloudflare hasn't issued this domain's HTTPS certificate yet, which usually takes 15 minutes to a few \
             hours after the domain goes active"
                .to_string()
        } else {
            format!("the request failed: {}", root_cause(&e))
        }
    })?;
    // Any answer from the app counts, even a 404 or a login redirect. These are Cloudflare's own errors.
    match res.status().as_u16() {
        530 => Err("Cloudflare can't reach the TunnelPup connector".into()),
        502 | 504 => Err(format!("the tunnel couldn't reach localhost:{port}")),
        _ => Ok(()),
    }
}

/// Some local apps (Ollama, Vite, Django, Rails…) only answer requests addressed to
/// localhost and refuse any other Host. True if the app on `port` refuses `hostname`
/// but answers localhost, so the tunnel should send it `Host: localhost` instead.
pub async fn rejects_public_host(hostname: &str, port: u16) -> bool {
    let Ok(client) =
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(5)).build()
    else {
        return false;
    };
    let status_for = |host: String| {
        let req = client.get(format!("http://localhost:{port}/")).header(reqwest::header::HOST, host);
        async move { req.send().await.ok().map(|r| r.status().as_u16()) }
    };
    let (local, public) = tokio::join!(status_for(format!("localhost:{port}")), status_for(hostname.to_string()));
    matches!((local, public), (Some(l), Some(p)) if l != p && matches!(p, 400 | 403 | 421))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CertStatus {
    /// Cloudflare presents a valid certificate for the zone and its subdomains.
    Ready,
    /// Cloudflare refuses the TLS handshake: it hasn't issued the certificate yet.
    Pending,
    /// Couldn't tell, e.g. offline.
    Unknown,
}

/// Whether Cloudflare has an HTTPS certificate for `zone` yet. It issues one a while
/// after the zone goes active; until then every https:// request to the zone fails.
pub async fn certificate(zone: &str) -> CertStatus {
    // Cloudflare picks the certificate by name on any of its addresses, so connect to
    // cloudflare.com's and ask for a made-up name under the zone. That needs no DNS
    // record in the zone, and the zone's certificate covers *.zone.
    let Some(ip) = doh::first_ipv4("cloudflare.com").await else { return CertStatus::Unknown };
    let host = format!("tunnelpup-cert-check.{zone}");
    let Ok(client) = reqwest::Client::builder()
        .resolve(&host, SocketAddr::new(ip, 443))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
    else {
        return CertStatus::Unknown;
    };
    match client.get(format!("https://{host}/")).send().await {
        // Any HTTP answer means the handshake, including the certificate's name check, passed.
        Ok(_) => CertStatus::Ready,
        Err(e) if is_missing_certificate(&e) => CertStatus::Pending,
        Err(_) => CertStatus::Unknown,
    }
}

/// Cloudflare answers a name it has no certificate for with a fatal HandshakeFailure alert.
fn is_missing_certificate(e: &reqwest::Error) -> bool {
    let cause = root_cause(e);
    cause.contains("HandshakeFailure") || cause.contains("certificate")
}

fn root_cause(e: &dyn std::error::Error) -> String {
    let mut e = e;
    while let Some(next) = e.source() {
        e = next;
    }
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A local HTTP server answering every request with `status(lowercased request)`.
    fn serve(status: fn(&str) -> &'static str) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let reply = format!("HTTP/1.1 {}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n", status(&request));
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        port
    }

    #[test]
    fn spots_an_app_that_only_answers_localhost() {
        // Like Ollama: 200 for Host localhost, 403 for anything else.
        let port = serve(|req| if req.contains("\r\nhost: localhost") { "200 OK" } else { "403 Forbidden" });
        assert!(tauri::async_runtime::block_on(rejects_public_host("ollama.example.com", port)));
    }

    #[test]
    fn leaves_other_apps_alone() {
        let any_host = serve(|_| "200 OK");
        let needs_login = serve(|_| "403 Forbidden");
        tauri::async_runtime::block_on(async {
            assert!(!rejects_public_host("app.example.com", any_host).await);
            assert!(
                !rejects_public_host("app.example.com", needs_login).await,
                "a 403 for everyone isn't about the Host"
            );
        });
    }

    /// Talks to Cloudflare. Run with
    /// `TUNNELPUP_TEST_DOMAIN=<a DigitalPlat domain on Cloudflare> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_certificate_check() {
        let domain = std::env::var("TUNNELPUP_TEST_DOMAIN")
            .expect("set TUNNELPUP_TEST_DOMAIN to a DigitalPlat domain that's active on Cloudflare");
        tauri::async_runtime::block_on(async {
            assert_eq!(certificate(&domain).await, CertStatus::Ready);
            // A DigitalPlat name that isn't on Cloudflare at all, so it has no certificate there.
            assert_eq!(certificate("tunnelpup-not-a-zone-7q.dpdns.org").await, CertStatus::Pending);
        });
    }
}
