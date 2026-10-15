//! Thin client for the Cloudflare v4 API endpoints TunnelPup needs.
//! Field names follow https://developers.cloudflare.com/api/ — re-check them
//! against the docs if a call starts failing after a Cloudflare change.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

const API: &str = "https://api.cloudflare.com/client/v4";

#[derive(Clone)]
pub struct Cloudflare {
    http: reqwest::Client,
    token: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<ApiMessage>,
    result: Option<T>,
}

#[derive(Deserialize)]
struct ApiMessage {
    code: i64,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStatus {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub name_servers: Vec<String>,
    pub account: Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tunnel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub content: String,
    #[serde(default)]
    pub proxied: bool,
    #[serde(default)]
    pub comment: Option<String>,
}

/// One ingress rule, as stored in the tunnel's remote configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    pub service: String,
    #[serde(rename = "originRequest", default, skip_serializing_if = "Option::is_none")]
    pub origin_request: Option<OriginRequest>,
}

/// Per-rule settings for how cloudflared talks to the local app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginRequest {
    /// Host header to send instead of the public hostname.
    #[serde(rename = "httpHostHeader")]
    pub http_host_header: String,
}

impl Cloudflare {
    pub fn new(token: impl Into<String>) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("TunnelPup/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { http, token: token.into() })
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> AppResult<T> {
        let res = req.bearer_auth(&self.token).send().await?;
        let status = res.status();
        let text = res.text().await?;
        let env: Envelope<T> = serde_json::from_str(&text).map_err(|_| {
            AppError::Cloudflare(format!("Unexpected response (HTTP {status}): {}", truncate(&text, 300)))
        })?;
        if !env.success {
            let msg = env
                .errors
                .iter()
                .map(|e| format!("{} (code {})", e.message, e.code))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(AppError::Cloudflare(if msg.is_empty() { format!("HTTP {status}") } else { msg }));
        }
        env.result.ok_or_else(|| AppError::Cloudflare("Response had no result".into()))
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.get(format!("{API}{path}"))
    }
    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.post(format!("{API}{path}"))
    }
    fn put(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.put(format!("{API}{path}"))
    }
    fn patch(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.patch(format!("{API}{path}"))
    }
    fn delete(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.delete(format!("{API}{path}"))
    }

    // ---- token & account ----------------------------------------------------

    /// Verify a user API token.
    pub async fn verify_token(&self) -> AppResult<TokenStatus> {
        self.send(self.get("/user/tokens/verify")).await
    }

    /// Verify an account-owned API token, which `verify_token` rejects as invalid.
    pub async fn verify_account_token(&self, account_id: &str) -> AppResult<TokenStatus> {
        self.send(self.get(&format!("/accounts/{account_id}/tokens/verify"))).await
    }

    pub async fn list_accounts(&self) -> AppResult<Vec<Account>> {
        self.send(self.get("/accounts?per_page=50")).await
    }

    // ---- zones --------------------------------------------------------------

    /// Zones the token can see, optionally only those in one account.
    pub async fn list_zones(&self, account_id: Option<&str>) -> AppResult<Vec<Zone>> {
        let mut req = self.get("/zones?per_page=50&order=name");
        if let Some(id) = account_id {
            req = req.query(&[("account.id", id)]);
        }
        self.send(req).await
    }

    pub async fn get_zone(&self, zone_id: &str) -> AppResult<Zone> {
        self.send(self.get(&format!("/zones/{zone_id}"))).await
    }

    pub async fn add_zone(&self, account_id: &str, name: &str) -> AppResult<Zone> {
        let body = json!({ "name": name, "account": { "id": account_id }, "type": "full" });
        self.send(self.post("/zones").json(&body)).await
    }

    /// Ask Cloudflare to re-check a pending zone's nameservers now. Cloudflare rate-limits this.
    pub async fn activation_check(&self, zone_id: &str) -> AppResult<()> {
        let _: Value = self.send(self.put(&format!("/zones/{zone_id}/activation_check"))).await?;
        Ok(())
    }

    // ---- tunnels ------------------------------------------------------------

    pub async fn find_tunnel(&self, account_id: &str, name: &str) -> AppResult<Option<Tunnel>> {
        let req = self
            .get(&format!("/accounts/{account_id}/cfd_tunnel"))
            .query(&[("name", name), ("is_deleted", "false")]);
        let list: Vec<Tunnel> = self.send(req).await?;
        Ok(list.into_iter().find(|t| t.name == name))
    }

    pub async fn create_tunnel(&self, account_id: &str, name: &str) -> AppResult<Tunnel> {
        let body = json!({ "name": name, "config_src": "cloudflare" });
        self.send(self.post(&format!("/accounts/{account_id}/cfd_tunnel")).json(&body)).await
    }

    /// The connector token cloudflared needs to run this tunnel. Keep it in memory only.
    pub async fn tunnel_token(&self, account_id: &str, tunnel_id: &str) -> AppResult<String> {
        self.send(self.get(&format!("/accounts/{account_id}/cfd_tunnel/{tunnel_id}/token"))).await
    }

    /// Replace the tunnel's ingress rules. A running connector picks this up without a restart.
    pub async fn put_ingress(&self, account_id: &str, tunnel_id: &str, rules: &[IngressRule]) -> AppResult<()> {
        let mut ingress: Vec<IngressRule> = rules.to_vec();
        // Cloudflare requires a final catch-all rule.
        ingress.push(IngressRule { hostname: None, service: "http_status:404".into(), origin_request: None });
        let body = json!({ "config": { "ingress": ingress } });
        let _: Value = self
            .send(self.put(&format!("/accounts/{account_id}/cfd_tunnel/{tunnel_id}/configurations")).json(&body))
            .await?;
        Ok(())
    }

    // ---- DNS ----------------------------------------------------------------

    pub async fn find_dns_record(&self, zone_id: &str, hostname: &str) -> AppResult<Option<DnsRecord>> {
        let req = self.get(&format!("/zones/{zone_id}/dns_records")).query(&[("name", hostname)]);
        let list: Vec<DnsRecord> = self.send(req).await?;
        Ok(list.into_iter().next())
    }

    /// Create (or repoint) a proxied CNAME `hostname -> <tunnel_id>.cfargotunnel.com`.
    /// Refuses to overwrite a record that is not a CNAME, so we never clobber someone's A record.
    /// Also returns the CNAME it repointed, if any, so the change can be undone.
    pub async fn upsert_tunnel_cname(
        &self,
        zone_id: &str,
        hostname: &str,
        tunnel_id: &str,
    ) -> AppResult<(DnsRecord, Option<DnsRecord>)> {
        let body = json!({
            "type": "CNAME",
            "name": hostname,
            "content": format!("{tunnel_id}.cfargotunnel.com"),
            "proxied": true,
            "comment": "Managed by TunnelPup",
        });
        match self.find_dns_record(zone_id, hostname).await? {
            Some(existing) if existing.record_type == "CNAME" => {
                let rec = self.send(self.patch(&format!("/zones/{zone_id}/dns_records/{}", existing.id)).json(&body)).await?;
                Ok((rec, Some(existing)))
            }
            Some(existing) => Err(AppError::Invalid(format!(
                "{hostname} already has a {} record. Remove it in the Cloudflare dashboard first.",
                existing.record_type
            ))),
            None => Ok((self.send(self.post(&format!("/zones/{zone_id}/dns_records")).json(&body)).await?, None)),
        }
    }

    /// Put back a CNAME exactly as it was before `upsert_tunnel_cname` repointed it.
    pub async fn restore_dns_record(&self, zone_id: &str, old: &DnsRecord) -> AppResult<()> {
        let body = json!({
            "content": old.content,
            "proxied": old.proxied,
            "comment": old.comment.as_deref().unwrap_or(""),
        });
        let _: Value = self.send(self.patch(&format!("/zones/{zone_id}/dns_records/{}", old.id)).json(&body)).await?;
        Ok(())
    }

    pub async fn delete_dns_record(&self, zone_id: &str, record_id: &str) -> AppResult<()> {
        let _: Value = self.send(self.delete(&format!("/zones/{zone_id}/dns_records/{record_id}"))).await?;
        Ok(())
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}
