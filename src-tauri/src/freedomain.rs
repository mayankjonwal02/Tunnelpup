//! Free domains from DigitalPlat FreeDomain (https://domain.digitalplat.org).
//! Hints whether a name is taken by asking DigitalPlat's own nameservers. A public
//! resolver can't be used: it would cache "not found" for the unregistered name
//! (24 hours on most of these suffixes), and the user's new domain would then look
//! missing for up to a day after they register it.

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::net::UdpSocket;

use crate::doh::{self, RecordType};

/// DigitalPlat suffixes on the Public Suffix List, the only ones Cloudflare accepts
/// as zones. Keep in sync with FREE_SUFFIXES in src/App.tsx.
pub const SUFFIXES: &[&str] = &["dpdns.org", "us.kg", "qzz.io", "xx.kg"];

/// DigitalPlat's main nameservers. They answer for every suffix, including qzz.io,
/// whose own listed servers refuse queries.
const DIGITALPLAT_NS: &[&str] =
    &["ns1.digitalplat.org", "ns2.digitalplat.org", "ns3.digitalplat.org", "ns4.digitalplat.org"];

const ANSWER_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// The registry says the name doesn't exist. Only a hint: a name registered
    /// without nameservers looks the same.
    Available,
    /// The name exists or is delegated, so someone has it.
    Taken,
    /// No DigitalPlat nameserver answered (outbound UDP port 53 may be blocked).
    Unknown,
}

pub async fn check(name: &str) -> Availability {
    let Some(suffix) = SUFFIXES.iter().find(|s| name.ends_with(&format!(".{s}"))) else {
        return Availability::Unknown;
    };
    let listed = doh::records(suffix, RecordType::Ns).await.unwrap_or_default();
    let mut servers: Vec<&str> = Vec::new();
    for host in listed.iter().map(|s| s.trim_end_matches('.')).chain(DIGITALPLAT_NS.iter().copied()) {
        if !servers.contains(&host) {
            servers.push(host);
        }
    }

    let id = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos() & 0xffff) as u16;
    let packet = query_packet(id, name);
    // Some servers have no IPv4 address, time out or refuse; any one that answers is enough.
    for server in servers {
        let Some(ip) = doh::first_ipv4(server).await else { continue };
        match ask(SocketAddr::new(ip, 53), &packet, id).await {
            Some(0) => return Availability::Taken,
            Some(3) => return Availability::Available,
            _ => continue,
        }
    }
    Availability::Unknown
}

/// Send one query and return the reply's response code.
async fn ask(server: SocketAddr, packet: &[u8], id: u16) -> Option<u8> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    socket.connect(server).await.ok()?;
    socket.send(packet).await.ok()?;
    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(ANSWER_TIMEOUT, socket.recv(&mut buf)).await.ok()?.ok()?;
    rcode(&buf[..n], id)
}

/// A non-recursive query for the NS records of `name`.
fn query_packet(id: u16, name: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(name.len() + 18);
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&[0, 0]); // standard query, recursion not desired
    p.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // one question, no other records
    for label in name.trim_end_matches('.').split('.') {
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.push(0);
    p.extend_from_slice(&[0, 2, 0, 1]); // type NS, class IN
    p
}

/// The response code, if `reply` is a response to query `id`.
fn rcode(reply: &[u8], id: u16) -> Option<u8> {
    if reply.len() < 12 || reply[..2] != id.to_be_bytes() || reply[2] & 0x80 == 0 {
        return None;
    }
    Some(reply[3] & 0x0f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_an_ns_query() {
        let p = query_packet(0xabcd, "myapp.us.kg");
        assert_eq!(&p[..12], &[0xab, 0xcd, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&p[12..], b"\x05myapp\x02us\x02kg\x00\x00\x02\x00\x01");
    }

    #[test]
    fn reads_the_response_code() {
        let mut reply = query_packet(7, "myapp.us.kg");
        assert_eq!(rcode(&reply, 7), None, "a query is not a response");
        reply[2] |= 0x80;
        reply[3] = 0x03;
        assert_eq!(rcode(&reply, 7), Some(3));
        assert_eq!(rcode(&reply, 8), None, "an answer to another query");
        assert_eq!(rcode(&reply[..5], 7), None, "truncated");
    }

    /// Talks to DigitalPlat's nameservers. Run with
    /// `TUNNELPUP_TEST_DOMAIN=<a DigitalPlat domain on Cloudflare> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_lookup() {
        let domain = std::env::var("TUNNELPUP_TEST_DOMAIN")
            .expect("set TUNNELPUP_TEST_DOMAIN to a DigitalPlat domain that's active on Cloudflare");
        tauri::async_runtime::block_on(async {
            assert_eq!(check(&domain).await, Availability::Taken);
            for s in SUFFIXES {
                assert_eq!(check(&format!("tunnelpup-test-unregistered-7q.{s}")).await, Availability::Available, "{s}");
            }
        });
    }
}
