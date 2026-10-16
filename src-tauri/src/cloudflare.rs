//! Thin client for the Cloudflare v4 API endpoints TunnelPup needs.
//! Field names follow https://developers.cloudflare.com/api/ — re-check them
//! against the docs if a call starts failing after a Cloudflare change.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::error::{AppError, AppResult};

const API: &str = "https://api.cloudflare.com/client/v4";

/// The comment on the DNS records TunnelPup creates.
const MANAGED_COMMENT: &str = "Managed by TunnelPup";

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
    #[serde(default)]
    result_info: Option<ResultInfo>,
}

/// Paging details that list endpoints send next to `result`.
#[derive(Deserialize)]
struct ResultInfo {
    #[serde(default)]
    total_pages: u32,
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
        Ok(self.send_page(req).await?.0)
    }

    /// Every page of a list endpoint, which returns at most 50 items per page.
    async fn send_all<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Vec<T>> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let req = self.get(path).query(query).query(&[("page", page), ("per_page", 50)]);
            let (items, pages): (Vec<T>, u32) = self.send_page(req).await?;
            all.extend(items);
            if page >= pages {
                return Ok(all);
            }
            page += 1;
        }
    }

    /// `send`, plus how many pages the list has (1 for anything that isn't a list).
    async fn send_page<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> AppResult<(T, u32)> {
        let res = req.bearer_auth(&self.token).send().await?;
        let status = res.status();
        let text = res.text().await?;
        let env: Envelope<T> = serde_json::from_str(&text).map_err(|_| {
            AppError::Cloudflare(format!("Unexpected response (HTTP {status}): {}", truncate(&text, 300)))
        })?;
        if !env.success {
            let msg =
                env.errors.iter().map(|e| format!("{} (code {})", e.message, e.code)).collect::<Vec<_>>().join("; ");
            return Err(AppError::Cloudflare(if msg.is_empty() { format!("HTTP {status}") } else { msg }));
        }
        let pages = env.result_info.map_or(1, |i| i.total_pages);
        let result = env.result.ok_or_else(|| AppError::Cloudflare("Response had no result".into()))?;
        Ok((result, pages))
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
        self.send_all("/accounts", &[]).await
    }

    // ---- zones --------------------------------------------------------------

    /// Zones the token can see, optionally only those in one account.
    pub async fn list_zones(&self, account_id: Option<&str>) -> AppResult<Vec<Zone>> {
        let mut query = vec![("order", "name")];
        if let Some(id) = account_id {
            query.push(("account.id", id));
        }
        self.send_all("/zones", &query).await
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
        let req =
            self.get(&format!("/accounts/{account_id}/cfd_tunnel")).query(&[("name", name), ("is_deleted", "false")]);
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
    /// Only repoints a CNAME that TunnelPup created or that already points at this tunnel, and
    /// refuses any other record, since removing the route later deletes it: someone's website
    /// or their own tunnel setup must never be taken over.
    /// Also returns the CNAME it repointed, if any, so the change can be undone.
    pub async fn upsert_tunnel_cname(
        &self,
        zone_id: &str,
        hostname: &str,
        tunnel_id: &str,
    ) -> AppResult<(DnsRecord, Option<DnsRecord>)> {
        let target = format!("{tunnel_id}.cfargotunnel.com");
        let body = json!({
            "type": "CNAME",
            "name": hostname,
            "content": &target,
            "proxied": true,
            "comment": MANAGED_COMMENT,
        });
        match self.find_dns_record(zone_id, hostname).await? {
            Some(existing) if existing.record_type == "CNAME" && managed_by_us(&existing, &target) => {
                let rec =
                    self.send(self.patch(&format!("/zones/{zone_id}/dns_records/{}", existing.id)).json(&body)).await?;
                Ok((rec, Some(existing)))
            }
            Some(existing) if existing.record_type == "CNAME" => Err(AppError::Invalid(format!(
                "{hostname} already points to {}. TunnelPup only changes records it created: remove that record in Cloudflare first, or pick another name.",
                existing.content
            ))),
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

/// True for a CNAME TunnelPup may repoint: one it created, or one already aimed at `target`.
fn managed_by_us(rec: &DnsRecord, target: &str) -> bool {
    rec.comment.as_deref() == Some(MANAGED_COMMENT) || rec.content.trim_end_matches('.').eq_ignore_ascii_case(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cname(content: &str, comment: Option<&str>) -> DnsRecord {
        DnsRecord {
            id: "rec".into(),
            name: "app.example.com".into(),
            record_type: "CNAME".into(),
            content: content.into(),
            proxied: true,
            comment: comment.map(Into::into),
        }
    }

    #[test]
    fn repoints_only_cnames_it_owns() {
        let target = "29a6.cfargotunnel.com";
        assert!(managed_by_us(&cname("29a6.cfargotunnel.com", None), target));
        assert!(managed_by_us(&cname("29A6.cfargotunnel.com.", None), target));
        assert!(managed_by_us(&cname("old-tunnel.cfargotunnel.com", Some(MANAGED_COMMENT)), target));
        // Someone's website, and someone's own cloudflared setup.
        assert!(!managed_by_us(&cname("cname.vercel-dns.com", None), target));
        assert!(!managed_by_us(&cname("other-tunnel.cfargotunnel.com", Some("home server")), target));
    }

    #[test]
    fn reads_how_many_pages_a_list_has() {
        let list = r#"{"success":true,"errors":[],"result":[],"result_info":{"page":1,"per_page":50,"total_pages":3}}"#;
        let env: Envelope<Vec<Value>> = serde_json::from_str(list).unwrap();
        assert_eq!(env.result_info.map(|i| i.total_pages), Some(3));
        let single: Envelope<Value> = serde_json::from_str(r#"{"success":true,"errors":[],"result":{}}"#).unwrap();
        assert!(single.result_info.is_none());
    }
}
