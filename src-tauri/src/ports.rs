//! Suggests apps already running on this computer, by checking which of the common
//! dev-server ports have something listening on loopback. Never looks beyond this computer.

use std::time::Duration;

use tokio::net::TcpStream;

const COMMON_PORTS: &[u16] = &[
    80, 1313, 3000, 3001, 3002, 3333, 4000, 4200, 4321, 5000, 5001, 5173, 5174, 5500, 6006, 7000, 8000, 8001, 8080,
    8081, 8088, 8443, 8787, 8888, 9000, 9090,
];

/// True if something accepts connections on `port` over IPv4 or IPv6 loopback.
pub async fn is_listening(port: u16) -> bool {
    let v4 = tokio::time::timeout(Duration::from_millis(250), TcpStream::connect(("127.0.0.1", port)));
    let v6 = tokio::time::timeout(Duration::from_millis(250), TcpStream::connect(("::1", port)));
    let (a, b) = tokio::join!(v4, v6);
    matches!(a, Ok(Ok(_))) || matches!(b, Ok(Ok(_)))
}

pub async fn listening_ports() -> Vec<u16> {
    listening_among(COMMON_PORTS).await
}

/// The ports in `ports` that have something listening, probed in parallel.
pub async fn listening_among(ports: &[u16]) -> Vec<u16> {
    let handles: Vec<_> = ports
        .iter()
        .map(|&port| tauri::async_runtime::spawn(async move { is_listening(port).await.then_some(port) }))
        .collect();
    let mut out = Vec::new();
    for h in handles {
        if let Ok(Some(p)) = h.await {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_only_ports_with_a_listener() {
        let open = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let open_port = open.local_addr().unwrap().port();
        // Bound then dropped right away, so nothing listens there.
        let closed_port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let found = tauri::async_runtime::block_on(listening_among(&[open_port, closed_port]));
        assert_eq!(found, vec![open_port]);
    }
}
