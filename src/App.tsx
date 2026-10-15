import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as autostartEnabled } from "@tauri-apps/plugin-autostart";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import cloudflaredLicense from "./licenses/cloudflared-LICENSE.txt?raw";
import {
  api,
  errorText,
  onLog,
  onRouteProgress,
  onStatus,
  type Availability,
  type CertStatus,
  type ConnectorStatus,
  type Route,
  type Snapshot,
  type Zone,
} from "./api";
import { GuideContent, type GuideSection } from "./Guide";
import { loadTheme, saveTheme, type ThemeChoice } from "./theme";
import "./App.css";

// Cloudflare's "Create token" form, pre-filled. It silently drops permission keys it doesn't
// know, and the key for Cloudflare Tunnel isn't documented, so users add that one row by hand
// (save_token then checks it's there).
const TOKEN_PERMISSIONS = [
  { key: "dns", type: "edit" },
  { key: "zone", type: "edit" },
  { key: "account_settings", type: "read" },
];
const TOKEN_URL =
  "https://dash.cloudflare.com/profile/api-tokens" +
  `?permissionGroupKeys=${encodeURIComponent(JSON.stringify(TOKEN_PERMISSIONS))}&accountId=*&zoneId=all&name=PortHop`;

type Run = (fn: () => Promise<unknown>, onError?: (message: string) => void) => void;

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const refresh = useCallback(() => api.snapshot().then(setSnap).catch((e) => setLoadError(errorText(e))), []);

  useEffect(() => {
    refresh();
    const un = onStatus((connector) => setSnap((s) => (s ? { ...s, connector } : s)));
    return () => void un.then((f) => f());
  }, [refresh]);

  if (loadError) return <div className="center error">{loadError}</div>;
  if (!snap) return <div className="center muted">Loading…</div>;
  return snap.hasToken ? <Dashboard snap={snap} refresh={refresh} /> : <Setup onDone={refresh} />;
}

// ---------------------------------------------------------------------------
// First run: paste token
// ---------------------------------------------------------------------------

function Setup({ onDone }: { onDone: () => void }) {
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [guide, setGuide] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.saveToken(token);
      onDone();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="setup">
      <h1 className="brand">PortHop</h1>
      <p className="lede">
        Put localhost:3000 on app.yourdomain.com, through Cloudflare.{" "}
        <a href="#" onClick={(e) => (e.preventDefault(), setGuide(true))}>New here? Read the guide</a>
      </p>

      <form className="card" onSubmit={submit}>
        <h2>Connect your Cloudflare account</h2>
        <ol className="steps">
          <li>
            <div className="row between">
              <strong>Create a token on Cloudflare</strong>
              <button type="button" className="primary small" onClick={() => openUrl(TOKEN_URL)}>Create token ↗</button>
            </div>
            <p className="muted small">
              The form opens with DNS, Zone and Account Settings filled in. Add one more permission,{" "}
              <strong>Account → Cloudflare Tunnel → Edit</strong>, then choose Continue to summary → Create Token.
            </p>
          </li>
          <li>
            <strong>Paste it here</strong>
            <div className="add-domain">
              <input type="password" autoFocus value={token} onChange={(e) => setToken(e.target.value)} placeholder="Paste token" spellCheck={false} />
              <button className="primary" disabled={busy || !token.trim()}>{busy ? "Checking…" : "Connect"}</button>
            </div>
            {error && <p className="error small">{error}</p>}
          </li>
        </ol>
        <details className="why">
          <summary>Why these permissions?</summary>
          <ul className="perms">
            <li><code>Account · Cloudflare Tunnel · Edit</code> — create and run this computer's tunnel</li>
            <li><code>Account · Account Settings · Read</code> — find your Cloudflare account</li>
            <li><code>Zone · DNS · Edit</code> — point your subdomains at the tunnel</li>
            <li><code>Zone · Zone · Edit</code> — add domains from PortHop, including free ones</li>
          </ul>
        </details>
        <p className="muted small">The token is stored in your system keychain, never in a file.</p>
      </form>

      {guide && (
        <Drawer title="Guide" wide onClose={() => setGuide(false)}>
          <GuideContent />
        </Drawer>
      )}
    </main>
  );
}

// ---------------------------------------------------------------------------
// Main screen
// ---------------------------------------------------------------------------

function Dashboard({ snap, refresh }: { snap: Snapshot; refresh: () => void }) {
  const { config, connector } = snap;
  const [zones, setZones] = useState<Zone[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [panel, setPanel] = useState<"none" | "logs" | "settings" | "domains" | "guide">("none");
  const [domainsMode, setDomainsMode] = useState<"own" | "free">("own");
  const [guideSection, setGuideSection] = useState<GuideSection | undefined>();
  const openDomains = (mode: "own" | "free" = "own") => {
    setDomainsMode(mode);
    setPanel("domains");
  };
  const openGuide = (section?: GuideSection) => {
    setGuideSection(section);
    setPanel("guide");
  };

  const loadZones = useCallback(() => api.listZones().then(setZones).catch((e) => setError(errorText(e))), []);
  useEffect(() => void loadZones(), [loadZones]);

  // Cloudflare issues a domain's HTTPS certificate a while after the domain goes active
  // (usually 15 minutes to a few hours). Until then no https:// route on it can load.
  const [certs, setCerts] = useState<Record<string, CertStatus>>({});
  const checkCert = useCallback(
    (name: string) => api.checkHttps(name).then((s) => setCerts((c) => ({ ...c, [name]: s }))).catch(() => {}),
    [],
  );
  const awaitingCert = zones.filter((z) => z.status === "active" && certs[z.name] !== "ready").map((z) => z.name).join(" ");
  useEffect(() => {
    if (!awaitingCert) return;
    const names = awaitingCert.split(" ");
    names.forEach(checkCert);
    const t = setInterval(() => names.forEach(checkCert), 30_000);
    return () => clearInterval(t);
  }, [awaitingCert, checkCert]);

  // Errors go to the banner at the top unless the caller shows them next to what was clicked.
  const run: Run = async (fn, onError) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      refresh();
    } catch (e) {
      (onError ?? setError)(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // There's nothing to decide about the tunnel, so create it as soon as the user is signed in.
  const [tunnelError, setTunnelError] = useState<string | null>(null);
  const createTunnel = () => {
    setTunnelError(null);
    run(api.ensureTunnel, setTunnelError);
  };
  const tunnelRequested = useRef(false);
  useEffect(() => {
    if (config.tunnel || tunnelRequested.current) return;
    tunnelRequested.current = true;
    createTunnel();
  });

  // Routes whose app isn't running: visitors get an error page until it's started again.
  const [down, setDown] = useState<Set<number>>(new Set());
  const routePorts = [...new Set(config.routes.map((r) => r.port))].join(",");
  useEffect(() => {
    if (!routePorts) {
      setDown(new Set());
      return;
    }
    const ports = routePorts.split(",").map(Number);
    const probe = () =>
      api.checkPorts(ports).then((up) => setDown(new Set(ports.filter((p) => !up.includes(p))))).catch(() => {});
    probe();
    const t = setInterval(probe, 15_000);
    return () => clearInterval(t);
  }, [routePorts]);

  const running = connector.state !== "stopped";
  const routeCount = config.routes.length;

  return (
    <main className="app">
      <header className="top">
        <div>
          <h1 className="brand">PortHop</h1>
          <span className="muted small">{config.accountName}{config.tunnel && ` · ${config.tunnel.name}`}</span>
        </div>
        <div className="row">
          <StatusPill status={connector} />
          {config.tunnel && (
            <button disabled={busy} onClick={() => run(running ? api.stopTunnel : api.startTunnel)}>
              {running ? "Stop" : "Start"}
            </button>
          )}
          <button className="ghost" onClick={() => (panel === "domains" ? setPanel("none") : openDomains())}>Domains</button>
          <button className="ghost" onClick={() => setPanel(panel === "logs" ? "none" : "logs")}>Logs</button>
          <button className="ghost" onClick={() => (panel === "guide" ? setPanel("none") : openGuide())}>Guide</button>
          <button className="ghost" onClick={() => setPanel(panel === "settings" ? "none" : "settings")}>Settings</button>
        </div>
      </header>

      {config.tunnel && connector.state === "stopped" && (
        <div className="banner banner-warn">
          <span>
            The tunnel is stopped
            {routeCount > 0 && `, so ${routeCount === 1 ? "your route is" : `your ${routeCount} routes are`} offline`}.
          </span>
          <button className="small" disabled={busy} onClick={() => run(api.startTunnel)}>Start</button>
        </div>
      )}
      {connector.state === "reconnecting" && (
        <div className="banner banner-warn">
          <span>The tunnel dropped ({connector.reason}). PortHop retries in {connector.retry_in_secs}s.</span>
          <button className="small ghost" onClick={() => setPanel("logs")}>View logs</button>
          <button className="small ghost" onClick={() => openGuide("troubleshooting")}>Help</button>
        </div>
      )}
      {error && <div className="banner error" onClick={() => setError(null)}>{error}</div>}

      {routeCount === 0 ? (
        <GetStarted
          snap={snap}
          zones={zones}
          certs={certs}
          busy={busy}
          tunnelError={tunnelError}
          onRetryTunnel={createTunnel}
          onStart={() => run(api.startTunnel)}
          onOpenDomains={openDomains}
        />
      ) : (
        <Routes routes={config.routes} down={down} offline={connector.state !== "connected"} run={run} busy={busy} />
      )}
      {config.tunnel && (
        <AddRoute zones={zones} certs={certs} routes={config.routes} run={run} busy={busy} onAddDomain={() => openDomains()} />
      )}

      {panel === "logs" && <Logs onClose={() => setPanel("none")} />}
      {panel === "settings" && <Settings snap={snap} onClose={() => setPanel("none")} onSignedOut={refresh} />}
      {panel === "domains" && (
        <Domains
          zones={zones}
          certs={certs}
          accountId={config.accountId}
          accountName={config.accountName}
          initialMode={domainsMode}
          onClose={() => setPanel("none")}
          onChanged={loadZones}
          onCheckCert={checkCert}
        />
      )}
      {panel === "guide" && (
        <Drawer title="Guide" wide onClose={() => setPanel("none")}>
          <GuideContent section={guideSection} />
        </Drawer>
      )}
    </main>
  );
}

/** First-run checklist, shown until the first route exists. */
function GetStarted({ snap, zones, certs, busy, tunnelError, onRetryTunnel, onStart, onOpenDomains }: {
  snap: Snapshot;
  zones: Zone[];
  certs: Record<string, CertStatus>;
  busy: boolean;
  tunnelError: string | null;
  onRetryTunnel: () => void;
  onStart: () => void;
  onOpenDomains: (mode?: "own" | "free") => void;
}) {
  const { config, connector } = snap;
  const ready = zones.filter((z) => z.status === "active" && certs[z.name] !== "pending");
  const awaitingCert = zones.find((z) => z.status === "active" && certs[z.name] === "pending");
  const awaitingNs = zones.find((z) => z.status !== "active");
  const tunnelLive = Boolean(config.tunnel) && connector.state === "connected";

  const tunnelStep =
    !config.tunnel
      ? tunnelError
        ? { text: `Couldn't set it up: ${tunnelError}`, error: true }
        : { text: "Setting up a tunnel for this computer…" }
      : connector.state === "connected"
        ? { text: "Running. No ports are opened on your router." }
        : connector.state === "stopped"
          ? { text: "The tunnel is stopped." }
          : { text: "Connecting to Cloudflare…" };

  const domainText =
    ready.length > 0
      ? `${ready.map((z) => z.name).join(", ")} ${ready.length === 1 ? "is" : "are"} ready.`
      : awaitingCert
        ? `${awaitingCert.name} is set up. Cloudflare is issuing its HTTPS certificate, which usually takes 15 minutes to a few hours.`
        : awaitingNs
          ? `${awaitingNs.name} is waiting for its nameservers. Finish the steps in Domains.`
          : "Use a domain you own, or get a free one in a few minutes.";

  return (
    <section className="card">
      <h2>Get started</h2>
      <ol className="steps">
        <li className="done">
          <strong>Connect to Cloudflare</strong>
          <p className="muted small">Signed in to {config.accountName}.</p>
        </li>
        <li className={tunnelLive ? "done" : ""}>
          <div className="row between">
            <strong>Start this computer's tunnel</strong>
            {!config.tunnel && tunnelError && <button className="small" disabled={busy} onClick={onRetryTunnel}>Try again</button>}
            {config.tunnel && connector.state === "stopped" && <button className="small" disabled={busy} onClick={onStart}>Start</button>}
          </div>
          <p className={tunnelStep.error ? "error small" : "muted small"}>{tunnelStep.text}</p>
        </li>
        <li className={ready.length > 0 ? "done" : ""}>
          <div className="row between">
            <strong>Add a domain</strong>
            {ready.length === 0 &&
              (awaitingCert || awaitingNs ? (
                <button className="small" onClick={() => onOpenDomains()}>Open Domains</button>
              ) : (
                <span className="row">
                  <button className="primary small" onClick={() => onOpenDomains("free")}>Get a free domain</button>
                  <button className="small" onClick={() => onOpenDomains("own")}>I have a domain</button>
                </span>
              ))}
          </div>
          <p className="muted small">{domainText}</p>
        </li>
        <li>
          <strong>Add your first route</strong>
          <p className="muted small">
            {ready.length > 0 && config.tunnel
              ? "Use the form below: pick a name, a domain and the port your app runs on."
              : "Comes after the steps above."}
          </p>
        </li>
      </ol>
    </section>
  );
}

function StatusPill({ status }: { status: ConnectorStatus }) {
  const text =
    status.state === "connected" ? "Live"
    : status.state === "starting" ? "Connecting…"
    : status.state === "reconnecting" ? `Retrying in ${status.retry_in_secs}s`
    : "Stopped";
  const title = status.state === "reconnecting" ? status.reason : status.state === "connected" ? `${status.connections} edge connections` : undefined;
  return <span className={`pill ${status.state}`} title={title}>{text}</span>;
}

function Routes({ routes, down, offline, run, busy }: {
  routes: Route[];
  down: Set<number>;
  offline: boolean;
  run: Run;
  busy: boolean;
}) {
  return (
    <section className={offline ? "card offline" : "card"}>
      <h2>Routes</h2>
      <ul className="routes">
        {routes.map((r) => (
          <li key={r.hostname}>
            <a href="#" className="host" onClick={(e) => (e.preventDefault(), openUrl(`https://${r.hostname}`))}>https://{r.hostname}</a>
            <span className="target">
              <span className="arrow">→</span>
              <PortEditor port={r.port} disabled={busy} onSave={(p) => run(() => api.updateRoutePort(r.hostname, p))} />
            </span>
            <span className="flags">
              {down.has(r.port) && (
                <span
                  className="tag bad"
                  title={`Nothing is listening on localhost:${r.port}, so visitors get an error page. Start the app and this clears by itself.`}
                >
                  App not running
                </span>
              )}
              {r.rewriteHost && (
                <span className="tag" title="This app only answers requests addressed to localhost, so PortHop has the tunnel send it Host: localhost.">
                  Host: localhost
                </span>
              )}
            </span>
            <span className="actions">
              <CopyButton text={`https://${r.hostname}`} label="Copy link" />
              <button className="ghost danger small" disabled={busy} onClick={() => confirm(`Remove ${r.hostname}?`) && run(() => api.removeRoute(r.hostname))}>
                Remove
              </button>
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function PortEditor({ port, disabled, onSave }: { port: number; disabled: boolean; onSave: (p: number) => void }) {
  const [value, setValue] = useState(String(port));
  useEffect(() => setValue(String(port)), [port]);
  const n = Number(value);
  const dirty = n !== port && Number.isInteger(n) && n > 0 && n < 65536;
  return (
    <span className="port">
      localhost:
      <input value={value} inputMode="numeric" onChange={(e) => setValue(e.target.value.replace(/\D/g, ""))} onKeyDown={(e) => e.key === "Enter" && dirty && onSave(n)} />
      {dirty && <button className="small" disabled={disabled} onClick={() => onSave(n)}>Save</button>}
    </span>
  );
}

function AddRoute({ zones, certs, routes, run, busy, onAddDomain }: {
  zones: Zone[];
  certs: Record<string, CertStatus>;
  routes: Route[];
  run: Run;
  busy: boolean;
  onAddDomain: () => void;
}) {
  // A domain is only usable once Cloudflare has its HTTPS certificate; until then it's listed but disabled.
  const active = zones.filter((z) => z.status === "active" && certs[z.name] !== "pending");
  const awaitingCert = zones.filter((z) => z.status === "active" && certs[z.name] === "pending");
  const [zoneId, setZoneId] = useState("");
  const [sub, setSub] = useState("");
  const [port, setPort] = useState("");
  const [suggested, setSuggested] = useState<number[]>([]);
  const [step, setStep] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [added, setAdded] = useState<Route | null>(null);
  const routed = new Set(routes.map((r) => r.port));
  const freePorts = suggested.filter((p) => !routed.has(p));

  useEffect(() => {
    if (!zoneId && active[0]) setZoneId(active[0].id);
  }, [active, zoneId]);
  useEffect(() => void api.suggestPorts().then(setSuggested), []);
  useEffect(() => {
    const un = onRouteProgress(setStep);
    return () => void un.then((f) => f());
  }, []);

  const zone = active.find((z) => z.id === zoneId);
  const portNum = Number(port);
  const valid = zone && sub.trim() && portNum > 0 && portNum < 65536;

  // The route only appears in the list once the backend has confirmed it loads.
  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!valid || !zone) return;
    setError(null);
    setAdded(null);
    run(async () => {
      try {
        setAdded(await api.addRoute(zone.id, zone.name, sub, portNum));
        setSub("");
        setPort("");
      } finally {
        setStep(null);
      }
    }, setError);
  };

  return (
    <form className="card" onSubmit={submit}>
      <h2>Add a route</h2>
      <div className="add-row">
        <input placeholder="app" value={sub} onChange={(e) => setSub(e.target.value)} spellCheck={false} />
        <span>.</span>
        <select value={zoneId} onChange={(e) => (e.target.value === "__add" ? onAddDomain() : setZoneId(e.target.value))}>
          {/* Without a placeholder, "Add a domain" would be pre-selected and choosing it would do nothing. */}
          {!active.length && <option value="" disabled>{awaitingCert.length ? "No domains ready yet" : "No domains yet"}</option>}
          {active.map((z) => <option key={z.id} value={z.id}>{z.name}</option>)}
          {awaitingCert.map((z) => <option key={z.id} value={z.id} disabled>{z.name}: HTTPS certificate pending</option>)}
          <option value="__add">+ Add a domain…</option>
        </select>
        <span className="arrow">→ localhost:</span>
        <input className="narrow" placeholder="3000" value={port} inputMode="numeric" onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))} />
        <button className="primary" disabled={busy || !valid}>{step ? "Adding…" : "Add"}</button>
      </div>
      {step && <p className="muted small step">{step}</p>}
      {error && <p className="error small step">{error}</p>}
      {added && (
        <div className="notice">
          <span>
            ✓ <a href="#" onClick={(e) => (e.preventDefault(), openUrl(`https://${added.hostname}`))}>https://{added.hostname}</a> is live.
          </span>
          <CopyButton text={`https://${added.hostname}`} label="Copy link" />
          <button type="button" className="ghost small" aria-label="Dismiss" onClick={() => setAdded(null)}>✕</button>
        </div>
      )}
      {freePorts.length > 0 && (
        <div className="chips">
          <span className="muted small">Listening now:</span>
          {freePorts.map((p) => <button type="button" key={p} className="chip" onClick={() => setPort(String(p))}>{p}</button>)}
        </div>
      )}
      <p className="warn small">Anything you map is public on the internet. Protect admin panels and dev servers.</p>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

function Drawer({ title, onClose, wide, children }: { title: string; onClose: () => void; wide?: boolean; children: React.ReactNode }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className={wide ? "drawer wide" : "drawer"} onClick={(e) => e.stopPropagation()}>
        <header className="row between"><h2>{title}</h2><button className="ghost" onClick={onClose}>Close</button></header>
        {children}
      </aside>
    </div>
  );
}

type LogLevel = "debug" | "info" | "warn" | "error" | "porthop" | "plain";
type LogEntry = { raw: string; body: string; time: string | null; level: LogLevel; message: string; fields: [string, string][] };
type LogGroup = LogEntry & { count: number };

const LOG_LEVELS: Record<string, LogLevel> = { DBG: "debug", INF: "info", WRN: "warn", ERR: "error", FTL: "error" };
// PortHop's own lines are about the connector failing or exiting, so they count as warnings.
const LOG_RANK: Record<LogLevel, number> = { debug: 0, info: 1, plain: 1, warn: 2, porthop: 2, error: 3 };
const LOG_HEAD = /^(\d{4}-\d\d-\d\dT[\d:.]+Z) (DBG|INF|WRN|ERR|FTL) /;
const LOG_FIELD = /\s([A-Za-z_][\w.-]*)=("(?:[^"\\]|\\.)*"|\S*)/g;

const unquote = (v: string) => {
  if (!v.startsWith('"')) return v;
  try { return JSON.parse(v) as string; } catch { return v.slice(1, -1); }
};

/** Split a cloudflared line (`<time> <LVL> message key=value …`) into its parts. */
function parseLog(raw: string): LogEntry {
  if (raw.startsWith("[porthop]")) {
    const message = raw.slice("[porthop]".length).trim();
    return { raw, body: raw, time: null, level: "porthop", message, fields: [] };
  }
  const head = LOG_HEAD.exec(raw);
  if (!head) return { raw, body: raw, time: null, level: "plain", message: raw, fields: [] };
  const rest = raw.slice(head[0].length);
  const cut = rest.search(/\s[A-Za-z_][\w.-]*=/);
  const fields = cut < 0 ? [] : [...rest.slice(cut).matchAll(LOG_FIELD)].map(([, k, v]): [string, string] => [k, unquote(v)]);
  return { raw, body: raw.slice(head[1].length), time: head[1], level: LOG_LEVELS[head[2]], message: cut < 0 ? rest : rest.slice(0, cut), fields };
}

/** Collapse runs of identical lines (cloudflared repeats some warnings several times). */
function groupRepeats(entries: LogEntry[]): LogGroup[] {
  const out: LogGroup[] = [];
  for (const e of entries) {
    const last = out[out.length - 1];
    if (last && last.body === e.body) last.count++;
    else out.push({ ...e, count: 1 });
  }
  return out;
}

const clock = (iso: string) =>
  new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

function Logs({ onClose }: { onClose: () => void }) {
  const [lines, setLines] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [minRank, setMinRank] = useState(0);
  const [follow, setFollow] = useState(true);
  const [copyNote, setCopyNote] = useState<string | null>(null);
  const view = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.logs().then(setLines);
    const un = onLog((l) => setLines((prev) => [...prev.slice(-299), l]));
    return () => void un.then((f) => f());
  }, []);

  const entries = useMemo(() => lines.map(parseLog), [lines]);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((e) => LOG_RANK[e.level] >= minRank && (!q || e.raw.toLowerCase().includes(q)));
  }, [entries, query, minRank]);
  const groups = useMemo(() => groupRepeats(shown), [shown]);
  const atLeast = (rank: number) => entries.filter((e) => LOG_RANK[e.level] >= rank).length;
  const filters = [["All", 0], ["Warnings", 2], ["Errors", 3]] as const;

  // Stay on the newest line unless the user has scrolled up to read.
  useEffect(() => {
    if (follow && view.current) view.current.scrollTop = view.current.scrollHeight;
  }, [groups, follow]);
  const onScroll = () => {
    const el = view.current;
    if (el) setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 24);
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(shown.map((e) => e.raw).join("\n"));
      setCopyNote("Copied");
    } catch {
      setCopyNote("Couldn't copy");
    }
    setTimeout(() => setCopyNote(null), 1500);
  };

  return (
    <Drawer title="Connector logs" onClose={onClose} wide>
      <div className="log-tools">
        <input type="search" placeholder="Filter lines…" value={query} onChange={(e) => setQuery(e.target.value)} spellCheck={false} />
        <div className="seg" role="group" aria-label="Show">
          {filters.map(([label, rank]) => (
            <button key={label} className={minRank === rank ? "on" : ""} onClick={() => setMinRank(rank)}>
              {label} <span className="count">{atLeast(rank)}</span>
            </button>
          ))}
        </div>
        <button onClick={copy} disabled={!shown.length}>{copyNote ?? "Copy"}</button>
      </div>
      <div className="log-box">
        <div className="log-view" ref={view} onScroll={onScroll}>
          {groups.length ? (
            groups.map((g, i) => <LogRow key={i} entry={g} />)
          ) : (
            <p className="log-empty">{lines.length ? "No lines match this filter." : "No output yet. Start the tunnel to see what the connector is doing."}</p>
          )}
        </div>
        {!follow && <button className="jump small" onClick={() => setFollow(true)}>↓ Latest</button>}
      </div>
      <p className="muted small">
        {shown.length === entries.length ? `${entries.length} lines` : `${shown.length} of ${entries.length} lines`} · PortHop keeps the last 300
      </p>
    </Drawer>
  );
}

function LogRow({ entry: e }: { entry: LogGroup }) {
  return (
    <div className={`log-row lvl-${e.level}`}>
      <time dateTime={e.time ?? undefined} title={e.time ?? undefined}>{e.time ? clock(e.time) : ""}</time>
      <span className="lvl">{e.level === "plain" ? "" : e.level}</span>
      {/* Real spaces between parts, not margins, so long lines can wrap between fields. */}
      <span className="msg">
        {e.message}
        {e.fields.map(([k, v], i) => (
          <span key={i}>
            {(i > 0 || e.message) && " "}
            <span className={k === "error" || k === "err" ? "kv err" : "kv"}><span className="k">{k}=</span>{v}</span>
          </span>
        ))}
        {e.count > 1 && <> <span className="repeat" title={`Repeated ${e.count} times`}>×{e.count}</span></>}
      </span>
    </div>
  );
}

// DigitalPlat's free suffixes that Cloudflare accepts. Keep in sync with SUFFIXES in src-tauri/src/freedomain.rs.
const FREE_SUFFIXES = ["dpdns.org", "us.kg", "qzz.io", "xx.kg"];
const DIGITALPLAT_URL = "https://dash.domain.digitalplat.org/";
const isFreeDomain = (name: string) => FREE_SUFFIXES.some((s) => name.endsWith(`.${s}`));

const ZONE_STATUS: Record<string, string> = {
  active: "Active",
  pending: "Waiting for nameservers",
  initializing: "Waiting for nameservers",
  moved: "Nameservers moved away",
  deactivated: "Deactivated",
};

/** The connected account's domains, plus adding a new one and walking it through activation. */
function Domains({ zones, certs, accountId, accountName, initialMode = "own", onClose, onChanged, onCheckCert }: {
  zones: Zone[];
  certs: Record<string, CertStatus>;
  accountId: string | null;
  accountName: string | null;
  initialMode?: "own" | "free";
  onClose: () => void;
  onChanged: () => void;
  onCheckCert: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState<string | null>(null);
  const [mode, setMode] = useState(initialMode);

  // A domain turns active once its nameservers point at Cloudflare; keep re-checking until then.
  const waiting = zones.some((z) => z.status !== "active");
  useEffect(() => {
    if (!waiting) return;
    const t = setInterval(onChanged, 30_000);
    return () => clearInterval(t);
  }, [waiting, onChanged]);

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.addZone(name);
      setName("");
      onChanged();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const checkNow = async (zone: Zone) => {
    setChecking(zone.id);
    setError(null);
    try {
      await api.checkZone(zone.id);
      onChanged();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setChecking(null);
    }
  };

  const checkCertNow = async (zone: Zone) => {
    setChecking(zone.id);
    await onCheckCert(zone.name);
    setChecking(null);
  };

  return (
    <Drawer title="Domains" onClose={onClose}>
      <div className="seg tabs" role="group" aria-label="Add a domain">
        <button className={mode === "own" ? "on" : ""} onClick={() => setMode("own")}>I have a domain</button>
        <button className={mode === "free" ? "on" : ""} onClick={() => setMode("free")}>Get a free domain</button>
      </div>

      {mode === "own" ? (
        <form className="stack" onSubmit={add}>
          <div className="field">
            <span>Add a domain you own</span>
            <div className="add-domain">
              <input autoFocus placeholder="example.com" value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
              <button className="primary" disabled={busy || !name.trim()}>{busy ? "Adding…" : "Add domain"}</button>
            </div>
          </div>
          <p className="muted small">
            It's added to {accountName ?? "your Cloudflare account"}. Cloudflare then gives you two nameservers to set where you bought the domain.
          </p>
          {error && <p className="error">{error}</p>}
        </form>
      ) : (
        <FreeDomain
          zones={zones}
          certs={certs}
          onChanged={onChanged}
          onCheckCert={onCheckCert}
          onUseOwn={(n) => { setName(n); setMode("own"); }}
        />
      )}

      {zones.length === 0 ? (
        <p className="muted">No domains in this account yet.</p>
      ) : (
        <ul className="domains">
          {zones.map((z) => {
            const certPending = z.status === "active" && certs[z.name] === "pending";
            return (
            <li key={z.id}>
              <div className="row between">
                <strong>{z.name}</strong>
                <span className={`badge ${z.status === "active" && !certPending ? "on" : "off"}`}>
                  {certPending ? "Waiting for HTTPS certificate" : ZONE_STATUS[z.status] ?? z.status}
                </span>
              </div>
              {certPending && (
                <div className="ns-steps">
                  <p>
                    The nameservers are set. Cloudflare is now issuing the free HTTPS certificate for <strong>{z.name}</strong> and its
                    subdomains. Routes on it can't load until then.
                  </p>
                  <div className="row">
                    <button className="small" disabled={checking === z.id} onClick={() => checkCertNow(z)}>
                      {checking === z.id ? "Checking…" : "Check now"}
                    </button>
                    <span className="muted small">Usually 15 minutes to a few hours. PortHop checks every 30 seconds.</span>
                  </div>
                </div>
              )}
              {z.status !== "active" && (
                <div className="ns-steps">
                  {isFreeDomain(z.name) ? (
                    <p>
                      In the <a href="#" onClick={(e) => (e.preventDefault(), openUrl(DIGITALPLAT_URL))}>DigitalPlat dashboard</a> → Domain List →{" "}
                      <strong>{z.name}</strong>, set these nameservers:
                    </p>
                  ) : (
                    <p>At your domain registrar, replace the nameservers for <strong>{z.name}</strong> with:</p>
                  )}
                  <NameServers zone={z} />
                  {/* A new free domain has no records to lose; a domain someone already uses might. */}
                  {!isFreeDomain(z.name) && (
                    <p className="warn small">
                      Before you switch, make sure the DNS records your website and email use are in Cloudflare (
                      <a href="#" onClick={(e) => (e.preventDefault(), openUrl(`https://dash.cloudflare.com/${accountId}/${z.name}/dns/records`))}>review DNS records</a>
                      ). Anything missing stops working once the nameservers change.
                    </p>
                  )}
                  <div className="row">
                    <button className="small" disabled={checking === z.id} onClick={() => checkNow(z)}>
                      {checking === z.id ? "Checking…" : "Check now"}
                    </button>
                    <span className="muted small">PortHop re-checks every 30 seconds. Nameserver changes can take a few hours.</span>
                  </div>
                </div>
              )}
            </li>
            );
          })}
        </ul>
      )}
    </Drawer>
  );
}

function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setDone(true);
      setTimeout(() => setDone(false), 1500);
    } catch { /* the text is still on screen to copy by hand */ }
  };
  // type="button" so it never submits a form it sits in.
  return <button type="button" className="ghost small" onClick={copy}>{done ? "Copied" : label}</button>;
}

function NameServers({ zone }: { zone: Zone }) {
  return (
    <ul className="ns">
      {zone.name_servers.map((n) => <li key={n}><code>{n}</code><CopyButton text={n} /></li>)}
    </ul>
  );
}

const LABEL = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;

/** Gets a free DigitalPlat domain: register it there, add it to Cloudflare, then point it at Cloudflare's nameservers. */
function FreeDomain({ zones, certs, onChanged, onCheckCert, onUseOwn }: {
  zones: Zone[];
  certs: Record<string, CertStatus>;
  onChanged: () => void;
  onCheckCert: (name: string) => Promise<void>;
  onUseOwn: (name: string) => void;
}) {
  const [label, setLabel] = useState("");
  const [suffix, setSuffix] = useState(FREE_SUFFIXES[0]);
  const [hint, setHint] = useState<Availability | "checking" | null>(null);
  const [added, setAdded] = useState<Zone | null>(null);
  const [opened, setOpened] = useState<"copied" | "opened" | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const name = `${label}.${suffix}`;
  const valid = LABEL.test(label);
  // The list refreshes a moment after adding; until then, use what add_zone returned.
  const zone = zones.find((z) => z.name === name) ?? (added?.name === name ? added : undefined);
  const inAccount = Boolean(zone);
  const active = zone?.status === "active";
  // Ready = nameservers active and the HTTPS certificate live ("unknown" doesn't block, like the route picker).
  const ready = active && (certs[name] === "ready" || certs[name] === "unknown");
  // Steps only make sense for a name the user can still get, or one they're part-way through setting up.
  const showSteps = valid && (inAccount ? !ready : hint === "available" || hint === "unknown");

  // Hint whether the name is free once typing pauses. Not needed for names already in the account.
  useEffect(() => {
    setHint(null);
    if (!valid || inAccount) return;
    let current = true;
    const t = setTimeout(() => {
      setHint("checking");
      api.checkFreeDomain(label, suffix)
        .then((a) => current && setHint(a))
        .catch(() => current && setHint("unknown"));
    }, 600);
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [label, suffix, valid, inAccount]);

  const addToCloudflare = async () => {
    setBusy(true);
    setError(null);
    try {
      setAdded(await api.addZone(name));
      onChanged();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const register = async () => {
    // DigitalPlat's form takes the name and the suffix separately, so copy just the name.
    const copied = await navigator.clipboard.writeText(label).then(() => true, () => false);
    await openUrl(DIGITALPLAT_URL);
    setOpened(copied ? "copied" : "opened");
  };

  const checkNow = async () => {
    if (!zone) return;
    setError(null);
    try {
      await api.checkZone(zone.id);
      onChanged();
    } catch (err) {
      setError(errorText(err));
    }
  };

  const hintLine =
    !label ? null
    : !valid ? <p className="hint bad">Use letters, numbers and hyphens, not starting or ending with a hyphen.</p>
    : inAccount ? (
      <p className="hint ok">
        {ready
          ? `${name} is ready in your Cloudflare account. Pick it when you add a route.`
          : `${name} is already in your Cloudflare account. Finish setting it up below.`}
      </p>
    )
    : hint === "checking" ? <p className="hint muted">Checking {name}…</p>
    : hint === "available" ? <p className="hint ok">● {name} looks available. This is a hint: a name registered without nameservers also looks free.</p>
    : hint === "taken" ? (
      <p className="hint bad">
        ● {name} is already registered. Try another name, or if it's yours,{" "}
        <a href="#" onClick={(e) => (e.preventDefault(), onUseOwn(name))}>add it as a domain you own</a>.
      </p>
    )
    : hint === "unknown" ? <p className="hint muted">Couldn't check {name}. You can still try.</p>
    : null;

  return (
    <div className="stack free">
      <div className="field">
        <span>Pick a name</span>
        <div className="add-domain">
          <input
            autoFocus
            placeholder="myapp"
            value={label}
            onChange={(e) => {
              setLabel(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""));
              setOpened(null);
            }}
            spellCheck={false}
          />
          <select value={suffix} onChange={(e) => { setSuffix(e.target.value); setOpened(null); }}>
            {FREE_SUFFIXES.map((s) => <option key={s} value={s}>.{s}</option>)}
          </select>
        </div>
        {hintLine}
      </div>

      {showSteps && (
        <ol className="steps">
          <li className={opened || inAccount ? "done" : ""}>
            <div className="row between">
              <strong>Register {name} on DigitalPlat</strong>
              {!inAccount && (
                <button className={opened ? "small" : "primary small"} onClick={register}>Copy name &amp; open DigitalPlat ↗</button>
              )}
            </div>
            <p className="muted small">
              {opened === "copied" && `Copied “${label}”. `}Paste the name and choose .{suffix}. You need a free DigitalPlat account.
              If DigitalPlat asks for nameservers now, do step 2 first and use the two it gives you.
            </p>
          </li>
          <li className={inAccount ? "done" : ""}>
            <div className="row between">
              <strong>Add it to Cloudflare</strong>
              {!inAccount && (
                <button className="primary small" disabled={busy} onClick={addToCloudflare}>{busy ? "Adding…" : "Add"}</button>
              )}
            </div>
            {zone ? (
              <>
                <p className="muted small">Cloudflare assigned these nameservers:</p>
                <NameServers zone={zone} />
              </>
            ) : (
              <p className="muted small">Cloudflare gives you two nameservers to set on DigitalPlat.</p>
            )}
          </li>
          <li className={active ? "done" : ""}>
            <div className="row between">
              <strong>{active ? "Nameservers are set" : "Set the nameservers on DigitalPlat"}</strong>
              {zone && !active && <button className="small" onClick={checkNow}>Check now</button>}
            </div>
            <p className="muted small">
              {active ? (
                `Cloudflare now answers for ${name}.`
              ) : (
                <>
                  In the <a href="#" onClick={(e) => (e.preventDefault(), openUrl(DIGITALPLAT_URL))}>DigitalPlat dashboard</a> → Domain List → {name},
                  enter the two nameservers from step 2. PortHop checks every 30 seconds; it usually takes a few minutes.
                </>
              )}
            </p>
          </li>
          <li className={ready ? "done" : ""}>
            <div className="row between">
              <strong>{ready ? `${name} is ready` : "Cloudflare issues the HTTPS certificate"}</strong>
              {active && !ready && <button className="small" onClick={() => onCheckCert(name)}>Check now</button>}
            </div>
            <p className="muted small">
              {ready
                ? "Pick it when you add a route."
                : active
                  ? "Usually 15 minutes to a few hours after the nameservers are set. PortHop checks every 30 seconds."
                  : "Starts once the nameservers are set."}
            </p>
          </li>
        </ol>
      )}

      {error && <p className="error">{error}</p>}
      <p className="muted small">
        DigitalPlat is a nonprofit that gives out free domains. Renew yours there (it's free) before it expires.
      </p>
    </div>
  );
}

function Settings({ snap, onClose, onSignedOut }: { snap: Snapshot; onClose: () => void; onSignedOut: () => void }) {
  const [autostart, setAutostart] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [showLicense, setShowLicense] = useState(false);
  const [theme, setTheme] = useState(loadTheme);
  const chooseTheme = (choice: ThemeChoice) => {
    saveTheme(choice);
    setTheme(choice);
  };

  useEffect(() => void autostartEnabled().then(setAutostart).catch(() => {}), []);

  const toggleAutostart = async () => {
    autostart ? await disableAutostart() : await enableAutostart();
    setAutostart(await autostartEnabled());
  };

  const update = async () => {
    setUpdateMsg("Checking…");
    try {
      const u = await checkUpdate();
      if (!u) return setUpdateMsg("You're on the latest version.");
      setUpdateMsg(`Installing ${u.version}…`);
      await u.downloadAndInstall();
      await relaunch();
    } catch (e) {
      setUpdateMsg(`Update check failed: ${errorText(e)}`);
    }
  };

  return (
    <Drawer title="Settings" onClose={onClose}>
      <div className="stack">
        <div className="field">
          <span>Theme</span>
          <div className="seg tabs" role="group" aria-label="Theme">
            {(["system", "light", "dark"] as const).map((t) => (
              <button key={t} className={theme === t ? "on" : ""} onClick={() => chooseTheme(t)}>
                {t === "system" ? "System" : t === "light" ? "Light" : "Dark"}
              </button>
            ))}
          </div>
          <p className="muted small">System follows your computer's light or dark setting.</p>
        </div>
        <label className="toggle"><input type="checkbox" checked={autostart} onChange={toggleAutostart} /> Start PortHop when I log in</label>
        {snap.updatesConfigured && (
          <div className="row"><button onClick={update}>Check for updates</button>{updateMsg && <span className="muted small">{updateMsg}</span>}</div>
        )}
        <p className="muted small">Bundled cloudflared {snap.cloudflaredVersion}</p>
        <button className="ghost" onClick={() => setShowLicense(!showLicense)}>Open-source licenses</button>
        {showLicense && <pre className="logs license">cloudflared — Apache License 2.0{"\n\n"}{cloudflaredLicense}</pre>}
        <hr />
        <button className="danger" onClick={async () => { if (confirm("Stop the tunnel and remove the saved token from this computer? Nothing is deleted in Cloudflare.")) { await api.signOut(); onSignedOut(); } }}>
          Sign out
        </button>
      </div>
    </Drawer>
  );
}
