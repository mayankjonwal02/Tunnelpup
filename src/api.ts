// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Account = { id: string; name: string };
export type Zone = {
  id: string;
  name: string;
  status: "active" | "pending" | "initializing" | "moved" | string;
  name_servers: string[];
  account: Account;
};
export type Route = {
  hostname: string;
  zoneId: string;
  port: number;
  dnsRecordId: string | null;
  /** The app only answers to localhost, so the tunnel sends it Host: localhost. */
  rewriteHost: boolean;
};
export type TunnelInfo = { id: string; name: string };
export type AppConfig = {
  accountId: string | null;
  accountName: string | null;
  tunnel: TunnelInfo | null;
  routes: Route[];
};
export type ConnectorStatus =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "connected"; connections: number }
  | { state: "reconnecting"; retry_in_secs: number; reason: string };

/** A hint only: a name registered without nameservers also looks available. */
export type Availability = "available" | "taken" | "unknown";
/** Whether Cloudflare has issued a domain's HTTPS certificate yet. */
export type CertStatus = "ready" | "pending" | "unknown";

export type Snapshot = {
  hasToken: boolean;
  config: AppConfig;
  connector: ConnectorStatus;
  cloudflaredVersion: string;
  /** False until the updater has a real signing key, so checking for updates would only fail. */
  updatesConfigured: boolean;
};

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  logs: () => invoke<string[]>("get_connector_logs"),
  saveToken: (token: string) => invoke<Account[]>("save_token", { token }),
  selectAccount: (account: Account) => invoke<AppConfig>("select_account", { account }),
  signOut: () => invoke<void>("sign_out"),
  listZones: () => invoke<Zone[]>("list_zones"),
  addZone: (name: string) => invoke<Zone>("add_zone", { name }),
  checkZone: (zoneId: string) => invoke<Zone>("check_zone", { zoneId }),
  checkFreeDomain: (label: string, suffix: string) => invoke<Availability>("check_free_domain", { label, suffix }),
  checkHttps: (zoneName: string) => invoke<CertStatus>("check_https", { zoneName }),
  ensureTunnel: () => invoke<TunnelInfo>("ensure_tunnel"),
  startTunnel: () => invoke<void>("start_tunnel"),
  stopTunnel: () => invoke<void>("stop_tunnel"),
  addRoute: (zoneId: string, zoneName: string, subdomain: string, port: number) =>
    invoke<Route>("add_route", { zoneId, zoneName, subdomain, port }),
  updateRoutePort: (hostname: string, port: number) => invoke<void>("update_route_port", { hostname, port }),
  removeRoute: (hostname: string) => invoke<void>("remove_route", { hostname }),
  suggestPorts: () => invoke<number[]>("suggest_ports"),
  checkPorts: (ports: number[]) => invoke<number[]>("check_ports", { ports }),
};

export const onStatus = (cb: (s: ConnectorStatus) => void): Promise<UnlistenFn> =>
  listen<ConnectorStatus>("connector-status", (e) => cb(e.payload));
export const onLog = (cb: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>("connector-log", (e) => cb(e.payload));
export const onRouteProgress = (cb: (step: string) => void): Promise<UnlistenFn> =>
  listen<string>("route-progress", (e) => cb(e.payload));

export const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
