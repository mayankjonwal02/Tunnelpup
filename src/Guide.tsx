// The in-app guide: how TunnelPup works and how to fix the problems people actually hit.
// Rendered inside a Drawer by App.tsx (from the header and from the setup screen).
import { useEffect } from "react";

export type GuideSection = "how" | "start" | "domains" | "routes" | "troubleshooting" | "security" | "background";

export function GuideContent({ section = "how" }: { section?: GuideSection }) {
  // Open on the requested section, scrolled into view.
  useEffect(() => {
    document.getElementById(`guide-${section}`)?.scrollIntoView({ block: "start" });
  }, [section]);

  const open = (id: GuideSection) => ({ id: `guide-${id}`, open: id === section });

  return (
    <div className="guide">
      <details {...open("how")}>
        <summary>How TunnelPup works</summary>
        <div className="flow" aria-label="A visitor reaches Cloudflare over HTTPS, Cloudflare sends the request through the tunnel to TunnelPup on this computer, and TunnelPup passes it to your app.">
          <span>Visitor</span>
          <i>→</i>
          <span>Cloudflare<small>HTTPS · your domain</small></span>
          <i>→</i>
          <span>Tunnel</span>
          <i>→</i>
          <span>TunnelPup<small>this computer</small></span>
          <i>→</i>
          <span>localhost:3000<small>your app</small></span>
        </div>
        <p>
          TunnelPup runs Cloudflare's connector on this computer. It keeps an outgoing connection open to Cloudflare, so no ports
          are opened on your router and your home IP address stays private. When someone opens <code>app.yourdomain.com</code>,
          Cloudflare sends the request down that connection to the app running on your computer.
        </p>
        <p>Your computer has to be on, with TunnelPup running, for your links to work.</p>
      </details>

      <details {...open("start")}>
        <summary>Getting started</summary>
        <ol>
          <li><strong>Connect Cloudflare.</strong> Create a token (the setup screen fills in most of it) and paste it in. TunnelPup then creates a tunnel for this computer.</li>
          <li><strong>Add a domain.</strong> Use one you own, or get a free one in <em>Domains → Get a free domain</em>.</li>
          <li><strong>Wait for it to go live.</strong> First the nameservers (minutes to a few hours), then Cloudflare's HTTPS certificate (usually 15 minutes to a few hours). Domains shows both.</li>
          <li><strong>Add a route.</strong> Pick a name, a domain and the port your app runs on. TunnelPup sets up the routing and DNS, checks the link loads, and only then lists it.</li>
          <li><strong>Share the link.</strong> Use <em>Copy link</em> next to the route.</li>
        </ol>
      </details>

      <details {...open("domains")}>
        <summary>Domains</summary>
        <p><strong>A domain you own:</strong> add it in Domains, then at the company you bought it from, replace its nameservers with the two Cloudflare gives you. Before switching, make sure the DNS records your website and email use are in Cloudflare, or they stop working.</p>
        <p><strong>A free domain:</strong> open <em>Domains → Get a free domain</em>, pick a name and an ending (<code>.dpdns.org</code>, <code>.us.kg</code>, <code>.qzz.io</code> or <code>.xx.kg</code>) and follow the steps. You register it on DigitalPlat (a free account), add it to Cloudflare, then enter Cloudflare's nameservers in DigitalPlat's dashboard. Free domains must be renewed on DigitalPlat, for free, before they expire.</p>
        <p>A domain is ready when Domains shows it as <em>Active</em>: its nameservers point at Cloudflare and its HTTPS certificate is issued. Until then it's greyed out when you add a route.</p>
      </details>

      <details {...open("routes")}>
        <summary>Routes</summary>
        <p>A route sends <code>name.yourdomain.com</code> to <code>localhost:&lt;port&gt;</code> on this computer. The app has to be running: if it stops, the route shows <em>App not running</em> and visitors get an error page.</p>
        <p>To change a route's port, edit the number and press Enter. Removing a route also deletes its DNS record.</p>
        <p>Some apps, such as Ollama, Vite, Django and Rails, only answer requests addressed to <code>localhost</code>. TunnelPup spots this when you add the route and sends them <code>Host: localhost</code>; the route then shows a <em>Host: localhost</em> tag.</p>
      </details>

      <details {...open("troubleshooting")}>
        <summary>Troubleshooting</summary>
        <dl>
          <dt>"This site can't be reached" right after adding a route</dt>
          <dd>
            Your network looked the name up before it existed and remembers "doesn't exist" for up to 30 minutes. Wait, or turn on
            secure DNS in Chrome (Settings → Privacy and security → Security → Use secure DNS), restart your router, or run{" "}
            <code>ipconfig /flushdns</code>. To avoid it, don't open a link before its route is added.
          </dd>

          <dt>HTTP 403 "Access denied"</dt>
          <dd>
            The app refused the request. If it only answers to localhost, remove the route and add it again so TunnelPup can detect
            that. Otherwise the app itself wants a login or only allows certain sites (for Ollama used from a web page, see its{" "}
            <code>OLLAMA_ORIGINS</code> setting).
          </dd>

          <dt>502 Bad gateway</dt>
          <dd>Nothing answered on the route's port. Start the app; the <em>App not running</em> tag clears by itself.</dd>

          <dt>Cloudflare error 1033 or HTTP 530</dt>
          <dd>The tunnel isn't connected. Check the status at the top of TunnelPup, press Start, and look in Logs for the reason.</dd>

          <dt>"Handshake failure" or a certificate error</dt>
          <dd>Cloudflare hasn't issued the domain's HTTPS certificate yet. Domains shows <em>Waiting for HTTPS certificate</em> until it has.</dd>

          <dt>A token or permission error</dt>
          <dd>
            TunnelPup needs Cloudflare Tunnel: Edit, Account Settings: Read, DNS: Edit and Zone: Edit, for all zones. Edit the token in
            Cloudflare's dashboard (you don't need a new one); if TunnelPup already turned it down, paste it again.
          </dd>
        </dl>
      </details>

      <details {...open("security")}>
        <summary>Security</summary>
        <ul>
          <li>Everything you route is public on the internet. Don't route admin panels, databases or dev tools unless they have their own login. For extra protection, put a route behind Cloudflare Access in the Cloudflare dashboard.</li>
          <li>Stop the tunnel when you don't need your links.</li>
          <li>Your API token is kept in the system keychain, never in a file. Signing out removes it from this computer; delete it in Cloudflare to revoke it.</li>
        </ul>
      </details>

      <details {...open("background")}>
        <summary>Running in the background</summary>
        <ul>
          <li>Closing the window keeps TunnelPup running in the system tray, so your links stay up. Choose <em>Quit TunnelPup</em> in the tray menu to stop it completely.</li>
          <li>Turn on <em>Start TunnelPup when I log in</em> in Settings to bring your links back after a restart.</li>
          <li>If the connection drops, TunnelPup reconnects by itself; the status at the top shows when it's retrying.</li>
        </ul>
      </details>
    </div>
  );
}
