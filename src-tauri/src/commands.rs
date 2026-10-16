//! Tauri commands called from the UI via `invoke(...)`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::cloudflare::{Account, Cloudflare, IngressRule, OriginRequest, Zone};
use crate::config::{self, AppConfig, Route, TunnelInfo};
use crate::connector::{Connector, ConnectorStatus};
use crate::error::{AppError, AppResult};
use crate::freedomain::{self, Availability};
use crate::verify::CertStatus;
use crate::{ports, secrets, verify};

const ROUTE_PROGRESS_EVENT: &str = "route-progress";

pub struct ConfigState(pub Mutex<AppConfig>);

/// Serialises route edits so two quick clicks can't race on the ingress list.
pub struct RouteLock(pub tokio::sync::Mutex<()>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    has_token: bool,
    config: AppConfig,
    connector: ConnectorStatus,
    cloudflared_version: &'static str,
    updates_configured: bool,
}

fn cf() -> AppResult<Cloudflare> {
    Cloudflare::new(secrets::require_token()?)
}

fn current(state: &State<ConfigState>) -> AppConfig {
    state.0.lock().unwrap().clone()
}

fn update(app: &AppHandle, f: impl FnOnce(&mut AppConfig)) -> AppResult<AppConfig> {
    let state = app.state::<ConfigState>();
    let mut cfg = state.0.lock().unwrap();
    f(&mut cfg);
    config::save(app, &cfg)?;
    Ok(cfg.clone())
}

fn account_id(cfg: &AppConfig) -> AppResult<String> {
    cfg.account_id.clone().ok_or(AppError::NoToken)
}

fn tunnel_id(cfg: &AppConfig) -> AppResult<String> {
    cfg.tunnel.as_ref().map(|t| t.id.clone()).ok_or(AppError::NoTunnel)
}

fn ingress_for(routes: &[Route]) -> Vec<IngressRule> {
    routes
        .iter()
        .map(|r| IngressRule {
            hostname: Some(r.hostname.clone()),
            service: format!("http://localhost:{}", r.port),
            origin_request: r.rewrite_host.then(|| OriginRequest { http_host_header: format!("localhost:{}", r.port) }),
        })
        .collect()
}

// ---- snapshot ---------------------------------------------------------------

#[tauri::command]
pub fn get_snapshot(app: AppHandle, state: State<ConfigState>, connector: State<Connector>) -> AppResult<AppSnapshot> {
    Ok(AppSnapshot {
        has_token: secrets::load_token()?.is_some(),
        config: current(&state),
        connector: connector.status(),
        cloudflared_version: crate::CLOUDFLARED_VERSION,
        updates_configured: updates_configured(&app),
    })
}

/// The updater needs a real signing key in tauri.conf.json; until one is set, checking can only fail.
fn updates_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.is_empty() && !k.starts_with("REPLACE_"))
}

#[tauri::command]
pub fn get_connector_logs(connector: State<Connector>) -> Vec<String> {
    connector.logs()
}

// ---- token ------------------------------------------------------------------

/// Verify a pasted token, then store it in the keychain. Returns the accounts it can see.
#[tauri::command]
pub async fn save_token(app: AppHandle, token: String) -> AppResult<Vec<Account>> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AppError::Invalid("Paste a Cloudflare API token first".into()));
    }
    let client = Cloudflare::new(&token)?;
    let user_check = client.verify_token().await;
    let accounts = visible_accounts(&client).await;
    let status = match user_check {
        Ok(status) => status,
        // Account-owned tokens fail the user check as "invalid"; they verify against their account.
        Err(e) if is_invalid_token(&e) => {
            let mut found = None;
            for account in &accounts {
                if let Ok(status) = client.verify_account_token(&account.id).await {
                    found = Some(status);
                    break;
                }
            }
            found.ok_or_else(|| {
                AppError::Invalid(
                    "Cloudflare doesn't recognise this token. Copy it again from the Cloudflare dashboard.".into(),
                )
            })?
        }
        Err(e) => return Err(e),
    };
    if status.status != "active" {
        return Err(AppError::Invalid(format!("This token is {}", status.status)));
    }
    let first = accounts.first().cloned().ok_or_else(|| {
        AppError::Invalid(
            "The token works but can't see any account. Add the 'Account → Account Settings: Read' permission.".into(),
        )
    })?;
    // The pre-filled token link can't include the Tunnel permission, so check it now rather
    // than failing later when TunnelPup creates the tunnel.
    if let Err(e) = client.find_tunnel(&first.id, &machine_tunnel_name()).await
        && is_permission_error(&e)
    {
        return Err(AppError::Invalid(
            "The token can't manage Cloudflare Tunnels. Edit it in Cloudflare and add Account → Cloudflare Tunnel → Edit.".into(),
        ));
    }

    secrets::save_token(&token)?;
    update(&app, |c| {
        if c.account_id.as_deref() != Some(first.id.as_str()) {
            c.account_id = Some(first.id.clone());
            c.account_name = Some(first.name.clone());
        }
    })?;
    Ok(accounts)
}

#[tauri::command]
pub fn select_account(app: AppHandle, account: Account) -> AppResult<AppConfig> {
    update(&app, |c| {
        if c.account_id.as_deref() != Some(account.id.as_str()) {
            c.account_id = Some(account.id);
            c.account_name = Some(account.name);
            c.tunnel = None;
            c.routes.clear();
        }
    })
}

/// Forget the token and local state. Does not delete anything in Cloudflare.
#[tauri::command]
pub fn sign_out(app: AppHandle, connector: State<Connector>) -> AppResult<()> {
    connector.stop(&app);
    secrets::delete_token()?;
    update(&app, |c| *c = AppConfig::default())?;
    Ok(())
}

// ---- domains ----------------------------------------------------------------

/// Domains in the connected account (a token can span several accounts).
#[tauri::command]
pub async fn list_zones(state: State<'_, ConfigState>) -> AppResult<Vec<Zone>> {
    let account = current(&state).account_id;
    cf()?.list_zones(account.as_deref()).await
}

#[tauri::command]
pub async fn add_zone(state: State<'_, ConfigState>, name: String) -> AppResult<Zone> {
    // Accept a pasted URL like https://example.com/ as well as a bare name.
    let name = name.trim().trim_start_matches("https://").trim_start_matches("http://");
    let name = name.split('/').next().unwrap_or_default().trim_end_matches('.').to_lowercase();
    if !name.contains('.') {
        return Err(AppError::Invalid("Enter a domain like example.com".into()));
    }
    let account = account_id(&current(&state))?;
    match cf()?.add_zone(&account, &name).await {
        Err(e) if is_permission_error(&e) => Err(AppError::Invalid(
            "Your API token isn't allowed to add domains. Edit it in Cloudflare → API Tokens and add \
             Zone → Zone → Edit, with Zone Resources set to all zones in this account."
                .into(),
        )),
        Err(e) if is_unregistered_error(&e) => Err(AppError::Invalid(format!(
            "Cloudflare doesn't see {name} as registered yet. Register it first (on DigitalPlat for a free domain), \
             then add it here again."
        ))),
        other => other,
    }
}

/// Hint whether `label.suffix` is still free on DigitalPlat.
#[tauri::command]
pub async fn check_free_domain(label: String, suffix: String) -> AppResult<Availability> {
    let label = label.trim().to_lowercase();
    if label.is_empty() || label == "@" || label.contains('.') {
        return Err(AppError::Invalid("Enter a single name like myapp".into()));
    }
    validate_label(&label)?;
    let suffix = suffix.trim_start_matches('.');
    if !freedomain::SUFFIXES.contains(&suffix) {
        return Err(AppError::Invalid(format!(".{suffix} isn't one of DigitalPlat's free domains")));
    }
    Ok(freedomain::check(&format!("{label}.{suffix}")).await)
}

/// Whether Cloudflare has issued `zone_name`'s HTTPS certificate yet.
#[tauri::command]
pub async fn check_https(zone_name: String) -> CertStatus {
    verify::certificate(zone_name.trim()).await
}

/// Ask Cloudflare to re-check a pending domain's nameservers, then return its current state.
#[tauri::command]
pub async fn check_zone(zone_id: String) -> AppResult<Zone> {
    let client = cf()?;
    // Best effort: Cloudflare limits how often this runs, and it re-checks on its own anyway.
    let _ = client.activation_check(&zone_id).await;
    client.get_zone(&zone_id).await
}

// ---- tunnel -----------------------------------------------------------------

/// Find this machine's tunnel (by name) or create it, then start cloudflared.
#[tauri::command]
pub async fn ensure_tunnel(app: AppHandle, state: State<'_, ConfigState>) -> AppResult<TunnelInfo> {
    let cfg = current(&state);
    let account = account_id(&cfg)?;
    let client = cf()?;

    let tunnel = match &cfg.tunnel {
        Some(t) => t.clone(),
        None => {
            let name = machine_tunnel_name();
            let t = match client.find_tunnel(&account, &name).await? {
                Some(t) => t,
                None => client.create_tunnel(&account, &name).await?,
            };
            let info = TunnelInfo { id: t.id, name: t.name };
            update(&app, |c| c.tunnel = Some(info.clone()))?;
            info
        }
    };

    let token = client.tunnel_token(&account, &tunnel.id).await?;
    app.state::<Connector>().start(&app, token)?;
    Ok(tunnel)
}

#[tauri::command]
pub async fn start_tunnel(app: AppHandle, state: State<'_, ConfigState>) -> AppResult<()> {
    let cfg = current(&state);
    let token = cf()?.tunnel_token(&account_id(&cfg)?, &tunnel_id(&cfg)?).await?;
    app.state::<Connector>().start(&app, token)
}

#[tauri::command]
pub fn stop_tunnel(app: AppHandle, connector: State<Connector>) {
    connector.stop(&app);
}

// ---- routes -----------------------------------------------------------------

/// Map `subdomain.zone` to a local port. The route is saved, and so shown, only once
/// https://hostname loads through Cloudflare; if any step fails, earlier ones are undone.
#[tauri::command]
pub async fn add_route(
    app: AppHandle,
    lock: State<'_, RouteLock>,
    zone_id: String,
    zone_name: String,
    subdomain: String,
    port: u16,
) -> AppResult<Route> {
    let _guard = lock.0.lock().await;
    let sub = subdomain.trim().trim_end_matches('.').to_lowercase();
    validate_label(&sub)?;
    if port == 0 {
        return Err(AppError::Invalid("Port must be between 1 and 65535".into()));
    }
    let hostname = if sub.is_empty() || sub == "@" { zone_name.clone() } else { format!("{sub}.{zone_name}") };

    let cfg = current(&app.state::<ConfigState>());
    if cfg.routes.iter().any(|r| r.hostname == hostname) {
        return Err(AppError::Invalid(format!("{hostname} is already mapped")));
    }
    let (account, tunnel) = (account_id(&cfg)?, tunnel_id(&cfg)?);

    // Catch what we can locally before changing anything in Cloudflare.
    if !matches!(app.state::<Connector>().status(), ConnectorStatus::Connected { .. }) {
        return Err(AppError::Invalid(
            "The tunnel isn't connected. Start it and wait for \"Live\", then add the route.".into(),
        ));
    }
    progress(&app, format!("Checking localhost:{port}…"));
    if !ports::is_listening(port).await {
        return Err(AppError::Invalid(format!(
            "Nothing is running on localhost:{port}. Start your app first, then add the route."
        )));
    }
    // Apps like Ollama or Vite refuse requests addressed to anything but localhost. Spot that
    // now and have cloudflared send them Host: localhost instead of a 403.
    let rewrite_host = verify::rejects_public_host(&hostname, port).await;
    if rewrite_host {
        progress(
            &app,
            format!("localhost:{port} only answers to localhost, so {hostname} will send it Host: localhost…"),
        );
    }
    let client = cf()?;

    let mut routes = cfg.routes.clone();
    routes.push(Route {
        hostname: hostname.clone(),
        zone_id: zone_id.clone(),
        port,
        dns_record_id: None,
        rewrite_host,
    });

    progress(&app, format!("Routing {hostname} through the tunnel…"));
    client.put_ingress(&account, &tunnel, &ingress_for(&routes)).await?;

    progress(&app, format!("Creating the DNS record for {hostname}…"));
    let (record, replaced) = match client.upsert_tunnel_cname(&zone_id, &hostname, &tunnel).await {
        Ok(v) => v,
        Err(e) => {
            let _ = client.put_ingress(&account, &tunnel, &ingress_for(&cfg.routes)).await;
            return Err(e);
        }
    };

    progress(&app, format!("Checking https://{hostname} loads…"));
    if let Err(why) = verify::wait_until_live(&hostname, port).await {
        let _ = match &replaced {
            Some(old) => client.restore_dns_record(&zone_id, old).await,
            None => client.delete_dns_record(&zone_id, &record.id).await,
        };
        let _ = client.put_ingress(&account, &tunnel, &ingress_for(&cfg.routes)).await;
        return Err(AppError::Invalid(format!(
            "https://{hostname} didn't load through Cloudflare ({why}), so the route was not added."
        )));
    }

    let route = Route { hostname, zone_id, port, dns_record_id: Some(record.id), rewrite_host };
    let saved = route.clone();
    update(&app, |c| c.routes.push(saved))?;
    Ok(route)
}

#[tauri::command]
pub async fn update_route_port(
    app: AppHandle,
    lock: State<'_, RouteLock>,
    hostname: String,
    port: u16,
) -> AppResult<()> {
    let _guard = lock.0.lock().await;
    if port == 0 {
        return Err(AppError::Invalid("Port must be between 1 and 65535".into()));
    }
    let cfg = current(&app.state::<ConfigState>());
    let mut routes = cfg.routes.clone();
    let r = routes
        .iter_mut()
        .find(|r| r.hostname == hostname)
        .ok_or_else(|| AppError::Invalid(format!("No route for {hostname}")))?;
    r.port = port;

    cf()?.put_ingress(&account_id(&cfg)?, &tunnel_id(&cfg)?, &ingress_for(&routes)).await?;
    update(&app, |c| c.routes = routes)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_route(app: AppHandle, lock: State<'_, RouteLock>, hostname: String) -> AppResult<()> {
    let _guard = lock.0.lock().await;
    let cfg = current(&app.state::<ConfigState>());
    let Some(route) = cfg.routes.iter().find(|r| r.hostname == hostname).cloned() else {
        return Ok(());
    };
    let remaining: Vec<Route> = cfg.routes.iter().filter(|r| r.hostname != hostname).cloned().collect();
    let client = cf()?;

    client.put_ingress(&account_id(&cfg)?, &tunnel_id(&cfg)?, &ingress_for(&remaining)).await?;
    // Already-deleted records are fine; anything else is surfaced.
    if let Some(id) = &route.dns_record_id
        && let Err(e) = client.delete_dns_record(&route.zone_id, id).await
        && !e.to_string().contains("81044")
    {
        update(&app, |c| c.routes = remaining.clone())?;
        return Err(AppError::Cloudflare(format!(
            "Route removed, but the DNS record for {hostname} could not be deleted: {e}"
        )));
    }
    update(&app, |c| c.routes = remaining)?;
    Ok(())
}

#[tauri::command]
pub async fn suggest_ports() -> Vec<u16> {
    ports::listening_ports().await
}

/// Which of `ports` have something listening, so the UI can flag routes whose app is down.
#[tauri::command]
pub async fn check_ports(ports: Vec<u16>) -> Vec<u16> {
    ports::listening_among(&ports).await
}

// ---- helpers ----------------------------------------------------------------

/// Cloudflare's "authentication error" / "permission denied" / "unauthorized" codes.
fn is_permission_error(e: &AppError) -> bool {
    matches!(e, AppError::Cloudflare(msg) if ["(code 10000)", "(code 1068)", "(code 9109)"].iter().any(|c| msg.contains(c)))
}

/// Cloudflare's "Invalid API Token". Also what the user-token check says about an account-owned token.
fn is_invalid_token(e: &AppError) -> bool {
    matches!(e, AppError::Cloudflare(msg) if msg.contains("(code 1000)"))
}

/// The accounts a token can see. Listing them needs "Account Settings: Read"; without it
/// Cloudflare returns an empty list rather than an error, so fall back to the accounts that
/// own the zones the token can see.
async fn visible_accounts(client: &Cloudflare) -> Vec<Account> {
    let mut accounts = client.list_accounts().await.unwrap_or_default();
    if accounts.is_empty() {
        for zone in client.list_zones(None).await.unwrap_or_default() {
            if !accounts.iter().any(|a| a.id == zone.account.id) {
                accounts.push(zone.account);
            }
        }
    }
    accounts
}

/// Cloudflare refusing a zone because the domain isn't registered (yet).
fn is_unregistered_error(e: &AppError) -> bool {
    matches!(e, AppError::Cloudflare(msg) if msg.contains("(code 1049)") || msg.contains("not a registered domain"))
}

/// Tell the UI which step of adding a route is running.
fn progress(app: &AppHandle, step: String) {
    let _ = app.emit(ROUTE_PROGRESS_EVENT, step);
}

fn validate_label(sub: &str) -> AppResult<()> {
    if sub.is_empty() || sub == "@" {
        return Ok(());
    }
    let ok = sub.split('.').all(|l| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    if ok { Ok(()) } else { Err(AppError::Invalid(format!("'{sub}' is not a valid subdomain"))) }
}

fn machine_tunnel_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().to_lowercase();
    let clean: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("tunnelpup-{}", if clean.is_empty() { "machine".into() } else { clean })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_cloudflare_error_codes_apart() {
        let invalid = AppError::Cloudflare("Invalid API Token (code 1000)".into());
        assert!(is_invalid_token(&invalid));
        assert!(!is_permission_error(&invalid));

        let denied = AppError::Cloudflare("Authentication error (code 10000)".into());
        assert!(is_permission_error(&denied));
        assert!(!is_invalid_token(&denied), "code 10000 isn't code 1000");
    }
}
