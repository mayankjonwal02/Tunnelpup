//! Supervises the bundled `cloudflared` sidecar: starts it, watches its logs to
//! derive connection status, and restarts it with backoff if it exits.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

use crate::error::{AppError, AppResult};
use crate::guard;

const MAX_LOG_LINES: usize = 300;
pub const STATUS_EVENT: &str = "connector-status";
pub const LOG_EVENT: &str = "connector-log";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ConnectorStatus {
    Stopped,
    Starting,
    Connected { connections: u32 },
    Reconnecting { retry_in_secs: u64, reason: String },
}

impl ConnectorStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Stopped => "Tunnel stopped".into(),
            Self::Starting => "Connecting…".into(),
            Self::Connected { connections } => format!("Connected ({connections} edge connections)"),
            Self::Reconnecting { retry_in_secs, .. } => format!("Reconnecting in {retry_in_secs}s"),
        }
    }
}

struct Inner {
    child: Option<CommandChild>,
    /// Bumped on every start/stop so a stale supervisor loop knows to exit.
    generation: u64,
    status: ConnectorStatus,
    logs: VecDeque<String>,
}

pub struct Connector {
    inner: Mutex<Inner>,
}

impl Default for Connector {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                child: None,
                generation: 0,
                status: ConnectorStatus::Stopped,
                logs: VecDeque::with_capacity(MAX_LOG_LINES),
            }),
        }
    }
}

impl Connector {
    pub fn status(&self) -> ConnectorStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn logs(&self) -> Vec<String> {
        self.inner.lock().unwrap().logs.iter().cloned().collect()
    }

    /// Start (or restart) cloudflared with the given connector token.
    pub fn start(&self, app: &AppHandle, token: String) -> AppResult<()> {
        // Fail fast if the sidecar binary is missing, before touching state.
        app.shell().sidecar(guard::SIDECAR).map_err(|e| AppError::Connector(e.to_string()))?;

        let generation = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(child) = inner.child.take() {
                let _ = child.kill();
            }
            inner.generation += 1;
            inner.generation
        };
        set_status(app, ConnectorStatus::Starting);

        let app = app.clone();
        tauri::async_runtime::spawn(async move { supervise(app, token, generation).await });
        Ok(())
    }

    pub fn stop(&self, app: &AppHandle) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            if let Some(child) = inner.child.take() {
                let _ = child.kill();
            }
        }
        guard::forget(app);
        set_status(app, ConnectorStatus::Stopped);
    }

    fn is_current(&self, generation: u64) -> bool {
        self.inner.lock().unwrap().generation == generation
    }
}

async fn supervise(app: AppHandle, token: String, generation: u64) {
    let connector = app.state::<Connector>();
    let mut backoff = 2u64;

    while connector.is_current(generation) {
        let spawned = app.shell().sidecar(guard::SIDECAR).and_then(|cmd| {
            cmd.args(["tunnel", "--no-autoupdate", "run"])
                // Passing the token via env keeps it out of the process list and off disk.
                .env("TUNNEL_TOKEN", &token)
                .spawn()
        });

        let (mut rx, child) = match spawned {
            Ok(v) => v,
            Err(e) => {
                push_log(&app, format!("[tunnelpup] failed to start cloudflared: {e}"));
                wait_and_retry(&app, generation, &mut backoff, e.to_string()).await;
                continue;
            }
        };
        // However TunnelPup ends, the connector must end with it.
        guard::tie_to_app(child.pid());
        guard::remember(&app, child.pid());

        {
            let mut inner = connector.inner.lock().unwrap();
            if inner.generation != generation {
                let _ = child.kill();
                return;
            }
            inner.child = Some(child);
        }

        let mut connections: u32 = 0;
        let mut exit_reason = String::from("cloudflared exited");

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(bytes) | CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes).trim_end().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if line.contains("Registered tunnel connection") {
                        connections += 1;
                        backoff = 2;
                        set_status(&app, ConnectorStatus::Connected { connections });
                    } else if line.contains("Unregistered tunnel connection") || line.contains("Connection terminated")
                    {
                        connections = connections.saturating_sub(1);
                        if connections > 0 {
                            set_status(&app, ConnectorStatus::Connected { connections });
                        } else {
                            set_status(&app, ConnectorStatus::Starting);
                        }
                    }
                    push_log(&app, line);
                }
                CommandEvent::Error(e) => push_log(&app, format!("[tunnelpup] {e}")),
                CommandEvent::Terminated(p) => {
                    exit_reason = match p.code {
                        Some(code) => format!("cloudflared exited with code {code}"),
                        None => "cloudflared was stopped by the system".into(),
                    };
                    break;
                }
                _ => {}
            }
        }

        connector.inner.lock().unwrap().child = None;
        if !connector.is_current(generation) {
            return; // stopped or restarted on purpose
        }
        push_log(&app, format!("[tunnelpup] {exit_reason}"));
        wait_and_retry(&app, generation, &mut backoff, exit_reason).await;
    }
}

async fn wait_and_retry(app: &AppHandle, generation: u64, backoff: &mut u64, reason: String) {
    set_status(app, ConnectorStatus::Reconnecting { retry_in_secs: *backoff, reason });
    tokio::time::sleep(Duration::from_secs(*backoff)).await;
    *backoff = (*backoff * 2).min(60);
    if app.state::<Connector>().is_current(generation) {
        set_status(app, ConnectorStatus::Starting);
    }
}

fn set_status(app: &AppHandle, status: ConnectorStatus) {
    let connector = app.state::<Connector>();
    {
        let mut inner = connector.inner.lock().unwrap();
        if inner.status == status {
            return;
        }
        inner.status = status.clone();
    }
    crate::tray::set_status(app, &status);
    let _ = app.emit(STATUS_EVENT, &status);
}

fn push_log(app: &AppHandle, line: String) {
    let connector = app.state::<Connector>();
    {
        let mut inner = connector.inner.lock().unwrap();
        if inner.logs.len() == MAX_LOG_LINES {
            inner.logs.pop_front();
        }
        inner.logs.push_back(line.clone());
    }
    let _ = app.emit(LOG_EVENT, line);
}
