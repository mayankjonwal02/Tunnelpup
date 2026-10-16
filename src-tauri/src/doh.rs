//! Minimal DNS-over-HTTPS client (JSON API), for lookups that must bypass the
//! system resolver, where the router may hold a stale "not found".
//! Only look up names that should exist: public resolvers cache "not found" too,
//! for as long as the parent zone says (24 hours on some suffixes).

use std::net::IpAddr;
use std::time::Duration;

use serde::Deserialize;

const DOH: &[&str] = &["https://cloudflare-dns.com/dns-query", "https://dns.google/resolve"];

#[derive(Clone, Copy)]
pub enum RecordType {
    A,
    Ns,
}

impl RecordType {
    fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Ns => "NS",
        }
    }
    fn code(self) -> u16 {
        match self {
            Self::A => 1,
            Self::Ns => 2,
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "type")]
    kind: u16,
    data: String,
}

#[derive(Deserialize)]
struct Reply {
    #[serde(rename = "Answer", default)]
    answer: Vec<Answer>,
}

/// Data of the `kind` records for `name`, from the first resolver that has any.
pub async fn records(name: &str, kind: RecordType) -> Option<Vec<String>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?;
    for url in DOH {
        let req =
            http.get(*url).query(&[("name", name), ("type", kind.name())]).header("accept", "application/dns-json");
        let Ok(res) = req.send().await else { continue };
        let Ok(reply) = res.json::<Reply>().await else { continue };
        let found: Vec<String> = reply.answer.into_iter().filter(|a| a.kind == kind.code()).map(|a| a.data).collect();
        if !found.is_empty() {
            return Some(found);
        }
    }
    None
}

/// First IPv4 address for `name`.
pub async fn first_ipv4(name: &str) -> Option<IpAddr> {
    records(name, RecordType::A).await?.iter().find_map(|d| d.parse().ok())
}
