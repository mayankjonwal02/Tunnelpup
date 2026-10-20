export const en = {
  setup: {
    title: "TunnelPup",
    description: "Put localhost:3000 on app.yourdomain.com, through Cloudflare.",
    readGuide: "New here? Read the guide",
    connectAccount: "Connect your Cloudflare account",
    createToken: {
      title: "Create a token on Cloudflare",
      button: "Create token ↗",
      descriptionBeforePermission:
        "The form opens with DNS, Zone and Account Settings filled in. Add one more permission, ",
      permission: "Account → Cloudflare Tunnel → Edit",
      descriptionAfterPermission:
        ", then choose Continue to summary → Create Token.",
    },
    pasteToken: {
      title: "Paste it here",
      placeholder: "Paste token",
      checking: "Checking…",
      connect: "Connect",
    },
    permissions: {
      title: "Why these permissions?",
      tunnel: {
        name: "Account · Cloudflare Tunnel · Edit",
        description: " — create and run this computer's tunnel",
      },
      account: {
        name: "Account · Account Settings · Read",
        description: " — find your Cloudflare account",
      },
      dns: {
        name: "Zone · DNS · Edit",
        description: " — point your subdomains at the tunnel",
      },
      zone: {
        name: "Zone · Zone · Edit",
        description: " — add domains from TunnelPup, including free ones",
      },
    },
    tokenStorage: "The token is stored in your system keychain, never in a file.",
    guideTitle: "Guide",
  },
};
